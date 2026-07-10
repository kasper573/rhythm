use crate::core::config::GameConfig;
use crate::core::high_scores::HighScores;
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::player::{PerPlayer, PlayMode};
use crate::core::sfx::SfxPlayer;
use crate::core::stepfile::{Difficulty, MusicPlayer};
use crate::nodes::fps_overlay::{FpsOverlay, FpsOverlayOptions};
use crate::nodes::menu::NavInput;
use crate::nodes::stepfile_player::note_skin::NoteSkinLibrary;
use crate::scenes::play::SelectedStepfile;
use crate::scenes::score::ScoreResults;
use crate::scenes::{self, GameScene};
use godot::classes::control::LayoutPreset;
use godot::classes::{CanvasLayer, ColorRect, Engine, INode, Node, Os};
use godot::prelude::*;

const FADE_SECONDS: f32 = 0.3;

/// The game's root: it boots the platform and the static data, owns the
/// service singletons and the scene transition, and carries the session
/// state scenes hand each other — the play mode, each player's preferred
/// difficulty, and the consumed route params. Scenes are children swapped
/// while the fade overlay is fully black; a torn-down scene keeps no state
/// of its own.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct Game {
    #[cfg(target_arch = "wasm32")]
    web_boot: Option<crate::web::WebBoot>,
    #[cfg(target_arch = "wasm32")]
    loading: Option<Gd<godot::classes::Label>>,
    scene: GameScene,
    current: Option<Gd<Node>>,
    fade: FadePhase,
    fade_alpha: f32,
    fade_rect: Option<Gd<ColorRect>>,
    play_mode: PlayMode,
    /// The difficulty rank each player is aiming for, kept across
    /// stepfiles and scene visits; each stepfile snaps to its nearest
    /// available chart.
    preferred_difficulty: PerPlayer<u8>,
    wheel_target: Option<StepfileId>,
    selected_stepfile: Option<SelectedStepfile>,
    score_results: Option<ScoreResults>,
    base: Base<Node>,
}

#[derive(Clone, Copy)]
enum FadePhase {
    Idle,
    FadingOut(GameScene),
    FadingIn,
}

#[godot_api]
impl Game {
    pub fn singleton() -> Gd<Game> {
        Engine::singleton()
            .get_singleton("Game")
            .expect("Game is registered at boot")
            .cast()
    }

    /// Drives the mandatory scene transition: fade to black, swap scene
    /// while black, fade back in. All scene switches go through here.
    pub fn change_scene(&mut self, to: GameScene) {
        if !matches!(self.fade, FadePhase::FadingOut(_)) {
            self.fade = FadePhase::FadingOut(to);
        }
    }

    pub fn scene(&self) -> GameScene {
        self.scene
    }

    /// Input is ignored while fading out, to avoid acting on a scene that
    /// is already on its way out.
    pub fn accepts_input(&self) -> bool {
        !matches!(self.fade, FadePhase::FadingOut(_))
    }

    pub fn play_mode(&self) -> PlayMode {
        self.play_mode
    }

    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
    }

    pub fn preferred_difficulty(&self) -> PerPlayer<u8> {
        self.preferred_difficulty
    }

    pub fn set_preferred_difficulty(&mut self, preferred: PerPlayer<u8>) {
        self.preferred_difficulty = preferred;
    }

    /// The wheel row to land on, inserted by whichever scene navigates to
    /// the wheel wanting a specific row active; consumed on enter.
    pub fn set_wheel_target(&mut self, target: StepfileId) {
        self.wheel_target = Some(target);
    }

    pub fn take_wheel_target(&mut self) -> Option<StepfileId> {
        self.wheel_target.take()
    }

    /// The play scene's entry param, inserted by whichever scene starts a
    /// session; consumed on enter.
    pub fn set_selected_stepfile(&mut self, selected: SelectedStepfile) {
        self.selected_stepfile = Some(selected);
    }

    pub fn take_selected_stepfile(&mut self) -> Option<SelectedStepfile> {
        self.selected_stepfile.take()
    }

    /// The score scene's entry param: a finished session's results,
    /// inserted by the play scene; consumed on enter.
    pub fn set_score_results(&mut self, results: ScoreResults) {
        self.score_results = Some(results);
    }

    pub fn take_score_results(&mut self) -> Option<ScoreResults> {
        self.score_results.take()
    }

    fn boot(&mut self) {
        GameConfig::install();
        StepfileLibrary::install();
        NoteSkinLibrary::install();

        let mut engine = Engine::singleton();
        engine.register_singleton("Game", &self.to_gd());
        let nav_input = NavInput::new_alloc();
        engine.register_singleton("NavInput", &nav_input);
        self.base_mut().add_child(&nav_input);
        let settings = crate::core::settings::Settings::new_alloc();
        engine.register_singleton("Settings", &settings);
        self.base_mut().add_child(&settings);
        let high_scores = HighScores::new_alloc();
        engine.register_singleton("HighScores", &high_scores);
        self.base_mut().add_child(&high_scores);
        let music = MusicPlayer::new_alloc();
        engine.register_singleton("MusicPlayer", &music);
        self.base_mut().add_child(&music);
        let sfx = SfxPlayer::new_alloc();
        engine.register_singleton("SfxPlayer", &sfx);
        self.base_mut().add_child(&sfx);
        let touch = crate::core::input::TouchSteps::new_alloc();
        self.base_mut().add_child(&touch);

        // The corner meter above every scene, and the transition overlay
        // above everything; the game boots behind a fully black overlay
        // that fades in like any other scene entrance.
        let mut hud_layer = CanvasLayer::new_alloc();
        hud_layer.set_layer(90);
        hud_layer.add_child(&FpsOverlay::instantiate(FpsOverlayOptions {
            fg: Color::from_rgb(0.0, 1.0, 1.0),
            bg: Color::from_rgb(0.0, 0.0, 0.13),
            edge_padding: 12.0,
        }));
        self.base_mut().add_child(&hud_layer);
        let mut fade_layer = CanvasLayer::new_alloc();
        fade_layer.set_layer(100);
        let mut fade_rect = ColorRect::new_alloc();
        fade_rect.set_color(Color::BLACK);
        fade_rect.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        fade_rect.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        fade_layer.add_child(&fade_rect);
        self.base_mut().add_child(&fade_layer);
        self.fade_rect = Some(fade_rect);
        self.fade = FadePhase::FadingIn;
        self.fade_alpha = 1.0;

        let args: Vec<String> = Os::singleton()
            .get_cmdline_user_args()
            .as_slice()
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        if !crate::dev::dispatch(self, &args) {
            self.swap_to(GameScene::MainMenu);
        }
    }

    fn swap_to(&mut self, scene: GameScene) {
        if let Some(mut current) = self.current.take() {
            current.queue_free();
        }
        self.scene = scene;
        let node = scenes::instantiate_scene(scene, self);
        self.base_mut().add_child(&node);
        self.current = Some(node);
    }

    fn run_fade(&mut self, delta: f32) {
        let step = delta / FADE_SECONDS;
        match self.fade {
            FadePhase::Idle => return,
            FadePhase::FadingOut(to) => {
                self.fade_alpha = (self.fade_alpha + step).min(1.0);
                if self.fade_alpha >= 1.0 {
                    // Swap scenes while the screen is fully black.
                    self.swap_to(to);
                    self.fade = FadePhase::FadingIn;
                }
            }
            FadePhase::FadingIn => {
                self.fade_alpha = (self.fade_alpha - step).max(0.0);
                if self.fade_alpha <= 0.0 {
                    self.fade = FadePhase::Idle;
                }
            }
        }
        let alpha = self.fade_alpha;
        if let Some(rect) = &mut self.fade_rect {
            let mut color = rect.get_color();
            color.a = alpha;
            rect.set_color(color);
        }
        NavInput::singleton()
            .bind_mut()
            .set_suspended(!self.accepts_input());
    }
}

#[godot_api]
impl INode for Game {
    fn init(base: Base<Node>) -> Game {
        Game {
            #[cfg(target_arch = "wasm32")]
            web_boot: None,
            #[cfg(target_arch = "wasm32")]
            loading: None,
            scene: GameScene::MainMenu,
            current: None,
            fade: FadePhase::Idle,
            fade_alpha: 0.0,
            fade_rect: None,
            play_mode: PlayMode::default(),
            preferred_difficulty: PerPlayer {
                p1: Difficulty::Medium.rank(),
                p2: Difficulty::Medium.rank(),
            },
            wheel_target: None,
            selected_stepfile: None,
            score_results: None,
            base,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ready(&mut self) {
        crate::core::platform::install(crate::native::NativePlatform);
        self.boot();
    }

    /// The web boots asynchronously: the asset prefetch must land before
    /// anything reads the asset tree, so the boot waits behind a loading
    /// line while it streams in.
    #[cfg(target_arch = "wasm32")]
    fn ready(&mut self) {
        let mut label = godot::classes::Label::new_alloc();
        label.set_text("Loading…");
        label.add_theme_font_size_override("font_size", 30);
        label.set_anchors_and_offsets_preset(LayoutPreset::CENTER);
        self.base_mut().add_child(&label);
        self.loading = Some(label);
        let host = self.base().clone().upcast::<Node>();
        self.web_boot = Some(crate::web::WebBoot::start(host));
    }

    fn process(&mut self, delta: f64) {
        #[cfg(target_arch = "wasm32")]
        if let Some(boot) = &mut self.web_boot {
            let Some(platform) = boot.poll() else {
                return;
            };
            self.web_boot = None;
            crate::core::platform::install(platform);
            if let Some(mut label) = self.loading.take() {
                label.queue_free();
            }
            self.boot();
        }
        self.run_fade(delta as f32);
    }
}
