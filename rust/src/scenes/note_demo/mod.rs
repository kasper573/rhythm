//! The note demo: one animation scenario from the catalog, played through
//! the real note field with a scripted stand-in for the gameplay systems,
//! then the game exits. Deep-linked with `--scene note-demo --scenario
//! <name>`; without a scenario it prints the catalog and exits, which is
//! also how tooling discovers it.

pub mod scenarios;

use crate::core::config::config;
use crate::core::player::PlayerId;
use crate::core::screen::CLEAR_COLOR;
use crate::core::settings::{NoteSpeed, Perspective};
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Bpm, Seconds};
use crate::game::Game;
use crate::nodes::stepfile_player::note_field::{
    FieldClock, FieldLayout, HOLD_OK_FADE_SECONDS, MineIndex, NoteFieldRig, NoteIndex, NoteSpawn,
    NoteTail, TARGET_Y,
};
use crate::nodes::stepfile_player::note_skin::load_note_skin;
use godot::classes::window::ContentScaleMode;
use godot::classes::{ColorRect, Control, IControl};
use godot::prelude::*;
use scenarios::{Scenario, ScriptAction, scenario_matrix};

/// The demo's entry params, inserted by the launch directives; consumed on
/// enter.
pub struct NoteDemoParams {
    /// Exact scenario name; `None` prints the catalog and exits.
    pub scenario: Option<String>,
    /// Note skin; the game config's default when omitted.
    pub skin: Option<String>,
    pub perspective: String,
    /// Tempo for scenarios without their own bpm timeline.
    pub bpm: f64,
}

/// The demoed field's arrow size: the classic in-game proportions.
const ARROW_SIZE: f32 = 88.0;
/// The first note starts this far below the receptors: past the bottom
/// edge whatever the scroll speed.
const LEAD_PIXELS: f32 = 760.0;
const TAIL_SECONDS: f64 = 1.2;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct NoteDemoScene {
    rig: Option<NoteFieldRig>,
    timing: StepfileTiming,
    start: Seconds,
    end: Seconds,
    elapsed: f64,
    notes: Vec<(NoteIndex, usize)>,
    mines: Vec<(MineIndex, usize)>,
    script: Vec<(Seconds, ScriptAction)>,
    next_action: usize,
    base: Base<Control>,
}

#[godot_api]
impl NoteDemoScene {
    pub fn instantiate(game: &mut Game) -> Gd<NoteDemoScene> {
        let params = game.take_note_demo().unwrap_or(NoteDemoParams {
            scenario: None,
            skin: None,
            perspective: "None".to_string(),
            bpm: 120.0,
        });
        let mut scene = NoteDemoScene::new_alloc();
        scene.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);

        let scenario = params.scenario.and_then(|name| {
            scenario_matrix()
                .into_iter()
                .find(|scenario| scenario.name == name)
        });
        let Some(scenario) = scenario else {
            for name in scenarios::scenario_names() {
                println!("scenario: {name}");
            }
            game.base().get_tree().quit();
            return scene;
        };
        assert!(params.bpm > 0.0, "--bpm must be positive");

        // The demo draws 1:1 at whatever size the window was launched with
        // (the tooling picks its capture resolution with `--resolution`).
        let mut window = game.base().get_window().expect("the game runs in a window");
        window.set_content_scale_mode(ContentScaleMode::DISABLED);
        let size = window.get_size();

        let mut backdrop = ColorRect::new_alloc();
        backdrop.set_color(CLEAR_COLOR);
        backdrop.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        scene.add_child(&backdrop);

        let defaults = &config().defaults.player_options;
        let skin_name = params.skin.unwrap_or_else(|| defaults.note_skin.clone());
        let perspective: Perspective = params
            .perspective
            .parse()
            .expect("--perspective must be None, Above, or Below");
        let layout = FieldLayout {
            player: PlayerId::P1,
            origin_x: 0.0,
            columns: 4,
            speed: defaults.note_speed,
            arrow_size: ARROW_SIZE,
        };
        let lane_camera = &config().lane_camera;
        let mut host = scene.clone().upcast::<Control>();
        let mut rig = NoteFieldRig::build(
            &mut host,
            layout,
            load_note_skin(&skin_name),
            perspective,
            lane_camera.fov_degrees,
            lane_camera.tilt_degrees,
            Vector2::new(size.x as f32, size.y as f32),
        );

        let timing = scenario_timing(&scenario, params.bpm);
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
        let (start, end) = demo_window(&scenario, &timing, defaults.note_speed);

        {
            let mut bound = scene.bind_mut();
            bound.rig = Some(rig);
            bound.timing = timing;
            bound.start = start;
            bound.end = end;
            bound.notes = notes;
            bound.mines = mines;
            bound.script = script;
        }
        scene
    }

    fn apply_action(&mut self, action: ScriptAction) {
        // The vanish flash plays in the best grade's color.
        let flash_color = config()
            .grading
            .dynamic
            .iter()
            .find_map(|grade| grade.arrow_flash)
            .unwrap_or(Color::WHITE);
        let rig = self.rig.as_mut().expect("the demo has a field");
        match action {
            ScriptAction::Hold(index, state) => {
                rig.set_hold_state(self.notes[index].0, state);
            }
            ScriptAction::Fade(index) => {
                rig.fade_out_note(self.notes[index].0, HOLD_OK_FADE_SECONDS);
            }
            ScriptAction::Vanish(index) => {
                let (note, column) = self.notes[index];
                rig.vanish_note(note);
                rig.arrow_flash(column, TARGET_Y, flash_color, false);
            }
            ScriptAction::Press(column, held) => {
                rig.set_receptor_held(column, held);
            }
            ScriptAction::ExplodeMine(index) => {
                let (mine, column) = self.mines[index];
                rig.remove_mine(mine);
                rig.mine_explosion(column, TARGET_Y);
            }
        }
    }
}

#[godot_api]
impl IControl for NoteDemoScene {
    fn init(base: Base<Control>) -> NoteDemoScene {
        NoteDemoScene {
            rig: None,
            timing: StepfileTiming::new(Seconds::ZERO, &[], &[]),
            start: Seconds::ZERO,
            end: Seconds::ZERO,
            elapsed: 0.0,
            notes: Vec::new(),
            mines: Vec::new(),
            script: Vec::new(),
            next_action: 0,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        if self.rig.is_none() {
            return;
        }
        let now = Seconds(self.start.0 + self.elapsed);
        self.elapsed += delta;
        while self.next_action < self.script.len() && self.script[self.next_action].0.0 <= now.0 {
            let action = self.script[self.next_action].1;
            self.apply_action(action);
            self.next_action += 1;
        }
        let clock = FieldClock {
            visible: now,
            timing: self.timing.clone(),
            target_y: TARGET_Y,
        };
        self.rig
            .as_mut()
            .expect("the demo has a field")
            .update(&clock, delta as f32);
        if now.0 >= self.end.0 {
            self.base().get_tree().quit();
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

/// The demo runs from a lead-in before the first thing on the timeline to
/// a tail after the last, so every note scrolls in from below and every
/// fade and popup finishes on screen. The lead is a fixed distance in
/// pixels, so it adapts to whatever the scroll speed is.
fn demo_window(
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
    let lead_arrows = (LEAD_PIXELS / ARROW_SIZE) as f64;
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
