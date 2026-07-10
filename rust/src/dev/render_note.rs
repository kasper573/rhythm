//! Renders note-field animation scenarios to mp4 files, so sprite rendering
//! can be reviewed frame by frame without playing the game. Scenarios flip
//! the same state the game's grading flips, and the field's clock is driven
//! manually — one exact step per captured frame — so the output is
//! deterministic whatever the wall-clock frame rate.

use super::note_scenarios::{Scenario, ScriptAction, scenario_matrix};
use crate::core::config::config;
use crate::core::player::PlayerId;
use crate::core::screen::CLEAR_COLOR;
use crate::core::settings::{NoteSpeed, Perspective};
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Bpm, Seconds};
use crate::game::Game;
use crate::nodes::stepfile_player::note_field::{
    FieldClock, FieldLayout, NoteFieldRig, NoteIndex, NoteSpawn, NoteTail, TARGET_Y,
};
use crate::nodes::stepfile_player::note_skin::load_note_skin;
use godot::classes::window::ContentScaleMode;
use godot::classes::{ColorRect, Control, INode, Node};
use godot::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const WIDTH: i32 = 640;
const HEIGHT: i32 = 720;
/// The rendered field's arrow size: the classic in-game proportions.
const RENDER_ARROW_SIZE: f32 = 88.0;
/// Clips start with the first note this far below the receptors: past the
/// bottom edge whatever the scroll speed.
const LEAD_PIXELS: f32 = 760.0;
const TAIL_SECONDS: f64 = 1.2;
/// Frames rendered before each clip so the skin's textures and pipelines
/// are warm; their captures are discarded.
const WARMUP_FRAMES: u32 = 20;

pub(super) struct RenderNoteArgs {
    pub filter: String,
    pub skin: Option<String>,
    pub perspective: String,
    pub bpm: f64,
    pub out: PathBuf,
    pub fps: u32,
}

pub(super) fn start(game: &mut Game, args: RenderNoteArgs) {
    assert!(args.bpm > 0.0, "--bpm must be positive");
    let scenarios: Vec<Scenario> = scenario_matrix()
        .into_iter()
        .filter(|scenario| args.filter == "all" || scenario.name.contains(&args.filter))
        .collect();
    if scenarios.is_empty() {
        eprintln!(
            "no scenario matches {:?}; use --list to see all",
            args.filter
        );
        game.base().get_tree().quit_ex().exit_code(1).done();
        return;
    }
    std::fs::create_dir_all(&args.out).expect("failed to create the output directory");

    let mut window = game.base().get_window().expect("the game runs in a window");
    window.set_content_scale_mode(ContentScaleMode::DISABLED);
    window.set_size(Vector2i::new(WIDTH, HEIGHT));

    let mut driver = RenderNoteDriver::new_alloc();
    {
        let mut bound = driver.bind_mut();
        bound.scenarios = scenarios;
        bound.args = Some(args);
    }
    game.base_mut().add_child(&driver);
}

#[derive(GodotClass)]
#[class(base=Node)]
struct RenderNoteDriver {
    scenarios: Vec<Scenario>,
    args: Option<RenderNoteArgs>,
    host: Option<Gd<Control>>,
    clip: Option<Clip>,
    current: usize,
    warmup: u32,
    base: Base<Node>,
}

/// One scenario mid-render: its rig, manual clock, scripted actions, and
/// the encoder eating the captured frames.
struct Clip {
    rig: NoteFieldRig,
    timing: StepfileTiming,
    start: Seconds,
    frame_count: u32,
    frame: u32,
    notes: Vec<(NoteIndex, usize)>,
    mines: Vec<(crate::nodes::stepfile_player::note_field::MineIndex, usize)>,
    script: Vec<(Seconds, ScriptAction)>,
    next_action: usize,
    encoder: FfmpegEncoder,
    path: PathBuf,
}

impl RenderNoteDriver {
    fn begin_clip(&mut self) {
        let (bpm, fps, skin_arg, perspective_arg, out) = {
            let args = self.args.as_ref().expect("driver started with args");
            (
                args.bpm,
                args.fps,
                args.skin.clone(),
                args.perspective.clone(),
                args.out.clone(),
            )
        };
        let scenario = &self.scenarios[self.current];
        let timing = scenario_timing(scenario, bpm);
        let scenario_index = self.current;

        // A fresh full-window host per clip: backdrop plus the field.
        if let Some(mut host) = self.host.take() {
            host.queue_free();
        }
        let mut host = Control::new_alloc();
        host.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        let mut backdrop = ColorRect::new_alloc();
        backdrop.set_color(CLEAR_COLOR);
        backdrop.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        host.add_child(&backdrop);
        self.base_mut().add_child(&host);
        let scenario = &self.scenarios[scenario_index];

        let defaults = &config().defaults.player_options;
        let skin_name = skin_arg.unwrap_or_else(|| defaults.note_skin.clone());
        let perspective: Perspective = perspective_arg
            .parse()
            .expect("--perspective must be None, Above, or Below");
        let layout = FieldLayout {
            player: PlayerId::P1,
            origin_x: 0.0,
            columns: 4,
            speed: defaults.note_speed,
            arrow_size: RENDER_ARROW_SIZE,
        };
        let lane_camera = &config().lane_camera;
        let mut rig = NoteFieldRig::build(
            &mut host,
            layout,
            load_note_skin(&skin_name),
            perspective,
            lane_camera.fov_degrees,
            lane_camera.tilt_degrees,
            Vector2::new(WIDTH as f32, HEIGHT as f32),
        );

        let mut notes = Vec::new();
        for note in &scenario.notes {
            let time = timing.seconds_at_beat(Beat(note.beat));
            let tail = note.length_beats.map(|length| {
                let end_beat = Beat(note.beat + length);
                NoteTail {
                    time: timing.seconds_at_beat(end_beat),
                    beat: end_beat,
                    roll: note.roll,
                }
            });
            let index = rig.spawn_note(&NoteSpawn {
                time,
                beat: Beat(note.beat),
                column: note.column,
                quant: config().recognized_quant(note.quant),
                tail,
            });
            notes.push((index, note.column));
        }
        let mut mines = Vec::new();
        for mine in &scenario.mines {
            let time = timing.seconds_at_beat(Beat(mine.beat));
            let index = rig.spawn_mine(time, Beat(mine.beat), mine.column);
            mines.push((index, mine.column));
        }

        let mut script: Vec<(Seconds, ScriptAction)> = scenario
            .script
            .iter()
            .map(|(beat, action)| (timing.seconds_at_beat(Beat(*beat)), *action))
            .collect();
        script.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));

        let (start, end) = clip_window(scenario, &timing, defaults.note_speed);
        let frame_count = ((end.0 - start.0) * fps as f64).ceil() as u32;
        let path = out.join(format!("{}.mp4", scenario.name));
        let encoder = FfmpegEncoder::start(&path, fps);

        self.host = Some(host);
        self.warmup = WARMUP_FRAMES;
        self.clip = Some(Clip {
            rig,
            timing,
            start,
            frame_count,
            frame: 0,
            notes,
            mines,
            script,
            next_action: 0,
            encoder,
            path,
        });
    }

    /// Captures the frame the renderer just finished — the state advanced
    /// by the PREVIOUS process tick — so content and captures stay exactly
    /// one-to-one however the engine paces itself.
    fn capture(&mut self) -> Vec<u8> {
        let viewport = self
            .base()
            .get_viewport()
            .expect("the driver lives in the tree");
        let texture = viewport.get_texture().expect("the viewport renders");
        let mut image = texture.get_image().expect("the viewport has an image");
        image.convert(godot::classes::image::Format::RGBA8);
        image.get_data().to_vec()
    }
}

#[godot_api]
impl INode for RenderNoteDriver {
    fn init(base: Base<Node>) -> RenderNoteDriver {
        RenderNoteDriver {
            scenarios: Vec::new(),
            args: None,
            host: None,
            clip: None,
            current: 0,
            warmup: 0,
            base,
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.clip.is_none() {
            self.begin_clip();
            return;
        }
        if self.warmup > 0 {
            self.warmup -= 1;
            if self.warmup > 0 {
                let Some(clip) = &mut self.clip else { return };
                let clock = FieldClock {
                    visible: clip.start,
                    timing: clip.timing.clone(),
                    target_y: TARGET_Y,
                };
                clip.rig.update(&clock, 0.0);
                return;
            }
        }

        let fps = self.args.as_ref().expect("driver started with args").fps;
        let frame_data = if self.clip.as_ref().is_some_and(|clip| clip.frame > 0) {
            Some(self.capture())
        } else {
            None
        };
        let Some(clip) = &mut self.clip else { return };
        if let Some(data) = frame_data {
            clip.encoder.push(&data);
        }

        if clip.frame >= clip.frame_count {
            // The final frame's capture arrived above; seal the clip.
            let encoder = std::mem::replace(&mut clip.encoder, FfmpegEncoder::finished());
            encoder.finish(&clip.path);
            println!(
                "wrote {} ({} frames)",
                clip.path.display(),
                clip.frame_count
            );
            self.clip = None;
            self.current += 1;
            if self.current >= self.scenarios.len() {
                self.base().get_tree().quit();
            }
            return;
        }

        let now = Seconds(clip.start.0 + clip.frame as f64 / fps as f64);
        while clip.next_action < clip.script.len() && clip.script[clip.next_action].0.0 <= now.0 {
            let action = clip.script[clip.next_action].1;
            apply_action(clip, action);
            clip.next_action += 1;
        }
        let clock = FieldClock {
            visible: now,
            timing: clip.timing.clone(),
            target_y: TARGET_Y,
        };
        clip.rig.update(&clock, 1.0 / fps as f32);
        clip.frame += 1;
    }
}

fn apply_action(clip: &mut Clip, action: ScriptAction) {
    // The vanish flash plays in the best grade's color.
    let flash_color = config()
        .grading
        .dynamic
        .iter()
        .find_map(|grade| grade.arrow_flash)
        .unwrap_or(Color::WHITE);
    match action {
        ScriptAction::Hold(index, state) => {
            clip.rig.set_hold_state(clip.notes[index].0, state);
        }
        ScriptAction::Fade(index) => {
            clip.rig.fade_out_note(
                clip.notes[index].0,
                crate::nodes::stepfile_player::note_field::HOLD_OK_FADE_SECONDS,
            );
        }
        ScriptAction::Vanish(index) => {
            let (note, column) = clip.notes[index];
            clip.rig.vanish_note(note);
            clip.rig.arrow_flash(column, TARGET_Y, flash_color, false);
        }
        ScriptAction::Press(column, held) => {
            clip.rig.set_receptor_held(column, held);
        }
        ScriptAction::ExplodeMine(index) => {
            let (mine, column) = clip.mines[index];
            clip.rig.remove_mine(mine);
            clip.rig.mine_explosion(column, TARGET_Y);
        }
    }
}

fn scenario_timing(scenario: &Scenario, cli_bpm: f64) -> StepfileTiming {
    let bpms: Vec<(Beat, Bpm)> = if scenario.bpms.is_empty() {
        vec![(Beat(0.0), Bpm(cli_bpm))]
    } else {
        scenario
            .bpms
            .iter()
            .map(|(beat, bpm)| (Beat(*beat), Bpm(*bpm)))
            .collect()
    };
    let stops: Vec<(Beat, Seconds)> = scenario
        .stops
        .iter()
        .map(|(beat, seconds)| (Beat(*beat), Seconds(*seconds)))
        .collect();
    StepfileTiming::new(Seconds(0.0), &bpms, &stops)
}

/// The clip runs from a lead-in before the first thing on the timeline to a
/// tail after the last, so every note scrolls in from below and every fade
/// and popup finishes on screen. The lead is a fixed distance in pixels, so
/// it adapts to whatever the scroll speed is.
fn clip_window(
    scenario: &Scenario,
    timing: &StepfileTiming,
    speed: NoteSpeed,
) -> (Seconds, Seconds) {
    let mut first = f64::INFINITY;
    let mut last = f64::NEG_INFINITY;
    let mut cover = |beat: f64| {
        first = first.min(beat);
        last = last.max(beat);
    };
    for note in &scenario.notes {
        cover(note.beat);
        cover(note.beat + note.length_beats.unwrap_or(0.0));
    }
    for mine in &scenario.mines {
        cover(mine.beat);
    }
    for (beat, _) in &scenario.script {
        cover(*beat);
    }
    assert!(first.is_finite(), "scenario has an empty timeline");
    let lead_arrows = (LEAD_PIXELS / RENDER_ARROW_SIZE) as f64;
    let start = match speed {
        NoteSpeed::Constant(scroll_bpm) => {
            timing.seconds_at_beat(Beat(first)) - Seconds(lead_arrows * 60.0 / scroll_bpm as f64)
        }
        NoteSpeed::Dynamic(multiplier) => {
            timing.seconds_at_beat(Beat(first - lead_arrows / multiplier as f64))
        }
    };
    (
        start,
        timing.seconds_at_beat(Beat(last)) + Seconds(TAIL_SECONDS),
    )
}

/// Encodes raw rgba frames into an mp4 through a piped ffmpeg process;
/// frames arrive strictly in order from the manual clock.
struct FfmpegEncoder {
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    written: u32,
}

impl FfmpegEncoder {
    fn start(path: &Path, fps: u32) -> FfmpegEncoder {
        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                &format!("{WIDTH}x{HEIGHT}"),
                "-r",
                &fps.to_string(),
                "-i",
                "-",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "18",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("failed to start ffmpeg; is it installed?");
        let stdin = child.stdin.take();
        FfmpegEncoder {
            child: Some(child),
            stdin,
            written: 0,
        }
    }

    fn finished() -> FfmpegEncoder {
        FfmpegEncoder {
            child: None,
            stdin: None,
            written: 0,
        }
    }

    fn push(&mut self, data: &[u8]) {
        assert_eq!(
            data.len(),
            (WIDTH * HEIGHT * 4) as usize,
            "captured frame has unexpected size"
        );
        let stdin = self.stdin.as_mut().expect("encoder already finished");
        stdin.write_all(data).expect("ffmpeg pipe closed early");
        self.written += 1;
    }

    fn finish(mut self, path: &Path) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let status = child.wait().expect("ffmpeg did not run");
            assert!(status.success(), "ffmpeg failed for {}", path.display());
        }
    }
}
