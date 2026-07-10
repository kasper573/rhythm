mod background;
mod clock;
mod tuning;

use self::background::Backgrounds;
use self::clock::Playback;
use self::tuning::Tuning;
use crate::core::audio::{MUSIC_BUS, SFX_BUS, SoundChannel, SoundOptions};
use crate::core::config::config;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::library::{StepfileId, library};
use crate::core::platform::{AssetFetch, FetchPoll, platform};
use crate::core::player::PlayerId;
use crate::core::screen::{SCREEN_SIZE, visible_rect};
use crate::core::settings::{NoteSpeed, Settings};
use crate::core::sfx::Sfx;
use crate::core::stepfile::MusicPlayer;
use crate::core::tick_track::render_tick_track;
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::health_vial::{HealthVial, HealthVialOptions, VialSide};
use crate::nodes::stepfile_player::note_field::{FieldLayout, fitted_arrow_size, max_arrow_size};
use crate::nodes::stepfile_player::{FieldSpec, StepfilePlayer, StepfilePlayerOptions, grade_text};
use crate::scenes::{GameScene, change_scene, scene_accepts_input};
use godot::classes::control::LayoutPreset;
use godot::classes::{Control, IControl};
use godot::global::godot_print;
use godot::prelude::*;
use strum::IntoEnumIterator;

/// The play scene's entry param: inserted by whichever scene starts a
/// session (the wheel, the bench); consumed on enter.
#[derive(Debug, Clone)]
pub struct SelectedStepfile {
    pub id: StepfileId,
    /// The chart each active player steps.
    pub charts: Vec<PlayerChart>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerChart {
    pub player: PlayerId,
    /// Index into the stepfile's `charts`.
    pub chart: usize,
}

/// The play scene: the real gameplay adapter around the stepfile player.
/// It fills the engine's ports from the audio clock (see `clock`) and the
/// keyboard, composes the stage furniture (health vials, backgrounds, the
/// tuning HUD), and turns the session's end into
/// [`ScoreResults`](crate::scenes::score::ScoreResults).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct PlayScene {
    selected: Option<SelectedStepfile>,
    engine: Option<Gd<StepfilePlayer>>,
    playback: Option<Playback>,
    music_fetch: Option<Box<dyn AssetFetch>>,
    music_name: String,
    music: Option<SoundChannel>,
    tick: Option<SoundChannel>,
    vials: Vec<(PlayerId, Gd<HealthVial>)>,
    backgrounds: Option<Backgrounds>,
    tuning: Option<Tuning>,
    /// Set by the `stage_failed` signal (which fires mid-grading, where
    /// the engine cannot be re-entered); acted on next frame.
    check_failure: bool,
    finished: bool,
    base: Base<Control>,
}

#[godot_api]
impl PlayScene {
    pub fn instantiate(game: &mut Game) -> Gd<PlayScene> {
        let mut scene = PlayScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let Some(selected) = game.take_selected_stepfile() else {
            game.change_scene(GameScene::Wheel);
            return scene;
        };
        MusicPlayer::singleton().bind_mut().stop();

        let entry = library().stepfile(selected.id);
        let timing = entry.stepfile.timing.clone();
        let Some(charts) = selected
            .charts
            .iter()
            .map(|player_chart| {
                entry
                    .stepfile
                    .charts
                    .get(player_chart.chart)
                    .map(|chart| (player_chart.player, chart))
            })
            .collect::<Option<Vec<_>>>()
        else {
            game.change_scene(GameScene::Wheel);
            return scene;
        };
        if charts.is_empty() {
            game.change_scene(GameScene::Wheel);
            return scene;
        }

        let backgrounds = Backgrounds::new(&mut scene.clone().upcast::<Control>(), entry, &timing);
        scene.bind_mut().backgrounds = Some(backgrounds);

        let settings = Settings::singleton();
        let specs: Vec<PackSpec> = charts
            .iter()
            .map(|(player, chart)| PackSpec {
                player: *player,
                columns: chart.columns,
                speed: settings.bind().player(*player).note_speed,
            })
            .collect();
        let layouts = pack_stage_fields(&specs, SCREEN_SIZE.x, 1.0);

        let fields: Vec<FieldSpec> = charts
            .iter()
            .zip(&layouts)
            .map(|((_, chart), layout)| FieldSpec {
                layout: layout.clone(),
                rows: chart.rows.clone(),
                mines: chart.mines.clone(),
                max_health: config().player_max_health,
            })
            .collect();
        let engine = StepfilePlayer::instantiate(StepfilePlayerOptions {
            fields,
            timing: timing.clone(),
            canvas: SCREEN_SIZE,
        });
        scene.add_child(&engine);
        engine
            .signals()
            .stage_failed()
            .connect_other(&scene, PlayScene::on_stage_failed);
        engine
            .signals()
            .press_banked()
            .connect_other(&scene, PlayScene::on_press_banked);
        let last_note_time = engine.bind().last_note_time();

        for (player, _) in &charts {
            let side = match player {
                PlayerId::P1 => VialSide::Left,
                PlayerId::P2 => VialSide::Right,
            };
            let vial = HealthVial::instantiate(HealthVialOptions {
                fill: 1.0,
                side,
                edge_padding: config().stage.screen_edge_padding,
            });
            scene.add_child(&vial);
            scene.bind_mut().vials.push((*player, vial));
        }

        // The pre-rendered tick track, one tick per row across every
        // stage's charts, so versus hears both; muted unless toggled on.
        let tick_times: Vec<Seconds> = charts
            .iter()
            .flat_map(|(_, chart)| {
                chart
                    .rows
                    .iter()
                    .map(|row| timing.seconds_at_beat(row.beat))
            })
            .collect();
        let tick = match render_tick_track(
            &crate::core::assets::asset_root().join(Sfx::Tick.asset_path()),
            &tick_times,
            config().tick_volume,
        ) {
            Ok(track) => SoundChannel::open_pcm(
                &mut scene.clone().upcast::<Control>(),
                &track.samples,
                track.sample_rate,
                SoundOptions {
                    paused: true,
                    muted: true,
                    bus: SFX_BUS,
                    ..Default::default()
                },
            )
            .inspect_err(|error| godot::global::godot_warn!("could not play tick track: {error}"))
            .ok(),
            Err(error) => {
                godot::global::godot_warn!("could not render tick track: {error}");
                None
            }
        };

        let music_path = entry.music_path();
        if music_path.is_none() {
            godot_print!(
                "no music file for \"{}\", playing silent",
                entry.display_title()
            );
        }
        let tuning = Tuning::new(&mut scene.clone().upcast::<Control>());
        let mut bound = scene.bind_mut();
        bound.music_fetch = music_path.as_ref().map(|path| platform().fetch_asset(path));
        bound.music_name = music_path
            .as_deref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        bound.tick = tick;
        bound.tuning = Some(tuning);
        bound.playback = Some(Playback::new(
            entry.display_title(),
            timing,
            config().stage.lead_in_seconds,
            last_note_time,
        ));
        bound.engine = Some(engine);
        bound.selected = Some(selected);
        drop(bound);
        scene
    }

    fn on_stage_failed(&mut self, _player: i64) {
        Sfx::Fail.play();
        self.check_failure = true;
    }

    fn on_press_banked(&mut self, error: f64) {
        if let Some(tuning) = &mut self.tuning {
            tuning.push_sample(Seconds(error));
        }
    }

    /// Opens the music channel (paused) once its bytes arrive; failures
    /// drop the music and the session plays with whatever survives.
    fn poll_music(&mut self) {
        let Some(fetch) = &mut self.music_fetch else {
            return;
        };
        match fetch.poll() {
            FetchPoll::Pending => {}
            FetchPoll::Failed(error) => {
                godot::global::godot_warn!("music failed to load: {error}");
                self.music_fetch = None;
            }
            FetchPoll::Ready(bytes) => {
                self.music_fetch = None;
                let mut host = self.base().clone().upcast::<godot::classes::Node>();
                match SoundChannel::open(
                    &mut host,
                    &bytes,
                    &self.music_name,
                    SoundOptions {
                        paused: true,
                        bus: MUSIC_BUS,
                        ..Default::default()
                    },
                ) {
                    Ok(channel) => self.music = Some(channel),
                    Err(error) => godot::global::godot_warn!("music cannot play: {error}"),
                }
            }
        }
    }

    /// Re-packs the stage onto the window's canvas: the arrow-size cap is
    /// a screen-pixel budget, so a resize re-derives every field's arrow
    /// size and origin, and the engine moves the lanes accordingly.
    fn refit_to_window(&mut self) {
        let rect = visible_rect(&self.base().clone().upcast::<Control>());
        let pixels_per_unit = self.pixels_per_unit(rect);
        let Some(engine) = &mut self.engine else {
            return;
        };
        let settings = Settings::singleton();
        let specs: Vec<PackSpec> = {
            let bound = engine.bind();
            bound
                .players()
                .iter()
                .zip(bound.field_layouts())
                .map(|(player, layout)| PackSpec {
                    player: *player,
                    columns: layout.columns,
                    speed: settings.bind().player(*player).note_speed,
                })
                .collect()
        };
        if specs.is_empty() {
            return;
        }
        let layouts = pack_stage_fields(&specs, rect.size.x, pixels_per_unit);
        let arrow_size = layouts[0].arrow_size;
        let mut bound = engine.bind_mut();
        bound.refit(layouts);
        bound.set_canvas(rect.size, pixels_per_unit);
        // The receptor row keeps the configured breathing room to the top
        // edge, whatever extra world a non-16:9 window reveals and
        // whatever size the arrows were fitted to.
        let padding = config().stage.screen_edge_padding;
        bound.set_target_y(rect.size.y / 2.0 - padding - arrow_size / 2.0);
        bound.set_grade_area(grade_text::grade_area(
            rect.size.y / 2.0 - padding,
            -rect.size.y / 2.0 + padding,
        ));
    }

    /// The window's canvas-to-pixel factor.
    fn pixels_per_unit(&self, rect: Rect2) -> f32 {
        self.base()
            .get_window()
            .map(|window| window.get_size().x as f32 / rect.size.x.max(1.0))
            .unwrap_or(1.0)
    }

    /// The real adapter's input driver: fills the engine's input port from
    /// the keyboard. Cleared rather than skipped while the scene fade
    /// runs, so a departing stage grants no input.
    fn wire_keyboard(&mut self) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        let mut bound = engine.bind_mut();
        bound.clear_input();
        if !scene_accepts_input() {
            return;
        }
        for player in PlayerId::iter() {
            for column in 0..4 {
                let action = GameAction::step(player, StepDirection::of_column(column));
                if Actions::pressed(action) {
                    bound.press(action, Actions::just_pressed(action));
                }
            }
        }
    }

    fn sync_health_vials(&mut self) {
        let Some(engine) = &self.engine else { return };
        let bound = engine.bind();
        let beat = bound.visible_beat();
        for (player, vial) in &mut self.vials {
            let Some(fill) = bound.health_fraction(*player) else {
                continue;
            };
            let mut vial = vial.bind_mut();
            vial.set_fill(fill);
            vial.set_beat(beat);
        }
    }

    fn collect_results(&self) -> Option<crate::scenes::score::ScoreResults> {
        let engine = self.engine.as_ref()?;
        let selected = self.selected.as_ref()?;
        let playback = self.playback.as_ref()?;
        let players = engine
            .bind()
            .results()
            .into_iter()
            .zip(&selected.charts)
            .map(|(stage, player_chart)| crate::scenes::score::PlayerResult {
                chart: player_chart.chart,
                stage,
            })
            .collect();
        Some(crate::scenes::score::ScoreResults {
            id: selected.id,
            title: playback.title.clone(),
            players,
        })
    }

    /// The session ends when every stage settled and the audio ran out (or
    /// nothing plays and the chart is over); the grades given so far become
    /// the final result.
    fn finish_when_complete(&mut self) {
        if self.finished {
            return;
        }
        let Some(engine) = &self.engine else { return };
        let Some(playback) = &self.playback else {
            return;
        };
        let failed_out = self.check_failure && engine.bind().all_failed();
        if !failed_out {
            if !engine.bind().all_settled() {
                return;
            }
            let audio_done = if let Some(music) = &self.music {
                music.is_finished()
            } else if self.music_fetch.is_some() {
                false
            } else if let Some(tick) = &self.tick {
                tick.is_finished()
            } else {
                playback.position().0 > playback.last_note_time.0 + 2.0
            };
            // Trailing mines and hold tails can outlive the audio; let
            // them resolve.
            let chart_done = playback.position().0 >= playback.last_note_time.0;
            if !audio_done || !chart_done || !playback.is_playing() {
                return;
            }
        }
        self.finished = true;
        if let Some(results) = self.collect_results() {
            Game::singleton().bind_mut().set_score_results(results);
        }
        change_scene(GameScene::Score);
    }

    fn handle_cancel(&mut self) {
        if !scene_accepts_input() {
            return;
        }
        let Some(engine) = &self.engine else { return };
        let cancelled = engine
            .bind()
            .players()
            .iter()
            .any(|player| Actions::just_pressed(GameAction::cancel(*player)));
        if cancelled {
            Sfx::Cancel.play();
            if let Some(selected) = &self.selected {
                Game::singleton().bind_mut().set_wheel_target(selected.id);
            }
            change_scene(GameScene::Wheel);
        }
    }
}

#[godot_api]
impl IControl for PlayScene {
    fn init(base: Base<Control>) -> PlayScene {
        PlayScene {
            selected: None,
            engine: None,
            playback: None,
            music_fetch: None,
            music_name: String::new(),
            music: None,
            tick: None,
            vials: Vec::new(),
            backgrounds: None,
            tuning: None,
            check_failure: false,
            finished: false,
            base,
        }
    }

    /// Fills the engine's ports for the frame; the engine (a child)
    /// processes after this, grading on what was just written.
    fn process(&mut self, delta: f64) {
        self.poll_music();
        self.refit_to_window();
        self.advance_clock(delta);
        self.wire_keyboard();

        let visible = self
            .playback
            .as_ref()
            .map(|playback| playback.visible_now())
            .unwrap_or(Seconds::ZERO);
        if let Some(backgrounds) = &mut self.backgrounds {
            backgrounds.update(visible, delta as f32);
        }
        self.sync_health_vials();
        self.run_tuning(delta);
        self.finish_when_complete();
        self.check_failure = false;
        self.handle_cancel();
    }

    fn exit_tree(&mut self) {
        MusicPlayer::singleton().bind_mut().stop();
    }
}

/// What a stage field is packed from, independent of whether it exists yet.
struct PackSpec {
    player: PlayerId,
    columns: usize,
    speed: NoteSpeed,
}

/// Sizes and places one field per stage: arrows grow to the configured
/// pixel cap (`stage.max_arrow_size`) when the window has room and shrink
/// until every column — plus the gaps between fields — fits between the
/// reserved screen edges. The fields pack left-to-right, centered as a
/// block, across the window's visible canvas width.
fn pack_stage_fields(
    specs: &[PackSpec],
    visible_width: f32,
    pixels_per_unit: f32,
) -> Vec<FieldLayout> {
    let stage = &config().stage;
    let columns: usize = specs.iter().map(|spec| spec.columns).sum();
    let gap_units = stage.field_gap_columns * (specs.len() - 1) as f32;
    let arrow_size = fitted_arrow_size(
        columns as f32 + gap_units,
        visible_width - 2.0 * stage.margin_x,
        max_arrow_size(config(), pixels_per_unit),
    );

    let mut layouts: Vec<FieldLayout> = specs
        .iter()
        .map(|spec| FieldLayout {
            player: spec.player,
            origin_x: 0.0,
            columns: spec.columns,
            speed: spec.speed,
            arrow_size,
        })
        .collect();
    let gap = stage.field_gap_columns * layouts[0].spacing();
    let total: f32 =
        layouts.iter().map(FieldLayout::width).sum::<f32>() + gap * (layouts.len() - 1) as f32;
    let mut x = -total / 2.0;
    for layout in &mut layouts {
        layout.origin_x = x + layout.width() / 2.0;
        x += layout.width() + gap;
    }
    layouts
}
