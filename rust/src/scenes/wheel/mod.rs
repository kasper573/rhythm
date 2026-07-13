mod info_panel;
mod player_options;
mod ratings;
mod wash;

use self::player_options::OptionsModal;
use self::ratings::RatingUi;
use self::wash::Wash;
use crate::core::config::{RhythmCycle, config};
use crate::core::font::{TextPivot, label, place_label};
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::library::{StepfileId, StepfileLibrary, library};
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::screen::visible_rect;
use crate::core::settings::Settings;
use crate::core::sfx::Sfx;
use crate::core::stepfile::{MusicPlayer, Stepfile, StepsType};
use crate::core::textures::PendingTexture;
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::menu::NavInput;
use crate::scenes::{GameScene, change_scene, scene_accepts_input};
use godot::classes::control::LayoutPreset;
use godot::classes::{
    ColorRect, Control, IControl, Image, ImageTexture, Label, Node2D, Sprite2D, TextureRect,
};
use godot::prelude::*;

const ROW_HEIGHT: f32 = 56.0;
const BAR_WIDTH: f32 = 660.0;
const BAR_HEIGHT: f32 = 50.0;
/// Bar center of the middle row; bars reach past the right screen edge.
const WHEEL_X: f32 = 330.0;
/// Rows shift right as they leave the center, curving the wheel.
const BULGE_PER_ROW: f32 = 3.0;
const BANNER_SIZE: Vector2 = Vector2::new(DETAILS_BOX_SIZE.x, 168.0);
const BACKDROP_COLOR: Color = Color::from_rgb(0.05, 0.085, 0.03);
const STEPFILE_BAR: Color = Color::from_rgb(0.10, 0.19, 0.07);
const GROUP_BAR: Color = Color::from_rgb(0.055, 0.10, 0.045);
const BORDER_COLOR: Color = Color::from_rgb(0.97, 1.0, 0.62);
const STEPFILE_TEXT: Color = Color::from_rgb(0.35, 0.95, 0.4);
const ACTIVE_STEPFILE_TEXT: Color = Color::from_rgb(0.8, 1.0, 0.75);
const GROUP_TEXT: Color = Color::from_rgb(0.95, 0.55, 0.15);
const ARTIST_TEXT: Color = Color::from_rgb(0.25, 0.75, 0.35);
const BPM_TEXT: Color = Color::from_rgb(0.85, 0.95, 0.55);
const BANNER_TINT: Color = Color::from_rgb(0.10, 0.18, 0.07);
const BANNER_TEXT: Color = Color::from_rgb(0.9, 1.0, 0.85);
const STATS_TEXT: Color = Color::from_rgb(0.75, 0.9, 0.7);
const HELP_TEXT: Color = Color::from_rgb(0.5, 0.62, 0.5);

/// The contrast box behind the stepfile details column. The banner sits
/// flush against its top and sides; only the content below is padded.
const DETAILS_BOX_SIZE: Vector2 = Vector2::new(540.0, 530.0);
const DETAILS_BOX_CENTER: Vector2 = Vector2::new(-320.0, 12.0);
/// Composites like a 50% black overlay: blending happens on linear color,
/// so matching an sRGB-space half-black needs `1 - 0.5^2.2`.
const DETAILS_BOX_ALPHA: f32 = 0.78;
/// The wheel's exponential settle rate, shared by the background
/// cross-fade so both animations move in lockstep.
const WHEEL_EASE_RATE: f32 = 14.0;

/// Scrolling must settle before the music, background wash, and info
/// panel react, so passing rows don't each load media.
const SETTLE_DELAY: Seconds = Seconds(0.35);

/// Once every beat, apex on it, decaying cubically until the next.
const HIGHLIGHT_PULSE: RhythmCycle = RhythmCycle {
    speed: 4.0,
    easing: [0.32, 0.0, 0.67, 0.0],
};

/// How long ¤Select¤ must be held to open the player options.
const OPTIONS_HOLD: Seconds = Seconds(0.5);

#[derive(Clone, Copy)]
pub(super) enum WheelEntry {
    Group { index: usize },
    Stepfile { id: StepfileId },
}

/// One spawned row slot: an unscaled root the animation places, carrying
/// the bar art, its texts, and the per-player rating widgets.
pub(super) struct SlotUi {
    root: Gd<Node2D>,
    bar: Gd<Sprite2D>,
    title: Gd<Label>,
    artist: Gd<Label>,
    ratings: PerPlayer<RatingUi>,
}

/// The ¤Select¤ hold state: only presses that began in browse are armed —
/// ¤Select¤ also closes the modal, and that press must not tap when browse
/// resumes.
#[derive(Default, Clone, Copy)]
struct SelectHold {
    held: Seconds,
    armed: bool,
}

/// The wheel scene: the stepfile browser. Every active player scrolls the
/// one wheel (in versus they race for it); the settled row drives the
/// scene's music, background wash, and info panel. Holding ¤Select¤ opens
/// the player options modal on top; input routes to exactly one of the two.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct WheelScene {
    players: Vec<PlayerId>,
    steps_type: StepsType,
    entries: Vec<WheelEntry>,
    active: usize,
    slots: Vec<SlotUi>,
    /// Rows of visual displacement remaining from recent navigation; eased
    /// back to zero every frame so the active item spins into the center.
    scroll_offset: f32,
    expanded_group: Option<usize>,
    /// The generated rounded-gradient texture shared by bars and panels.
    bar_texture: Gd<ImageTexture>,
    dirty: bool,
    /// Time since the last scroll step; the heavyweight reactions (music,
    /// wash, info panel) wait out [`SETTLE_DELAY`] so rows scrolling past
    /// cost nothing.
    settle: Seconds,
    just_settled: bool,
    select_holds: PerPlayer<SelectHold>,
    /// The centered canvas space every world-placed piece parents under.
    canvas: Option<Gd<Node2D>>,
    highlight: Option<Gd<Sprite2D>>,
    info_panel: Option<Gd<Node2D>>,
    banner: Option<(PendingTexture, Gd<TextureRect>)>,
    wash: Wash,
    modal: Option<OptionsModal>,
    base: Base<Control>,
}

#[godot_api]
impl WheelScene {
    pub fn instantiate(game: &mut Game) -> Gd<WheelScene> {
        let mut scene = WheelScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        Settings::singleton()
            .signals()
            .changed()
            .connect_other(&scene, WheelScene::on_settings_changed);

        let mut backdrop = ColorRect::new_alloc();
        backdrop.set_color(BACKDROP_COLOR);
        backdrop.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        backdrop.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        scene.add_child(&backdrop);

        let mut canvas = Node2D::new_alloc();
        scene.add_child(&canvas);

        let mode = game.play_mode();
        // Only the target row's group starts expanded.
        let target = game
            .take_wheel_target()
            .or_else(|| wheel_default_selection(library()))
            .or_else(|| {
                (!library().is_empty()).then_some(StepfileId {
                    group: 0,
                    stepfile: 0,
                })
            });
        let expanded_group = target.map(|id| id.group);
        let entries = build_entries(library(), expanded_group, &mode.steps_type());
        let active = target
            .and_then(|id| {
                entries.iter().position(
                    |entry| matches!(entry, WheelEntry::Stepfile { id: entry_id } if *entry_id == id),
                )
            })
            .unwrap_or(0);

        let bar_texture = rounded_texture(512, 64, 16.0, None);
        let mut details_box = Sprite2D::new_alloc();
        details_box.set_texture(&rounded_texture(
            DETAILS_BOX_SIZE.x as u32,
            DETAILS_BOX_SIZE.y as u32,
            5.0,
            None,
        ));
        details_box.set_modulate(Color::from_rgba(0.0, 0.0, 0.0, DETAILS_BOX_ALPHA));
        details_box.set_position(Vector2::new(DETAILS_BOX_CENTER.x, -DETAILS_BOX_CENTER.y));
        details_box.set_z_index(45);
        canvas.add_child(&details_box);

        // The active-row frame: a fixed overlay over the center slot that
        // rows slide beneath; once the wheel rests it reads as the row's
        // border.
        let overlay_size = Vector2::new(BAR_WIDTH + 10.0, BAR_HEIGHT + 10.0);
        let mut highlight = Sprite2D::new_alloc();
        highlight.set_texture(&rounded_texture(
            overlay_size.x as u32,
            overlay_size.y as u32,
            18.0,
            Some(5.0),
        ));
        highlight.set_modulate(BORDER_COLOR);
        highlight.set_position(Vector2::new(WHEEL_X, 0.0));
        highlight.set_z_index(120);
        canvas.add_child(&highlight);

        if entries.is_empty() {
            let message = format!("No stepfiles with {} charts found", mode.label());
            let mut empty = label(&message, 30.0, Color::from_rgb(0.9, 0.4, 0.4));
            canvas.add_child(&empty);
            place_label(&mut empty, Vector2::ZERO, TextPivot::CENTER);
            empty.set_z_index(200);
        }

        let mut help = label(
            "up/down: change difficulty\nhold select: change options",
            20.0,
            HELP_TEXT,
        );
        help.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        canvas.add_child(&help);
        place_label(&mut help, Vector2::new(-320.0, 214.0), TextPivot::CENTER);
        help.set_z_index(50);

        let mut bound = scene.bind_mut();
        bound.players = mode.players().to_vec();
        bound.steps_type = mode.steps_type();
        bound.entries = entries;
        bound.active = active;
        bound.expanded_group = expanded_group;
        bound.bar_texture = bar_texture;
        bound.canvas = Some(canvas);
        bound.highlight = Some(highlight);
        bound.dirty = true;
        bound.mark_settled();
        drop(bound);
        scene
    }

    /// Discrete actions (anything but scrolling) take effect immediately.
    fn mark_settled(&mut self) {
        self.settle = SETTLE_DELAY;
        self.just_settled = true;
    }

    /// The chart this stepfile would play `player`, honoring their
    /// preferred difficulty.
    pub(super) fn chart_for(&self, stepfile: &Stepfile, player: PlayerId) -> Option<usize> {
        let preferred = Game::singleton().bind().preferred_difficulty();
        stepfile.closest_chart(&self.steps_type, preferred[player])
    }

    pub(super) fn slot_entry(&self, slot: usize) -> Option<WheelEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let len = self.entries.len() as i64;
        let index =
            (self.active as i64 + slot as i64 - (self.slots.len() / 2) as i64).rem_euclid(len);
        self.entries.get(index as usize).copied()
    }

    /// Every active player scrolls the one wheel; in versus they race
    /// for it.
    /// The settings' `changed` signal: previews mirror the options they
    /// render, so an open modal rebuilds them.
    fn on_settings_changed(&mut self) {
        if let Some(modal) = &mut self.modal {
            modal.mark_rebuild();
        }
    }

    fn navigate(&mut self) {
        for pulse in NavInput::pulses() {
            if self.entries.is_empty() {
                return;
            }
            let Some((player, direction)) = pulse.as_step() else {
                continue;
            };
            if !self.players.contains(&player) {
                continue;
            }
            let step: i64 = match direction {
                StepDirection::Left => -1,
                StepDirection::Right => 1,
                _ => continue,
            };
            let len = self.entries.len() as i64;
            self.active = (self.active as i64 + step).rem_euclid(len) as usize;
            self.scroll_offset -= step as f32;
            self.dirty = true;
            self.settle = Seconds::ZERO;
            Sfx::WheelMove.play();
        }
    }

    /// Each active player steps their own difficulty with their pad's
    /// up/down.
    fn change_difficulty(&mut self) {
        let Some(WheelEntry::Stepfile { id }) = self.entries.get(self.active).copied() else {
            return;
        };
        let stepfile = &library().stepfile(id).stepfile;
        for player in self.players.clone() {
            let mut delta: i32 = 0;
            if Actions::just_pressed(GameAction::step(player, StepDirection::Up)) {
                delta += 1;
            }
            if Actions::just_pressed(GameAction::step(player, StepDirection::Down)) {
                delta -= 1;
            }
            if delta == 0 {
                continue;
            }
            let charts = stepfile.playable_charts(&self.steps_type);
            let Some(current) = self.chart_for(stepfile, player) else {
                continue;
            };
            let position = charts
                .iter()
                .position(|&index| index == current)
                .expect("current chart comes from the same list");
            let new_position = (position as i32 + delta).clamp(0, charts.len() as i32 - 1) as usize;
            if new_position != position {
                let mut game = Game::singleton();
                let mut preferred = game.bind().preferred_difficulty();
                preferred[player] = stepfile.charts[charts[new_position]].difficulty.rank();
                game.bind_mut().set_preferred_difficulty(preferred);
                self.dirty = true;
                self.mark_settled();
                Sfx::Navigate.play();
            }
        }
    }

    /// Recognizes each active player's ¤Select¤ gesture: holding opens the
    /// player options modal (a shared space either player may toggle), a
    /// shorter tap acts on the active row.
    fn track_select(&mut self, delta: f64) {
        if self.entries.is_empty() {
            return;
        }
        for player in self.players.clone() {
            let select = GameAction::select(player);
            let mut hold = self.select_holds[player];
            if Actions::just_pressed(select) {
                hold.armed = true;
                hold.held = Seconds::ZERO;
            }
            if hold.armed {
                if Actions::pressed(select) {
                    hold.held += Seconds(delta);
                    if hold.held >= OPTIONS_HOLD {
                        hold.armed = false;
                        self.select_holds[player] = hold;
                        Sfx::Select.play();
                        self.open_options();
                        return;
                    }
                } else if Actions::just_released(select) {
                    hold.armed = false;
                    self.select_holds[player] = hold;
                    self.handle_tap();
                    return;
                }
            }
            self.select_holds[player] = hold;
        }
    }

    /// A tap acts on the active row: groups toggle open, stepfiles start
    /// with each active player on their own preferred chart.
    fn handle_tap(&mut self) {
        Sfx::WheelSelect.play();
        match self.entries[self.active] {
            WheelEntry::Group { index } => {
                // Only one group is ever expanded: opening a group closes
                // the previous one, opening it again closes it.
                self.expanded_group = (self.expanded_group != Some(index)).then_some(index);
                self.entries = build_entries(library(), self.expanded_group, &self.steps_type);
                self.active = self
                    .entries
                    .iter()
                    .position(
                        |entry| matches!(entry, WheelEntry::Group { index: i } if *i == index),
                    )
                    .unwrap_or(0);
                self.dirty = true;
                self.mark_settled();
                Sfx::GroupToggle.play();
            }
            WheelEntry::Stepfile { id } => {
                let stepfile = &library().stepfile(id).stepfile;
                let charts: Vec<crate::scenes::play::PlayerChart> = self
                    .players
                    .iter()
                    .map(|player| crate::scenes::play::PlayerChart {
                        player: *player,
                        chart: self
                            .chart_for(stepfile, *player)
                            .expect("listed rows have a playable chart of the wheel's type"),
                    })
                    .collect();
                Game::singleton()
                    .bind_mut()
                    .set_selected_stepfile(crate::scenes::play::SelectedStepfile { id, charts });
                Sfx::StartFile.play();
                change_scene(GameScene::Play);
            }
        }
    }

    fn open_options(&mut self) {
        NavInput::singleton().bind_mut().clear();
        let players = self.players.clone();
        let mut host = self.base().clone().cast::<Control>();
        self.modal = Some(OptionsModal::open(&mut host, players));
    }

    fn handle_cancel(&mut self) {
        if Actions::any_just_pressed(&self.players, GameAction::cancel) {
            Sfx::Cancel.play();
            change_scene(GameScene::ModeSelect);
        }
    }

    /// Advances the settle timer; crossing [`SETTLE_DELAY`] fires the
    /// settled reactions once.
    fn settle_wheel(&mut self, delta: f64) {
        if self.settle >= SETTLE_DELAY {
            return;
        }
        self.settle += Seconds(delta);
        if self.settle >= SETTLE_DELAY {
            self.just_settled = true;
        }
    }

    fn animate_wheel(&mut self, delta: f64) {
        if self.scroll_offset != 0.0 {
            self.scroll_offset *= (-WHEEL_EASE_RATE * delta as f32).exp();
            if self.scroll_offset.abs() < 0.01 {
                self.scroll_offset = 0.0;
            }
        }
        let slots = self.slots.len();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let x = slot_x(index, slots, self.scroll_offset);
            let y = slot_y(index, slots, self.scroll_offset);
            slot.root.set_position(Vector2::new(x, -y));
        }
    }

    /// The settled row's stepfile is the scene's background music; rows
    /// without one (groups) fall back to the default BGM. Switching to what
    /// is already playing is the player's no-op, so rows that resolve to
    /// the same music keep it running uninterrupted.
    fn drive_wheel_bgm(&mut self) {
        if !self.just_settled {
            return;
        }
        let entry = match self.entries.get(self.active) {
            Some(WheelEntry::Stepfile { id }) => library().stepfile(*id),
            _ => &library().default_bgm,
        };
        MusicPlayer::singleton().bind_mut().play(entry.bgm());
    }

    /// Pulses the active-row highlight's opacity between 0.5 and 1 on the
    /// music's beat, apex on the beat; a steady 1 while nothing plays.
    fn pulse_active_row(&mut self) {
        let settings = Settings::singleton();
        let timing = settings.bind().machine().timing.clone();
        let alpha = match MusicPlayer::singleton().bind().visible_beat(&timing) {
            Some(beat) => 0.5 + 0.5 * HIGHLIGHT_PULSE.strike(beat),
            None => 1.0,
        };
        if let Some(highlight) = &mut self.highlight {
            let mut modulate = highlight.get_modulate();
            modulate.a = alpha;
            highlight.set_modulate(modulate);
        }
    }

    fn refresh_wheel_rows(&mut self) {
        if !self.dirty {
            return;
        }
        let center = self.slots.len() / 2;
        for index in 0..self.slots.len() {
            let entry = self.slot_entry(index);
            let slot = &mut self.slots[index];
            slot.bar.set_modulate(match entry {
                Some(WheelEntry::Group { .. }) => GROUP_BAR,
                _ => STEPFILE_BAR,
            });
            let (title, title_color, title_y, artist) = match entry {
                Some(WheelEntry::Group { index }) => (
                    library().groups[index].name.clone(),
                    GROUP_TEXT,
                    0.0,
                    String::new(),
                ),
                Some(WheelEntry::Stepfile { id }) => {
                    let entry = library().stepfile(id);
                    let artist = entry.display_artist();
                    let color = if index == center {
                        ACTIVE_STEPFILE_TEXT
                    } else {
                        STEPFILE_TEXT
                    };
                    (
                        entry.display_title(),
                        color,
                        if artist.is_empty() { 0.0 } else { 9.0 },
                        match artist.is_empty() {
                            true => String::new(),
                            false => format!("/ {artist}"),
                        },
                    )
                }
                None => (String::new(), STEPFILE_TEXT, 0.0, String::new()),
            };
            slot.title.set_text(&title);
            slot.title
                .add_theme_color_override("font_color", title_color);
            place_label(
                &mut slot.title,
                Vector2::new(-BAR_WIDTH / 2.0 + 26.0, -title_y),
                TextPivot::CENTER_LEFT,
            );
            slot.artist.set_text(&artist);
            place_label(
                &mut slot.artist,
                Vector2::new(-BAR_WIDTH / 2.0 + 60.0, 15.0),
                TextPivot::CENTER_LEFT,
            );
        }
    }

    /// Respawns the wheel rows when the window's visible height changes how
    /// many are needed.
    fn fit_wheel_rows(&mut self) {
        let rect = visible_rect(&self.base().clone().upcast::<Control>());
        let slots = slots_for(rect.size.y);
        if slots == self.slots.len() {
            return;
        }
        for slot in &mut self.slots {
            slot.root.queue_free();
        }
        self.slots.clear();
        let Some(canvas) = self.canvas.clone() else {
            return;
        };
        for index in 0..slots {
            let slot = self.spawn_slot(&canvas, index, slots);
            self.slots.push(slot);
        }
        self.dirty = true;
        self.mark_settled();
    }

    fn spawn_slot(&mut self, canvas: &Gd<Node2D>, index: usize, slots: usize) -> SlotUi {
        let mut root = Node2D::new_alloc();
        root.set_position(Vector2::new(
            slot_x(index, slots, 0.0),
            -slot_y(index, slots, 0.0),
        ));
        root.set_z_index(100);
        let mut bar = Sprite2D::new_alloc();
        bar.set_texture(&self.bar_texture);
        bar.set_scale(Vector2::new(BAR_WIDTH / 512.0, BAR_HEIGHT / 64.0));
        bar.set_modulate(STEPFILE_BAR);
        root.add_child(&bar);
        let title = label("", 26.0, STEPFILE_TEXT);
        root.add_child(&title);
        let artist = label("", 17.0, ARTIST_TEXT);
        root.add_child(&artist);
        let ratings = PerPlayer {
            p1: RatingUi::spawn(&mut root, PlayerId::P1),
            p2: RatingUi::spawn(&mut root, PlayerId::P2),
        };
        canvas.clone().add_child(&root);
        SlotUi {
            root,
            bar,
            title,
            artist,
            ratings,
        }
    }
}

#[godot_api]
impl IControl for WheelScene {
    fn init(base: Base<Control>) -> WheelScene {
        WheelScene {
            players: Vec::new(),
            steps_type: StepsType::DanceSingle,
            entries: Vec::new(),
            active: 0,
            slots: Vec::new(),
            scroll_offset: 0.0,
            expanded_group: None,
            bar_texture: ImageTexture::new_gd(),
            dirty: false,
            settle: Seconds::ZERO,
            just_settled: false,
            select_holds: PerPlayer::default(),
            canvas: None,
            highlight: None,
            info_panel: None,
            banner: None,
            wash: Wash::default(),
            modal: None,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        // The canvas space tracks the visible center, so world-placed
        // pieces stay centered whatever the window reveals.
        let rect = visible_rect(&self.base().clone().upcast::<Control>());
        let center = rect.position + rect.size / 2.0;
        if let Some(canvas) = &mut self.canvas {
            canvas.set_position(center);
        }

        if let Some(modal) = &mut self.modal {
            let closed = modal.update(delta);
            if closed {
                self.modal = None;
                NavInput::singleton().bind_mut().clear();
            }
        } else if scene_accepts_input() {
            self.navigate();
            self.change_difficulty();
            self.track_select(delta);
            self.handle_cancel();
        }

        self.fit_wheel_rows();
        self.animate_wheel(delta);
        self.pack_player_ratings();
        self.position_rating_labels();
        self.settle_wheel(delta);
        self.drive_wheel_bgm();
        self.pulse_active_row();
        self.refresh_wash();
        self.fade_wash(delta);
        self.refresh_wheel_rows();
        self.refresh_wheel_ratings();
        self.refresh_info_panel();
        self.poll_banner();
        self.dirty = false;
        self.just_settled = false;
    }

    fn exit_tree(&mut self) {
        MusicPlayer::singleton().bind_mut().stop();
    }
}

fn slot_y(slot: usize, slots: usize, scroll_offset: f32) -> f32 {
    ((slots / 2) as f32 - slot as f32 + scroll_offset) * ROW_HEIGHT
}

/// Rows curve away to the right as they leave the center, like the visible
/// edge of a wheel.
fn slot_x(slot: usize, slots: usize, scroll_offset: f32) -> f32 {
    let rows_from_center = (slots / 2) as f32 - slot as f32 + scroll_offset;
    WHEEL_X + BULGE_PER_ROW * rows_from_center * rows_from_center
}

/// Slots needed to fill the window's visible height — the canvas shows
/// more than the design height when the window is taller than 16:9 — plus
/// one above and below so scrolling never reveals a gap, forced odd so a
/// center slot exists.
fn slots_for(visible_height: f32) -> usize {
    ((visible_height / ROW_HEIGHT).ceil() as usize + 2) | 1
}

/// The wheel lists only what the mode can play: selectable stepfiles with
/// at least one non-empty chart of the given type, and the groups holding
/// them.
fn build_entries(
    library: &StepfileLibrary,
    expanded_group: Option<usize>,
    steps_type: &StepsType,
) -> Vec<WheelEntry> {
    let mut entries = Vec::new();
    for (group_index, group) in library.groups.iter().enumerate() {
        let stepfiles: Vec<usize> = (0..group.stepfiles.len())
            .filter(|index| {
                let stepfile = &group.stepfiles[*index].stepfile;
                stepfile.selectable && !stepfile.playable_charts(steps_type).is_empty()
            })
            .collect();
        if stepfiles.is_empty() {
            continue;
        }
        entries.push(WheelEntry::Group { index: group_index });
        if expanded_group != Some(group_index) {
            continue;
        }
        for stepfile_index in stepfiles {
            entries.push(WheelEntry::Stepfile {
                id: StepfileId {
                    group: group_index,
                    stepfile: stepfile_index,
                },
            });
        }
    }
    entries
}

/// Resolves the configured `wheel_default` `(group, stepfile)` search pair:
/// the first group whose name contains the group string and that holds a
/// stepfile whose title contains the stepfile string, both case-insensitive.
fn wheel_default_selection(library: &StepfileLibrary) -> Option<StepfileId> {
    let (group_search, stepfile_search) = &config().wheel_default;
    let group_search = group_search.to_lowercase();
    let stepfile_search = stepfile_search.to_lowercase();
    for (group_index, group) in library.groups.iter().enumerate() {
        if !group.name.to_lowercase().contains(&group_search) {
            continue;
        }
        let stepfile_index = group.stepfiles.iter().position(|entry| {
            entry
                .display_title()
                .to_lowercase()
                .contains(&stepfile_search)
        });
        if let Some(stepfile_index) = stepfile_index {
            return Some(StepfileId {
                group: group_index,
                stepfile: stepfile_index,
            });
        }
    }
    None
}

/// A white vertical-gradient rounded rectangle for sprites to tint: every
/// bar and panel in this scene, and — with a `hollow_border` — the
/// active-row frame, whose interior fades to a faint wash so the rows
/// beneath stay readable. Generated at the exact size it is drawn so edges
/// and ring stay uniformly thick.
fn rounded_texture(
    width: u32,
    height: u32,
    radius: f32,
    hollow_border: Option<f32>,
) -> Gd<ImageTexture> {
    const INTERIOR_WASH: f32 = 0.18;
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let brightness = 255.0 - 130.0 * (y as f32 / (height - 1) as f32);
        for x in 0..width {
            let to_edge_x =
                (x as f32 + 0.5 - width as f32 / 2.0).abs() - (width as f32 / 2.0 - radius);
            let to_edge_y =
                (y as f32 + 0.5 - height as f32 / 2.0).abs() - (height as f32 / 2.0 - radius);
            let distance = Vector2::new(to_edge_x.max(0.0), to_edge_y.max(0.0)).length() - radius;
            let mut alpha = (0.5 - distance).clamp(0.0, 1.0);
            if let Some(border) = hollow_border {
                let interior = (-distance - border).clamp(0.0, 1.0);
                alpha *= 1.0 - interior * (1.0 - INTERIOR_WASH);
            }
            data.extend_from_slice(&[
                brightness as u8,
                brightness as u8,
                brightness as u8,
                (alpha * 255.0) as u8,
            ]);
        }
    }
    let image = Image::create_from_data(
        width as i32,
        height as i32,
        false,
        godot::classes::image::Format::RGBA8,
        &PackedByteArray::from(data.as_slice()),
    )
    .expect("generated image data is well-formed");
    ImageTexture::create_from_image(&image).expect("generated image becomes a texture")
}
