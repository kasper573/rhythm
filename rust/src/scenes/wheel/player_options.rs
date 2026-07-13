use crate::core::config::{GameConfig, config};
use crate::core::font::label;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::screen::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::screen::{SCREEN_SIZE, linear_blend};
use crate::core::settings::{GradeLayer, NoteSpeed, Perspective, PlayerOptions, Settings};
use crate::core::sfx::Sfx;
use crate::core::stepfile::{Arrow, MusicPlayer, Row, StepfileTiming, Tail};
use crate::core::units::{Beat, Percent, Seconds};
use crate::nodes::menu::NavInput;
use crate::nodes::stepfile_player::note_field::{FieldLayout, fitted_arrow_size, max_arrow_size};
use crate::nodes::stepfile_player::note_skin::{NoteSkinLibrary, note_skins};
use crate::nodes::stepfile_player::{FieldSpec, StepfilePlayer, StepfilePlayerOptions, grade_text};
use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::{
    CenterContainer, ColorRect, Control, HBoxContainer, Label, MarginContainer, SubViewport,
    TextureRect, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// The design-canvas height each preview frames, so a field reads as tall
/// as it does full-screen before its surface scales it down.
const PREVIEW_BAND: f32 = SCREEN_SIZE.y;
/// The grades the autoplay walks through — only the top three tiers.
const PREVIEW_GRADES: [usize; 3] = [0, 1, 2];
/// How long past its note an autoplayed tap stays pressed — long enough to
/// bank, short enough not to catch the next note in its column.
const AUTOPLAY_TAP_HOLD: Seconds = Seconds(0.05);

const TRANSITION_SECONDS: f32 = 0.25;

/// Fixed column widths keep the table from re-centering when a value's
/// text length changes.
const NAME_WIDTH: f32 = 220.0;
const VALUE_WIDTH: f32 = 200.0;

/// The player options modal: edits each active player's options in place
/// (they live in the player settings, so changes persist immediately) as
/// an edge-to-edge stripe over the vertical center of the wheel, which
/// stays mounted underneath. One options column per active player's pad —
/// P1's flank to the left, P2's to the right — each flank showing that
/// player's own autoplayed preview: the stepfile player driven by a mocked
/// chart, clocked by the wheel music, rebuilt in place whenever an option
/// changes so the preview always reflects the current selection exactly.
pub(super) struct OptionsModal {
    root: Gd<Control>,
    background: Gd<ColorRect>,
    content: Gd<Control>,
    column: Gd<VBoxContainer>,
    texts: Vec<Gd<Label>>,
    /// `t` runs 0..=1 through the open/close effect; `dir` is +1 while
    /// opening and -1 while closing.
    t: f32,
    dir: f32,
    players: Vec<PlayerId>,
    panels: Vec<PanelState>,
    row_names: Vec<Gd<Label>>,
    values: Vec<ValueCell>,
    previews: Vec<Preview>,
    /// The mocked chart and loop tracking; `None` until the wheel music
    /// reports its loop window.
    state: Option<PreviewState>,
}

/// One player's cursor.
struct PanelState {
    player: PlayerId,
    active_row: usize,
}

struct ValueCell {
    player: PlayerId,
    row: usize,
    label: Gd<Label>,
}

struct Preview {
    player: PlayerId,
    flank: Gd<Control>,
    viewport: Option<Gd<SubViewport>>,
    engine: Option<Gd<StepfilePlayer>>,
}

struct PreviewState {
    timing: StepfileTiming,
    rows: Vec<Row>,
    last_visible: Seconds,
    rebuild: bool,
}

impl OptionsModal {
    pub fn open(host: &mut Control, players: Vec<PlayerId>) -> OptionsModal {
        let versus = players.len() > 1;
        let mut root = Control::new_alloc();
        root.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        root.set_z_index(300);
        host.add_child(&root);

        // The background and the content are siblings so the transition
        // can slide them in from opposite directions.
        let mut background = ColorRect::new_alloc();
        background.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        root.add_child(&background);

        // The stripe's content: a plain box the transition places by hand
        // — full width, vertically centered at the table's natural height —
        // so the wheel stays visible above and below.
        let mut content = Control::new_alloc();
        let mut stripe_row = HBoxContainer::new_alloc();
        stripe_row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        stripe_row.add_theme_constant_override("separation", 0);
        let mut texts = Vec::new();
        let mut previews = Vec::new();

        // Equal flanks on both sides keep the table centered whatever it
        // reads; each active player's flank hosts a preview surface.
        let mut left = Control::new_alloc();
        left.set_h_size_flags(SizeFlags::EXPAND_FILL);
        stripe_row.add_child(&left);
        if let Some(player) = players.first() {
            previews.push(Preview {
                player: *player,
                flank: left.clone(),
                viewport: None,
                engine: None,
            });
        }

        let settings = Settings::singleton();
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 12);
        let mut title = label("Player Options", 48.0, TITLE_COLOR);
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        let mut title_box = MarginContainer::new_alloc();
        title_box.add_theme_constant_override("margin_bottom", 12);
        title_box.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        title_box.add_child(&title);
        column.add_child(&title_box);
        texts.push(title);

        let mut row_names = Vec::new();
        let mut values = Vec::new();
        // The `P1`/`P2` tags over the value columns (versus only).
        if versus {
            let mut header = HBoxContainer::new_alloc();
            header.add_theme_constant_override("separation", 20);
            let mut name_pad = Control::new_alloc();
            name_pad.set_custom_minimum_size(Vector2::new(NAME_WIDTH, 0.0));
            header.add_child(&name_pad);
            for player in &players {
                let mut cell = CenterContainer::new_alloc();
                cell.set_custom_minimum_size(Vector2::new(VALUE_WIDTH, 36.0));
                let tag = label(player.label(), 30.0, TITLE_COLOR);
                cell.add_child(&tag);
                texts.push(tag);
                header.add_child(&cell);
            }
            column.add_child(&header);
        }
        for (index, row) in OptionRow::iter().enumerate() {
            let mut row_box = HBoxContainer::new_alloc();
            row_box.add_theme_constant_override("separation", 20);
            let mut name_cell = Control::new_alloc();
            name_cell.set_custom_minimum_size(Vector2::new(NAME_WIDTH, 34.0));
            let name = label(<&str>::from(row), 28.0, INACTIVE_COLOR);
            name_cell.add_child(&name);
            row_names.push(name.clone());
            texts.push(name);
            row_box.add_child(&name_cell);
            for player in &players {
                let mut cell = CenterContainer::new_alloc();
                cell.set_custom_minimum_size(Vector2::new(VALUE_WIDTH, 34.0));
                let value = row_value(row, settings.bind().player(*player), config(), note_skins());
                let value_label = label(&value, 28.0, INACTIVE_COLOR);
                cell.add_child(&value_label);
                values.push(ValueCell {
                    player: *player,
                    row: index,
                    label: value_label.clone(),
                });
                texts.push(value_label);
                row_box.add_child(&cell);
            }
            column.add_child(&row_box);
        }
        stripe_row.add_child(&column.clone());

        let mut right = Control::new_alloc();
        right.set_h_size_flags(SizeFlags::EXPAND_FILL);
        stripe_row.add_child(&right);
        if let Some(player) = players.get(1) {
            previews.push(Preview {
                player: *player,
                flank: right.clone(),
                viewport: None,
                engine: None,
            });
        }
        content.add_child(&stripe_row);
        root.add_child(&content);

        OptionsModal {
            root,
            background,
            content,
            column,
            texts,
            t: 0.0,
            dir: 1.0,
            panels: players
                .iter()
                .map(|player| PanelState {
                    player: *player,
                    active_row: 0,
                })
                .collect(),
            players,
            row_names,
            values,
            previews,
            state: None,
        }
    }

    /// Rebuilds the previews on the next frame — the wheel forwards the
    /// settings' `changed` signal here while the modal is open.
    pub fn mark_rebuild(&mut self) {
        if let Some(state) = &mut self.state {
            state.rebuild = true;
        }
    }

    /// One modal frame; returns true once the closing transition finished
    /// and the modal removed itself.
    pub fn update(&mut self, delta: f64) -> bool {
        if self.dir > 0.0 && self.t >= 1.0 {
            self.handle_pulses();
            self.handle_close();
        }
        if !self.animate_transition(delta) {
            self.root.queue_free();
            return true;
        }
        self.build_previews();
        self.refit_previews();
        self.drive_previews();
        self.refresh_values();
        self.highlight_rows();
        false
    }

    /// Routes each pulse to the pulsing player's own panel: their pad's
    /// up/down moves between rows, left/right steps the value.
    fn handle_pulses(&mut self) {
        for pulse in NavInput::pulses() {
            let Some((player, direction)) = pulse.as_step() else {
                continue;
            };
            let Some(panel) = self.panels.iter_mut().find(|panel| panel.player == player) else {
                continue;
            };
            let acted = match direction {
                StepDirection::Up => {
                    panel.active_row = (panel.active_row + OptionRow::COUNT - 1) % OptionRow::COUNT;
                    true
                }
                StepDirection::Down => {
                    panel.active_row = (panel.active_row + 1) % OptionRow::COUNT;
                    true
                }
                StepDirection::Left | StepDirection::Right => {
                    let delta = if direction == StepDirection::Left {
                        -1
                    } else {
                        1
                    };
                    let row = row(panel.active_row);
                    let mut settings = Settings::singleton();
                    let mut acted = false;
                    settings.bind_mut().edit_player(player, |options| {
                        acted = change_value(row, delta, options, config(), note_skins());
                    });
                    acted
                }
            };
            if acted {
                Sfx::Navigate.play();
            }
        }
    }

    /// Closing the modal is a shared space: any active player's ¤Select¤
    /// or ¤Cancel¤ closes it for everyone.
    fn handle_close(&mut self) {
        if self.dir < 0.0 {
            return;
        }
        if Actions::any_just_pressed(&self.players, GameAction::cancel)
            || Actions::any_just_pressed(&self.players, GameAction::select)
        {
            Sfx::Cancel.play();
            self.dir = -1.0;
        }
    }

    /// The background slides in from the left and the content from the
    /// right, both fading in; closing plays the same effect in reverse.
    /// Returns false once fully closed.
    fn animate_transition(&mut self, delta: f64) -> bool {
        let advance = !(self.t >= 1.0 && self.dir > 0.0);
        if advance {
            self.t = (self.t + self.dir * delta as f32 / TRANSITION_SECONDS).clamp(0.0, 1.0);
        }
        if self.t <= 0.0 && self.dir < 0.0 {
            return false;
        }
        let size = self.root.get_size();
        let height = self.column.get_combined_minimum_size().y + 48.0;
        let top = (size.y - height) / 2.0;
        let eased = 1.0 - (1.0 - self.t).powi(3);
        self.background
            .set_position(Vector2::new(-size.x * (1.0 - eased), top));
        self.background.set_size(Vector2::new(size.x, height));
        self.background.set_color(Color::from_rgba(
            0.0,
            0.0,
            0.0,
            1.0 - linear_blend(1.0 - eased),
        ));
        self.content
            .set_position(Vector2::new(size.x * (1.0 - eased), top));
        self.content.set_size(Vector2::new(size.x, height));
        for text in &mut self.texts {
            let mut modulate = text.get_modulate();
            modulate.a = linear_blend(eased);
            text.set_modulate(modulate);
        }
        true
    }

    /// Once the wheel music plays and every flank is laid out, builds a
    /// render viewport per flank and arms the first field spawn.
    fn build_previews(&mut self) {
        if self.state.is_some() {
            return;
        }
        let music = MusicPlayer::singleton();
        let Some((timing, start, length)) = music.bind().loop_window() else {
            return;
        };
        if self.previews.is_empty() {
            self.state = Some(PreviewState {
                rows: mocked_rows(&timing, start, length),
                timing,
                last_visible: Seconds::ZERO,
                rebuild: false,
            });
            return;
        }
        if self
            .previews
            .iter()
            .any(|preview| preview.flank.get_size().x <= 0.0 || preview.flank.get_size().y <= 0.0)
        {
            return;
        }
        for preview in &mut self.previews {
            let size = preview.flank.get_size();
            let mut viewport = SubViewport::new_alloc();
            viewport.set_transparent_background(true);
            viewport.set_size(Vector2i::new(size.x as i32, size.y as i32));
            viewport.set_update_mode(godot::classes::sub_viewport::UpdateMode::ALWAYS);
            preview.flank.add_child(&viewport);
            let mut display = TextureRect::new_alloc();
            display.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            if let Some(texture) = viewport.get_texture() {
                display.set_texture(&texture);
            }
            preview.flank.add_child(&display);
            preview.viewport = Some(viewport);
        }
        self.state = Some(PreviewState {
            rows: mocked_rows(&timing, start, length),
            timing,
            last_visible: Seconds::ZERO,
            rebuild: true,
        });
    }

    /// Follows each flank's laid-out size every frame — the stripe height
    /// and the window can both change under the modal — so the surface
    /// renders at native resolution and the band maps onto it undistorted.
    fn refit_previews(&mut self) {
        for preview in &mut self.previews {
            let (Some(viewport), Some(engine)) = (&mut preview.viewport, &mut preview.engine)
            else {
                continue;
            };
            let surface = preview.flank.get_size();
            if surface.x <= 0.0 || surface.y <= 0.0 {
                continue;
            }
            viewport.set_size(Vector2i::new(surface.x as i32, surface.y as i32));
            engine
                .bind_mut()
                .set_canvas(band_canvas(surface), surface.y / PREVIEW_BAND);
        }
    }

    /// The mocked adapter: clocks the previews from the wheel music,
    /// rebuilds them whenever an option changes or the music loops back,
    /// and autoplays every note deterministically at its tier's offset.
    fn drive_previews(&mut self) {
        let Some(state) = &mut self.state else {
            return;
        };
        let settings = Settings::singleton();
        let timing_settings = settings.bind().machine().timing.clone();
        let music = MusicPlayer::singleton();
        let Some(visible) = music
            .bind()
            .visible_now(&timing_settings)
            .map(|(visible, _)| visible)
        else {
            return;
        };
        if visible.0 + 0.05 < state.last_visible.0 {
            state.rebuild = true;
        }
        state.last_visible = visible;

        if state.rebuild {
            state.rebuild = false;
            let live: Vec<Row> = state
                .rows
                .iter()
                .filter(|chart_row| row_until(chart_row, &state.timing) > visible)
                .cloned()
                .collect();
            for preview in &mut self.previews {
                if let Some(mut engine) = preview.engine.take() {
                    engine.queue_free();
                }
                let Some(viewport) = &mut preview.viewport else {
                    continue;
                };
                let surface = preview.flank.get_size();
                let canvas = band_canvas(surface);
                let arrow = preview_arrow_size(config());
                let mut engine = StepfilePlayer::instantiate(StepfilePlayerOptions {
                    fields: vec![FieldSpec {
                        layout: FieldLayout {
                            player: preview.player,
                            origin_x: 0.0,
                            columns: 4,
                            speed: settings.bind().player(preview.player).note_speed,
                            arrow_size: arrow,
                        },
                        rows: live.clone(),
                        mines: Vec::new(),
                        max_health: u32::MAX,
                    }],
                    timing: state.timing.clone(),
                    canvas,
                });
                viewport.add_child(&engine);
                let padding = config().stage.screen_edge_padding;
                let half = PREVIEW_BAND / 2.0;
                let mut bound = engine.bind_mut();
                bound.set_canvas(canvas, surface.y.max(1.0) / PREVIEW_BAND);
                bound.set_target_y(half - padding - arrow / 2.0);
                bound.set_grade_area(grade_text::grade_area(half - padding, -half + padding));
                drop(bound);
                preview.engine = Some(engine);
            }
        }

        // The engine's ports, filled deterministically: every note in its
        // hit window is pressed at the offset that grades it to its tier,
        // held through a hold's tail. No keyboard.
        for preview in &mut self.previews {
            let Some(engine) = &mut preview.engine else {
                continue;
            };
            let mut bound = engine.bind_mut();
            bound.set_time(visible, visible);
            bound.clear_input();
            for chart_row in &state.rows {
                let time = state.timing.seconds_at_beat(chart_row.beat);
                let Some(offset) = autoplay_offset(config(), note_tier(chart_row.beat)) else {
                    continue;
                };
                let due = time - offset;
                for arrow in &chart_row.arrows {
                    if visible >= due && visible < arrow_until(chart_row, arrow, &state.timing) {
                        let action = GameAction::step(
                            preview.player,
                            StepDirection::of_column(arrow.column),
                        );
                        bound.press(action, true);
                    }
                }
            }
        }
    }

    /// Keeps every value text on its player's current selection.
    fn refresh_values(&mut self) {
        let settings = Settings::singleton();
        for cell in &mut self.values {
            let current = row_value(
                row(cell.row),
                settings.bind().player(cell.player),
                config(),
                note_skins(),
            );
            cell.label.set_text(&current);
        }
    }

    fn highlight_rows(&mut self) {
        // The name highlights when any player edits that row; a value only
        // when its own player does.
        for (index, name) in self.row_names.iter_mut().enumerate() {
            let active = self.panels.iter().any(|panel| panel.active_row == index);
            name.add_theme_color_override("font_color", row_color(active));
        }
        for cell in &mut self.values {
            let active = self
                .panels
                .iter()
                .any(|panel| panel.player == cell.player && panel.active_row == cell.row);
            cell.label
                .add_theme_color_override("font_color", row_color(active));
        }
    }
}

fn row_color(active: bool) -> Color {
    if active { ACTIVE_COLOR } else { INACTIVE_COLOR }
}

#[derive(Debug, Clone, Copy, PartialEq, EnumCount, EnumIter, IntoStaticStr)]
enum OptionRow {
    #[strum(serialize = "Speed Type")]
    SpeedType,
    #[strum(serialize = "Speed Modifier")]
    SpeedModifier,
    #[strum(serialize = "Note Skin")]
    NoteSkin,
    Perspective,
    #[strum(serialize = "Grade Layer")]
    GradeLayer,
    #[strum(serialize = "Grade Position")]
    GradePosition,
}

/// The grade position steps between the top and bottom screen edges in
/// tenths, shown as a percentage.
const GRADE_POSITION_STEP: Percent = Percent(10.0);

fn row(index: usize) -> OptionRow {
    OptionRow::iter().nth(index).expect("row index is wrapped")
}

/// The grade tier a note plays to, by its 8th-note position so it stays
/// put however the rows are filtered on a rebuild.
fn note_tier(beat: Beat) -> usize {
    let ordinal = (beat.0 * 2.0).round() as i64;
    PREVIEW_GRADES[ordinal.rem_euclid(PREVIEW_GRADES.len() as i64) as usize]
}

/// The moment an arrow stops being pressed: a hold's tail, or a tap's
/// brief release window.
fn arrow_until(row: &Row, arrow: &Arrow, timing: &StepfileTiming) -> Seconds {
    match arrow.tail {
        Some(tail) => timing.seconds_at_beat(tail.end),
        None => timing.seconds_at_beat(row.beat) + AUTOPLAY_TAP_HOLD,
    }
}

/// The moment a row stops being pressable, for the rebuild's live-rows
/// filter.
fn row_until(row: &Row, timing: &StepfileTiming) -> Seconds {
    row.arrows
        .iter()
        .map(|arrow| arrow_until(row, arrow, timing))
        .fold(timing.seconds_at_beat(row.beat), Seconds::max)
}

/// The timing error to press a note with so it grades to `tier`: the
/// midpoint of the tier's window.
fn autoplay_offset(config: &GameConfig, tier: usize) -> Option<Seconds> {
    let dynamic = &config.grading.dynamic;
    let def = dynamic.get(tier)?;
    let lower = if tier == 0 {
        Seconds::ZERO
    } else {
        dynamic[tier - 1].window
    };
    Some(Seconds((lower.0 + def.window.0) / 2.0))
}

/// The band-tall canvas a preview surface frames, its width following the
/// surface's aspect.
fn band_canvas(surface: Vector2) -> Vector2 {
    let aspect = surface.x.max(1.0) / surface.y.max(1.0);
    Vector2::new(PREVIEW_BAND * aspect, PREVIEW_BAND)
}

/// A single field's arrows at the play stage's design-canvas size, so the
/// preview reads as a shrunk play stage.
fn preview_arrow_size(config: &GameConfig) -> f32 {
    fitted_arrow_size(
        4.0,
        SCREEN_SIZE.x - 2.0 * config.stage.margin_x,
        max_arrow_size(config, 1.0),
    )
}

/// The mocked chart: `U, L, D-hold, U, R, D-hold` per measure — a mix of
/// 4th and 8th notes with two short holds — repeated across the music's
/// loop.
fn mocked_rows(timing: &StepfileTiming, start: Seconds, length: Seconds) -> Vec<Row> {
    // (beat within the measure, column L=0/D=1/U=2/R=3, hold end beat, quant).
    const PATTERN: [(f64, usize, Option<f64>, u32); 6] = [
        (0.0, 2, None, 4),
        (0.5, 0, None, 8),
        (1.0, 1, Some(1.5), 4),
        (2.0, 2, None, 4),
        (2.5, 3, None, 8),
        (3.0, 1, Some(3.5), 4),
    ];
    let first = (timing.beat_at_seconds(start).0 / 4.0).ceil() as i64;
    let last = ((timing.beat_at_seconds(start + length).0 / 4.0).floor() as i64 - 1).max(first);
    (first..=last)
        .flat_map(|measure| {
            let base = measure as f64 * 4.0;
            PATTERN
                .iter()
                .map(move |&(offset, column, hold_end, quant)| Row {
                    beat: Beat(base + offset),
                    quant,
                    arrows: vec![Arrow {
                        column,
                        tail: hold_end.map(|end| Tail {
                            end: Beat(base + end),
                            roll: false,
                        }),
                    }],
                })
        })
        .collect()
}

/// Steps the row's value; the ends do not wrap. Switching the speed type
/// resets the modifier to the new type's default — they are one value in
/// reality (see [`NoteSpeed`]).
fn change_value(
    row: OptionRow,
    delta: i32,
    options: &mut PlayerOptions,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> bool {
    match row {
        OptionRow::SpeedType => {
            let switched = match (options.note_speed, delta) {
                (NoteSpeed::Dynamic(_), -1) => {
                    NoteSpeed::Constant(config.speed_modifiers.constant.default)
                }
                (NoteSpeed::Constant(_), 1) => {
                    NoteSpeed::Dynamic(config.speed_modifiers.dynamic.default)
                }
                _ => return false,
            };
            options.note_speed = switched;
            true
        }
        OptionRow::SpeedModifier => {
            let set = config.speed_modifiers.set(options.note_speed);
            let index = selected_index(&set.options, options.note_speed.value());
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(value) = set.options.get(stepped).copied() else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.note_speed = match options.note_speed {
                NoteSpeed::Constant(_) => NoteSpeed::Constant(value),
                NoteSpeed::Dynamic(_) => NoteSpeed::Dynamic(value),
            };
            true
        }
        OptionRow::NoteSkin => {
            let index = skins
                .skins
                .iter()
                .position(|skin| skin.name == options.note_skin)
                .unwrap_or(0);
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(skin) = skins.skins.get(stepped) else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.note_skin = skin.name.clone();
            true
        }
        OptionRow::Perspective => {
            let all: Vec<Perspective> = Perspective::iter().collect();
            let index = all
                .iter()
                .position(|perspective| *perspective == options.perspective)
                .unwrap_or(0);
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(perspective) = all.get(stepped) else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.perspective = *perspective;
            true
        }
        OptionRow::GradeLayer => {
            let all: Vec<GradeLayer> = GradeLayer::iter().collect();
            let index = all
                .iter()
                .position(|layer| *layer == options.grade_layer)
                .unwrap_or(0);
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(layer) = all.get(stepped) else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.grade_layer = *layer;
            true
        }
        OptionRow::GradePosition => {
            let stepped = Percent(
                (options.grade_position.0 + delta as f32 * GRADE_POSITION_STEP.0).clamp(0.0, 100.0),
            );
            if stepped == options.grade_position {
                return false;
            }
            options.grade_position = stepped;
            true
        }
    }
}

/// The label of the row's currently selected value.
fn row_value(
    row: OptionRow,
    options: &PlayerOptions,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> String {
    match row {
        OptionRow::SpeedType => match options.note_speed {
            NoteSpeed::Constant(_) => "Constant".to_string(),
            NoteSpeed::Dynamic(_) => "Dynamic".to_string(),
        },
        OptionRow::SpeedModifier => {
            let set = config.speed_modifiers.set(options.note_speed);
            let index = selected_index(&set.options, options.note_speed.value());
            format_modifier(set.options[index], options.note_speed)
        }
        OptionRow::NoteSkin => skins
            .skins
            .iter()
            .find(|skin| skin.name == options.note_skin)
            .map(|skin| skin.display_name.clone())
            .unwrap_or_else(|| options.note_skin.clone()),
        OptionRow::Perspective => <&str>::from(options.perspective).to_string(),
        OptionRow::GradeLayer => <&str>::from(options.grade_layer).to_string(),
        OptionRow::GradePosition => format!("{:.0}%", options.grade_position.0),
    }
}

/// The option closest to the current value; exact when the value came from
/// the same list.
fn selected_index(options: &[f32], value: f32) -> usize {
    options
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - value).abs().total_cmp(&(*b - value).abs()))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// Dynamic multipliers always render with an `x` suffix.
fn format_modifier(value: f32, speed: NoteSpeed) -> String {
    match speed {
        NoteSpeed::Constant(_) => format_value(value),
        NoteSpeed::Dynamic(_) => format!("{}x", format_value(value)),
    }
}

fn format_value(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
