//! Renders note-field animation scenarios to mp4 files, so sprite rendering
//! can be reviewed frame by frame without playing the game. Scenarios flip
//! the same state components the game's grading systems flip.
//!
//! ```text
//! cargo run --bin render_note all --skin ddrextreme_default --bpm 125
//! cargo run --bin render_note hold_quant_16
//! cargo run --bin render_note --list
//! ```

use bevy::app::SubApps;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{PollType, TextureFormat, TextureUsages};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use clap::Parser;
use rhythm::core::config::GameConfig;
use rhythm::core::note_field::{
    FadeOut, HOLD_OK_FADE_SECONDS, HoldPart, HoldVisual, HoldVisualState, InColumn, LaneEffects,
    LaneView, MineNote, NoteArrow, NoteField, NoteFieldClock, NoteFieldPlugin, NoteSpawn,
    NoteSpeed, NoteTail, Perspective, Receptor, SpawnedNote, TARGET_Y, spawn_mine, spawn_note,
    spawn_receptors,
};
use rhythm::core::note_skin::{ActiveNoteSkins, load_note_skin};
use rhythm::core::player::PlayerId;
use rhythm::core::settings::{PlayerOptions, PlayerSettings};
use rhythm::core::stepfile::StepfileTiming;
use rhythm::core::units::{Beat, Seconds};
use rhythm::core::{CLEAR_COLOR, OVERLAY_CAMERA_ORDER, OVERLAY_LAYER};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

#[derive(Parser)]
#[command(about = "Render note-field animation scenarios to mp4 files")]
struct Cli {
    /// "all", or a substring matched against scenario names
    #[arg(default_value = "all")]
    filter: String,
    /// Note skin to render; defaults to the game config's default
    #[arg(long)]
    skin: Option<String>,
    /// Lane camera perspective: None, Above, or Below
    #[arg(long, default_value = "None")]
    perspective: String,
    #[arg(long, default_value_t = 120.0)]
    bpm: f64,
    /// Output directory for the mp4 files
    #[arg(long, default_value = "out")]
    out: PathBuf,
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// List all scenario names and exit
    #[arg(long)]
    list: bool,
}

fn main() {
    rhythm::core::platform::install(rhythm::native::NativePlatform);
    let cli = Cli::parse();
    assert!(cli.bpm > 0.0, "--bpm must be positive");
    let scenarios = scenario_matrix();
    if cli.list {
        for scenario in &scenarios {
            println!("{}", scenario.name);
        }
        return;
    }
    let selected: Vec<&Scenario> = scenarios
        .iter()
        .filter(|scenario| cli.filter == "all" || scenario.name.contains(&cli.filter))
        .collect();
    if selected.is_empty() {
        eprintln!(
            "no scenario matches \"{}\"; use --list to see all",
            cli.filter
        );
        std::process::exit(1);
    }

    let config = GameConfig::load();
    let defaults = &config.defaults.player_options;
    let options = PlayerOptions {
        note_skin: cli
            .skin
            .clone()
            .unwrap_or_else(|| defaults.note_skin.clone()),
        perspective: cli
            .perspective
            .parse::<Perspective>()
            .expect("--perspective must be None, Above, or Below"),
        ..defaults.clone()
    };
    std::fs::create_dir_all(&cli.out).expect("failed to create the output directory");

    let mut renderer = FieldRenderer::new(&config, &options, cli.fps);
    for scenario in selected {
        let timing = scenario_timing(scenario, cli.bpm);
        let path = cli.out.join(format!("{}.mp4", scenario.name));
        let frames = renderer.render(scenario, &config, &timing, &path);
        println!("wrote {} ({frames} frames)", path.display());
    }
}

const WIDTH: u32 = 640;
const HEIGHT: u32 = 720;
/// The rendered field's arrow size: the classic in-game proportions.
const RENDER_ARROW_SIZE: f32 = 88.0;
/// Clips start with the first note this far below the receptors: past the
/// bottom edge whatever the scroll speed.
const LEAD_PIXELS: f32 = 760.0;
const TAIL_SECONDS: f64 = 1.2;

struct Scenario {
    name: String,
    notes: Vec<ScenarioNote>,
    mines: Vec<ScenarioMine>,
    script: Vec<(f64, ScriptAction)>,
    /// `(beat, bpm)` changes; empty means the CLI `--bpm` throughout.
    bpms: Vec<(f64, f64)>,
    /// `(beat, seconds)` stops.
    stops: Vec<(f64, f64)>,
}

/// Everything a scenario leaves on the field between runs.
type AnyFieldEntity = Or<(
    With<NoteArrow>,
    With<HoldPart>,
    With<MineNote>,
    With<Receptor>,
    With<FadeOut>,
    With<Screenshot>,
)>;

fn scenario_timing(scenario: &Scenario, cli_bpm: f64) -> StepfileTiming {
    let bpms: Vec<(Beat, f64)> = if scenario.bpms.is_empty() {
        vec![(Beat(0.0), cli_bpm)]
    } else {
        scenario
            .bpms
            .iter()
            .map(|(beat, bpm)| (Beat(*beat), *bpm))
            .collect()
    };
    let stops: Vec<(Beat, Seconds)> = scenario
        .stops
        .iter()
        .map(|(beat, seconds)| (Beat(*beat), Seconds(*seconds)))
        .collect();
    StepfileTiming::new(Seconds(0.0), &bpms, &stops)
}

struct ScenarioNote {
    beat: f64,
    column: usize,
    quant: u32,
    length_beats: Option<f64>,
    roll: bool,
}

struct ScenarioMine {
    beat: f64,
    column: usize,
}

/// A scripted stand-in for the gameplay systems that drive the field.
#[derive(Clone, Copy)]
enum ScriptAction {
    /// Set the render state of the scenario's i-th note's hold.
    Hold(usize, HoldVisualState),
    /// Apply the hold-OK fade to the i-th note's head.
    Fade(usize),
    /// Vanish the i-th note at the receptor: despawn it and play the arrow
    /// flash on its column, as grading does for taps.
    Vanish(usize),
    /// Press or release a receptor's panel.
    Press(usize, bool),
    /// Blow up the i-th mine.
    ExplodeMine(usize),
}

fn scenario_matrix() -> Vec<Scenario> {
    use HoldVisualState::{Dropped, Held, Ok, Released};
    use ScriptAction::{ExplodeMine, Fade, Hold, Press, Vanish};

    const QUANTS: [u32; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
    let mut all = Vec::new();
    let mut add = |name: &str,
                   notes: Vec<ScenarioNote>,
                   mines: Vec<ScenarioMine>,
                   script: Vec<(f64, ScriptAction)>| {
        all.push(Scenario {
            name: name.to_string(),
            notes,
            mines,
            script,
            bpms: Vec::new(),
            stops: Vec::new(),
        });
    };

    for quant in QUANTS {
        add(
            &format!("single_quant_{quant}"),
            vec![note(0.0, 1, quant, None)],
            vec![],
            vec![],
        );
    }

    for quant in QUANTS {
        for (label, length) in [
            ("half_beat", 0.5),
            ("one_beat", 1.0),
            ("two_and_a_half_beats", 2.5),
        ] {
            add(
                &format!("hold_quant_{quant}_{label}"),
                vec![note(0.0, 1, quant, Some(length))],
                vec![],
                vec![],
            );
        }
    }

    add(
        "hold_held_to_ok",
        vec![note(0.0, 1, 4, Some(2.0))],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (2.0, Hold(0, Ok)),
            (2.0, Fade(0)),
            (2.0, Press(1, false)),
        ],
    );
    add(
        "hold_released_and_regrabbed",
        vec![note(0.0, 1, 4, Some(3.0))],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (1.0, Press(1, false)),
            (1.0, Hold(0, Released)),
            (1.75, Press(1, true)),
            (1.75, Hold(0, Held)),
            (3.0, Hold(0, Ok)),
            (3.0, Fade(0)),
            (3.0, Press(1, false)),
        ],
    );
    add(
        "hold_dropped_midway",
        vec![note(0.0, 1, 4, Some(3.0))],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (1.0, Press(1, false)),
            (1.0, Hold(0, Released)),
            (1.5, Hold(0, Dropped)),
        ],
    );
    add(
        "hold_head_missed",
        vec![note(0.0, 1, 4, Some(2.0))],
        vec![],
        vec![(0.5, Hold(0, Dropped))],
    );
    add("roll_two_beats", vec![roll(0.0, 1, 4, 2.0)], vec![], vec![]);
    add(
        "roll_held_to_ok",
        vec![roll(0.0, 1, 4, 2.0)],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (2.0, Hold(0, Ok)),
            (2.0, Fade(0)),
            (2.0, Press(1, false)),
        ],
    );
    add(
        "hold_chain_one_column",
        vec![
            note(0.0, 1, 4, Some(0.5)),
            note(1.0, 1, 4, Some(0.5)),
            note(2.0, 1, 4, Some(0.5)),
        ],
        vec![],
        vec![],
    );
    add(
        "hold_staircase",
        vec![
            note(0.0, 0, 4, Some(1.0)),
            note(0.5, 1, 8, Some(1.0)),
            note(1.0, 2, 4, Some(1.0)),
            note(1.5, 3, 8, Some(1.0)),
        ],
        vec![],
        vec![],
    );
    add(
        "jump_hold",
        vec![note(0.0, 1, 4, Some(2.0)), note(0.0, 2, 4, Some(2.0))],
        vec![],
        vec![],
    );

    add(
        "tap_vanish_at_receptor",
        vec![note(0.0, 1, 4, None)],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Vanish(0)),
            (0.4, Press(1, false)),
        ],
    );
    add(
        "jump",
        vec![note(0.0, 0, 4, None), note(0.0, 3, 4, None)],
        vec![],
        vec![],
    );
    add(
        "every_column",
        vec![
            note(0.0, 0, 4, None),
            note(1.0, 1, 4, None),
            note(2.0, 2, 4, None),
            note(3.0, 3, 4, None),
        ],
        vec![],
        vec![],
    );
    add(
        "stream_16ths",
        (0..16)
            .map(|i| {
                let quant = [4, 16, 8, 16][i % 4];
                note(i as f64 * 0.25, i % 4, quant, None)
            })
            .collect(),
        vec![],
        vec![],
    );

    add("mine", vec![], vec![mine(0.0, 1)], vec![]);
    add(
        "mine_row",
        vec![],
        (0..4).map(|column| mine(0.0, column)).collect(),
        vec![],
    );
    add(
        "mine_exploding",
        vec![],
        vec![mine(0.0, 1)],
        vec![
            (-0.5, Press(1, true)),
            (0.0, ExplodeMine(0)),
            (0.5, Press(1, false)),
        ],
    );

    add(
        "receptors_idle",
        vec![],
        vec![],
        vec![
            (1.0, Press(1, true)),
            (2.0, Press(1, false)),
            (3.0, Press(2, true)),
            (4.0, Press(2, false)),
        ],
    );

    // Tempo gimmicks: under Dynamic speed the spacing per beat must stay
    // uniform while the scroll rate doubles, and a stop must freeze the
    // field; under Constant speed the spacing itself changes instead.
    all.push(Scenario {
        name: "stream_bpm_change".to_string(),
        notes: (0..8).map(|i| note(i as f64, i % 4, 4, None)).collect(),
        mines: vec![],
        script: vec![],
        bpms: vec![(0.0, 125.0), (4.0, 250.0)],
        stops: vec![],
    });
    all.push(Scenario {
        name: "stream_stop".to_string(),
        notes: (0..8).map(|i| note(i as f64, i % 4, 4, None)).collect(),
        mines: vec![],
        script: vec![],
        bpms: vec![],
        stops: vec![(4.0, 1.0)],
    });

    all
}

fn note(beat: f64, column: usize, quant: u32, length_beats: Option<f64>) -> ScenarioNote {
    ScenarioNote {
        beat,
        column,
        quant,
        length_beats,
        roll: false,
    }
}

fn roll(beat: f64, column: usize, quant: u32, length_beats: f64) -> ScenarioNote {
    ScenarioNote {
        roll: true,
        ..note(beat, column, quant, Some(length_beats))
    }
}

fn mine(beat: f64, column: usize) -> ScenarioMine {
    ScenarioMine { beat, column }
}

/// A headless bevy app rendering one P1 note field into an image target,
/// one fixed-step frame at a time.
struct FieldRenderer {
    apps: SubApps,
    target: Handle<Image>,
    sender: Sender<(u32, Vec<u8>)>,
    frames: Receiver<(u32, Vec<u8>)>,
    field: Entity,
    layout: NoteField,
    fps: u32,
}

impl FieldRenderer {
    fn new(config: &GameConfig, options: &PlayerOptions, fps: u32) -> FieldRenderer {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(RenderPlugin {
                    // Every frame is captured; the first ones must already
                    // have working render pipelines.
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(NoteFieldPlugin)
        .insert_resource(ClearColor(CLEAR_COLOR))
        .insert_resource(config.clone())
        .insert_resource(PlayerSettings::uniform(options.clone()))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / fps as f64,
        )));
        let skin = load_note_skin(app.world().resource::<AssetServer>(), &options.note_skin);
        app.insert_resource(ActiveNoteSkins::shared(skin));
        app.finish();
        app.cleanup();
        let mut apps = std::mem::take(app.sub_apps_mut());

        let world = apps.main.world_mut();
        let mut target =
            Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
        target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let target = world.resource_mut::<Assets<Image>>().add(target);
        let layout = NoteField {
            player: PlayerId::P1,
            lane: 0,
            origin_x: 0.0,
            columns: 4,
            speed: options.note_speed,
            arrow_size: RENDER_ARROW_SIZE,
            // The field's lane camera draws into the capture image, whose
            // world is the image itself.
            view: LaneView {
                target: RenderTarget::Image(target.clone().into()),
                canvas: Vec2::new(WIDTH as f32, HEIGHT as f32),
            },
        };
        let field = world.spawn(layout.clone()).id();
        // The world and overlay cameras bracketing the lane camera the
        // plugin spawns, all drawing into the capture image. Every camera
        // keeps the default MSAA: cameras sharing a target must agree on it.
        world
            .spawn_scene(bsn! { Camera2d })
            .expect("static scene resolution cannot fail")
            .insert(RenderTarget::Image(target.clone().into()));
        world
            .spawn_scene(bsn! { Camera2d })
            .expect("static scene resolution cannot fail")
            .insert((
                RenderTarget::Image(target.clone().into()),
                Camera {
                    order: OVERLAY_CAMERA_ORDER,
                    clear_color: bevy::camera::ClearColorConfig::None,
                    ..default()
                },
                bevy::camera::visibility::RenderLayers::layer(OVERLAY_LAYER),
            ));

        let (sender, frames) = channel();
        let mut renderer = FieldRenderer {
            apps,
            target,
            sender,
            frames,
            field,
            layout,
            fps,
        };
        renderer.wait_for_skin_assets();
        renderer
    }

    /// Frames rendered before the skin's assets finish loading would show
    /// nothing; pump the app until they are in and uploaded.
    fn wait_for_skin_assets(&mut self) {
        let world = self.apps.main.world();
        let handles = world
            .resource::<ActiveNoteSkins>()
            .get(PlayerId::P1)
            .loading_assets();
        for _ in 0..600 {
            self.update();
            let asset_server = self.apps.main.world().resource::<AssetServer>();
            if handles
                .iter()
                .all(|handle| asset_server.is_loaded(handle.id()))
            {
                // One more frame so the gpu copies exist before capturing.
                self.update();
                return;
            }
        }
        panic!("note skin assets did not finish loading");
    }

    fn update(&mut self) {
        self.apps.update();
        // Wait out the frame's gpu work so capture callbacks can fire.
        self.apps
            .main
            .world()
            .resource::<RenderDevice>()
            .wgpu_device()
            .poll(PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("gpu poll failed");
    }

    /// Renders one scenario and encodes it to `path`. Returns the number of
    /// frames written.
    fn render(
        &mut self,
        scenario: &Scenario,
        config: &GameConfig,
        timing: &StepfileTiming,
        path: &std::path::Path,
    ) -> u32 {
        let (start, end) = clip_window(scenario, timing, self.layout.speed);
        let frame_count = ((end.0 - start.0) * self.fps as f64).ceil() as u32;

        let world = self.apps.main.world_mut();
        world.insert_resource(NoteFieldClock {
            visible: start,
            timing: timing.clone(),
            target_y: TARGET_Y,
        });
        let field = self.field;
        let layout = &self.layout;
        let asset_server = world.resource::<AssetServer>().clone();
        let mut notes: Vec<(SpawnedNote, usize)> = Vec::new();
        let mut mines: Vec<(Entity, usize)> = Vec::new();
        world.resource_scope(|world, skins: Mut<ActiveNoteSkins>| {
            let skin = skins.get(layout.player);
            let mut commands = world.commands();
            spawn_receptors(&mut commands, skin, field, layout);
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
                notes.push((
                    spawn_note(
                        &mut commands,
                        &asset_server,
                        skin,
                        field,
                        layout,
                        &NoteSpawn {
                            time,
                            beat: Beat(note.beat),
                            column: note.column,
                            quant: config.recognized_quant(note.quant),
                            tail,
                        },
                    ),
                    note.column,
                ));
            }
            for mine in &scenario.mines {
                let time = timing.seconds_at_beat(Beat(mine.beat));
                mines.push((
                    spawn_mine(
                        &mut commands,
                        skin,
                        field,
                        layout,
                        time,
                        Beat(mine.beat),
                        mine.column,
                    ),
                    mine.column,
                ));
            }
        });
        world.flush();

        let mut script: Vec<(Seconds, ScriptAction)> = scenario
            .script
            .iter()
            .map(|(beat, action)| (timing.seconds_at_beat(Beat(*beat)), *action))
            .collect();
        script.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));

        // The vanish flash plays in the best grade's color.
        let flash_color = config
            .grading
            .dynamic
            .iter()
            .find_map(|grade| grade.arrow_flash)
            .unwrap_or(Color::WHITE);
        let mut encoder = FfmpegEncoder::start(path, self.fps);
        let mut next_action = 0;
        for frame in 0..frame_count {
            let now = Seconds(start.0 + frame as f64 / self.fps as f64);
            let world = self.apps.main.world_mut();
            world.resource_mut::<NoteFieldClock>().visible = now;
            while next_action < script.len() && script[next_action].0.0 <= now.0 {
                apply_action(
                    world,
                    &self.layout,
                    &notes,
                    &mines,
                    script[next_action].1,
                    flash_color,
                );
                next_action += 1;
            }
            let sender = self.sender.clone();
            world.spawn(Screenshot::image(self.target.clone())).observe(
                move |capture: On<ScreenshotCaptured>| {
                    let data = capture.event().image.data.clone().unwrap_or_default();
                    let _ = sender.send((frame, data));
                },
            );
            self.update();
            for frame in self.frames.try_iter() {
                encoder.push(frame);
            }
        }
        // Captures trail the frames they were requested on; pump until the
        // last ones arrive.
        for _ in 0..300 {
            if encoder.written == frame_count {
                break;
            }
            self.update();
            for frame in self.frames.try_iter() {
                encoder.push(frame);
            }
        }
        assert_eq!(
            encoder.written, frame_count,
            "{}: captured {} of {frame_count} frames",
            scenario.name, encoder.written
        );
        encoder.finish(path);

        self.clear_field();
        frame_count
    }

    fn clear_field(&mut self) {
        let world = self.apps.main.world_mut();
        let mut query = world.query_filtered::<Entity, AnyFieldEntity>();
        let entities: Vec<Entity> = query.iter(world).collect();
        for entity in entities {
            world.despawn(entity);
        }
        world.remove_resource::<NoteFieldClock>();
        while self.frames.try_recv().is_ok() {}
    }
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

fn apply_action(
    world: &mut World,
    layout: &NoteField,
    notes: &[(SpawnedNote, usize)],
    mines: &[(Entity, usize)],
    action: ScriptAction,
    flash_color: Color,
) {
    match action {
        ScriptAction::Hold(index, state) => {
            let mut entity = world.entity_mut(notes[index].0.head);
            let mut visual = entity
                .get_mut::<HoldVisual>()
                .expect("scripted hold state on a note without a hold");
            if visual.state != state {
                visual.state = state;
            }
        }
        ScriptAction::Fade(index) => {
            world
                .entity_mut(notes[index].0.head)
                .insert(FadeOut::over(HOLD_OK_FADE_SECONDS));
        }
        ScriptAction::Vanish(index) => {
            let (note, column) = &notes[index];
            world.despawn(note.head);
            let column = *column;
            let asset_server = world.resource::<AssetServer>().clone();
            world.resource_scope(|world, skins: Mut<ActiveNoteSkins>| {
                let mut commands = world.commands();
                LaneEffects {
                    commands: &mut commands,
                    asset_server: &asset_server,
                    skin: skins.get(layout.player),
                    layout,
                }
                .arrow_flash(column, TARGET_Y, flash_color, false);
            });
            world.flush();
        }
        ScriptAction::Press(column, held) => {
            let mut receptors = world.query::<(&InColumn, &mut Receptor)>();
            for (anchor, mut receptor) in receptors.iter_mut(world) {
                if anchor.column == column && receptor.held != held {
                    receptor.held = held;
                }
            }
        }
        ScriptAction::ExplodeMine(index) => {
            let (entity, column) = mines[index];
            world.despawn(entity);
            let asset_server = world.resource::<AssetServer>().clone();
            world.resource_scope(|world, skins: Mut<ActiveNoteSkins>| {
                let mut commands = world.commands();
                LaneEffects {
                    commands: &mut commands,
                    asset_server: &asset_server,
                    skin: skins.get(layout.player),
                    layout,
                }
                .mine_explosion(column, TARGET_Y);
            });
            world.flush();
        }
    }
}

/// Encodes raw rgba frames into an mp4 through a piped ffmpeg process.
/// Frames may arrive out of order (captures are asynchronous); they are
/// written strictly sequentially.
struct FfmpegEncoder {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    pending: BTreeMap<u32, Vec<u8>>,
    next: u32,
    written: u32,
}

impl FfmpegEncoder {
    fn start(path: &std::path::Path, fps: u32) -> FfmpegEncoder {
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
            child,
            stdin,
            pending: BTreeMap::new(),
            next: 0,
            written: 0,
        }
    }

    fn push(&mut self, (index, data): (u32, Vec<u8>)) {
        assert_eq!(
            data.len(),
            (WIDTH * HEIGHT * 4) as usize,
            "captured frame has unexpected size"
        );
        self.pending.insert(index, data);
        let stdin = self.stdin.as_mut().expect("encoder already finished");
        while let Some(data) = self.pending.remove(&self.next) {
            stdin.write_all(&data).expect("ffmpeg pipe closed early");
            self.next += 1;
            self.written += 1;
        }
    }

    fn finish(mut self, path: &std::path::Path) {
        drop(self.stdin.take());
        let status = self.child.wait().expect("ffmpeg did not run");
        assert!(status.success(), "ffmpeg failed for {}", path.display());
    }
}
