use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

static CHARACTERS_CACHE: OnceCell<Vec<CharacterDef>> = OnceCell::new();

use std::sync::Mutex;
pub static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

pub fn init_logging() {
    let log_path = std::env::var("TUX_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".local/share/tux-pet/tux-pet.log"))
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/tux-pet.log"))
        });
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        *LOG_FILE.lock().unwrap() = Some(file);
        eprintln!("[tux-pet] logging to {:?}", log_path);
    }
}

pub fn write_log(msg: &str) {
    eprintln!("{}", msg);
    if let Some(ref mut f) = *LOG_FILE.lock().unwrap() {
        use std::io::Write;
        let _ = writeln!(f, "{}", msg);
        let _ = f.flush();
    }
}

macro_rules! tux_log {
    ($($arg:tt)*) => {{
        let msg = format!("[{:?}] {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
            format!($($arg)*));
        eprintln!("{}", msg);
        if let Some(ref mut f) = *shared::LOG_FILE.lock().unwrap() {
            use std::io::Write;
            let _ = writeln!(f, "{}", msg);
            let _ = f.flush();
        }
    }};
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnimationKind {
    Frames {
        frames: Vec<String>,
        /// lower = faster
        ticks_per_frame: u32,
    },
    Video {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorType {
    Fixed,
    Idle,
    WalkLeft,
    WalkRight,
    RunLeft,
    RunRight,
    ClimbUp,
    ClimbDown,
    Jump,
    FollowCursor,
    Fall,
    Shake,
    Sequence,
}

impl Default for BehaviorType {
    fn default() -> Self { BehaviorType::Idle }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceStep {
    /// ticks this step lasts
    pub duration: u32,
    /// x displacement per tick (positive = right)
    #[serde(default)]
    pub move_x: f64,
    /// y displacement per tick as initial velocity (negative = up, positive = down)
    #[serde(default)]
    pub move_y: f64,
    /// apply gravity each tick (move_y treated as initial vel_y, gravity accumulates)
    #[serde(default)]
    pub gravity: bool,
    /// frame paths for this step (absolute after loading)
    pub frames: Vec<String>,
    /// ticks per frame switch for this step
    #[serde(default = "default_ticks_per_frame")]
    pub ticks_per_frame: u32,
}

fn default_ticks_per_frame() -> u32 { 6 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorDef {
    #[serde(rename = "type")]
    pub behavior_type: BehaviorType,
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// for Sequence type: the steps to execute in order
    #[serde(default)]
    pub steps: Vec<SequenceStep>,
    /// for Sequence type: whether to loop after last step
    #[serde(default)]
    pub loop_sequence: bool,
}

fn default_speed() -> f64 { 1.8 }

impl Default for BehaviorDef {
    fn default() -> Self {
        Self {
            behavior_type: BehaviorType::Idle,
            speed: 1.8,
            steps: Vec::new(),
            loop_sequence: false,
        }
    }
}

impl BehaviorDef {
    pub fn fixed() -> Self { Self { behavior_type: BehaviorType::Fixed, ..Self::default() } }
    pub fn idle()  -> Self { Self { behavior_type: BehaviorType::Idle,  ..Self::default() } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationDef {
    pub id: String,
    pub name: String,
    pub kind: AnimationKind,
    #[serde(default)]
    pub behavior: BehaviorDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDef {
    pub id: String,
    pub name: String,
    pub avatar_path: String,
    pub animations: Vec<AnimationDef>,
    pub rules: Vec<TriggerRule>,
    pub dir: PathBuf,
}

impl CharacterDef {
    pub fn find_animation(&self, id: &str) -> &AnimationDef {
        self.animations.iter().find(|a| a.id == id).unwrap_or(&self.animations[0])
    }
}

#[derive(Debug, Deserialize)]
struct RawAnimationEntry {
    id: String,
    name: String,
    #[serde(rename = "type")]
    anim_type: String,
    #[serde(default)]
    frames: Vec<String>,
    #[serde(default)]
    ticks_per_frame: Option<u32>,
    #[serde(default)]
    video_file: Option<String>,
    #[serde(default)]
    behavior: Option<BehaviorDef>,
}

#[derive(Debug, Deserialize)]
struct RawPetConfig {
    name: String,
    avatar: String,
    animations: Vec<RawAnimationEntry>,
    #[serde(default)]
    rules: Vec<TriggerRule>,
}

fn load_character_from_dir(dir: &Path) -> Option<CharacterDef> {
    let json5_path = dir.join("pet.json5");
    if !json5_path.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&json5_path).ok()?;
    let raw: RawPetConfig = json5::from_str(&text).ok()?;

    let animations = raw.animations.into_iter().filter_map(|a| {
        let kind = match a.anim_type.as_str() {
            "frames" => {
                if a.frames.is_empty() { return None; }
                AnimationKind::Frames {
                    frames: a.frames.iter().map(|f| dir.join(f).to_string_lossy().into_owned()).collect(),
                    ticks_per_frame: a.ticks_per_frame.unwrap_or(6),
                }
            }
            "video" => {
                let vf = a.video_file?;
                let abs = dir.join(&vf);
                AnimationKind::Video {
                    path: abs.to_string_lossy().into_owned(),
                }
            }
            _ => return None,
        };
        let mut behavior = a.behavior.unwrap_or_else(|| match kind {
            AnimationKind::Video { .. } => BehaviorDef::fixed(),
            AnimationKind::Frames { .. } => BehaviorDef::idle(),
        });
        for step in &mut behavior.steps {
            step.frames = step.frames.iter()
                .map(|f| dir.join(f).to_string_lossy().into_owned())
                .collect();
        }
        Some(AnimationDef { id: a.id, name: a.name, kind, behavior })
    }).collect::<Vec<_>>();

    if animations.is_empty() {
        return None;
    }

    let id = dir.file_name()?.to_string_lossy().to_lowercase();
    let avatar_path = dir.join(&raw.avatar).to_string_lossy().into_owned();

    Some(CharacterDef {
        id,
        name: raw.name,
        avatar_path,
        animations,
        rules: raw.rules,
        dir: dir.to_path_buf(),
    })
}

pub fn pets_dir() -> PathBuf {
    if let Ok(env_dir) = std::env::var("TUX_ASSETS") {
        let p = PathBuf::from(&env_dir).join("pet");
        if p.is_dir() { return p; }
    }
    
    let exe = std::env::current_exe().unwrap_or_default();
    if let Some(dev_path) = exe.ancestors().find(|p| p.join("assets/pet").is_dir()) {
        return dev_path.join("assets/pet");
    }
    
    let sys_paths = [
        PathBuf::from("/usr/share/tux-pet/pet"),
        PathBuf::from("/usr/local/share/tux-pet/pet"),
    ];
    for p in &sys_paths {
        if p.is_dir() { return p.clone(); }
    }
    
    if let Some(home) = std::env::var_os("HOME") {
        let user_path = PathBuf::from(home).join(".local/share/tux-pet/pet");
        if user_path.is_dir() { return user_path; }
    }
    
    PathBuf::from("/usr/share/tux-pet/pet")
}

pub fn all_characters() -> &'static [CharacterDef] {
    CHARACTERS_CACHE.get_or_init(|| {
        let dir = pets_dir();
        let mut chars: Vec<CharacterDef> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| load_character_from_dir(&e.path()))
            .collect();
        chars.sort_by(|a, b| a.name.cmp(&b.name));
        chars
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PetConfig {
    pub character: String,
    pub animation: String,
    pub base_scale: f64,
}

impl Default for PetConfig {
    fn default() -> Self {
        Self {
            character: "kitty".to_string(),
            animation: "idle".to_string(),
            base_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetState {
    pub hunger: f64,
    pub mood: f64,
    pub energy: f64,
    pub idle_ticks: u64,
    pub last_interaction_ts: u64,
    pub current_animation: String,
    pub animation_ticks_left: u64,
}

impl Default for PetState {
    fn default() -> Self {
        Self {
            hunger: 30.0,
            mood: 70.0,
            energy: 100.0,
            idle_ticks: 0,
            last_interaction_ts: 0,
            current_animation: "idle".to_string(),
            animation_ticks_left: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRule {
    pub trigger_type: TriggerType,
    #[serde(default)]
    pub condition: Option<Condition>,
    pub animation: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default)]
    pub cooldown_ticks: u64,
    #[serde(default)]
    pub duration_ticks: u64,
}

fn default_weight() -> f64 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerType {
    Idle {
        min_ticks: u64,
        max_ticks: u64,
    },
    State {
        state: String,
        op: CompareOp,
        value: f64,
    },
    Time {
        hour_start: u32,
        hour_end: u32,
    },
    Random {
        chance_per_tick: f64,
    },
    MouseHover,
    MouseClick,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub state: String,
    pub op: CompareOp,
    pub value: f64,
}

impl PetState {
    pub fn tick(&mut self, is_moving: bool) {
        self.idle_ticks += 1;

        if self.animation_ticks_left > 0 {
            self.animation_ticks_left -= 1;
        }

        self.hunger = (self.hunger + 0.001).min(100.0);

        if is_moving {
            self.energy = (self.energy - 0.002).max(0.0);
        } else {
            self.energy = (self.energy - 0.0005).max(0.0);
            self.mood = (self.mood + 0.0005).min(100.0);
        }
    }

    pub fn interact(&mut self) {
        self.mood = (self.mood + 5.0).min(100.0);
        self.idle_ticks = 0;
        self.last_interaction_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    pub fn feed(&mut self) {
        self.hunger = (self.hunger - 30.0).max(0.0);
        self.mood = (self.mood + 10.0).min(100.0);
    }

    pub fn get_state_value(&self, state_name: &str) -> f64 {
        match state_name {
            "hunger" => self.hunger,
            "mood" => self.mood,
            "energy" => self.energy,
            "idle_ticks" => self.idle_ticks as f64,
            _ => 0.0,
        }
    }

    pub fn check_condition(&self, cond: &Condition) -> bool {
        let val = self.get_state_value(&cond.state);
        match cond.op {
            CompareOp::Gt => val > cond.value,
            CompareOp::Lt => val < cond.value,
            CompareOp::Gte => val >= cond.value,
            CompareOp::Lte => val <= cond.value,
            CompareOp::Eq => (val - cond.value).abs() < 0.01,
        }
    }
}

use rand::Rng;

pub fn select_animation(
    state: &PetState,
    rules: &[TriggerRule],
    current_time_hour: u32,
) -> Option<String> {
    let mut rng = rand::thread_rng();
    let mut candidates: Vec<(String, f64)> = Vec::new();

    for rule in rules {
        let triggered = match &rule.trigger_type {
            TriggerType::Idle { min_ticks, max_ticks } => {
                state.idle_ticks >= *min_ticks
                    && state.idle_ticks <= *max_ticks
                    && state.animation_ticks_left == 0
            }
            TriggerType::State { state: s, op, value } => {
                let val = state.get_state_value(s);
                match op {
                    CompareOp::Gt => val > *value,
                    CompareOp::Lt => val < *value,
                    CompareOp::Gte => val >= *value,
                    CompareOp::Lte => val <= *value,
                    CompareOp::Eq => (val - *value).abs() < 0.01,
                }
            }
            TriggerType::Time { hour_start, hour_end } => {
                if hour_start <= hour_end {
                    current_time_hour >= *hour_start && current_time_hour < *hour_end
                } else {
                    current_time_hour >= *hour_start || current_time_hour < *hour_end
                }
            }
            TriggerType::Random { chance_per_tick } => {
                rng.gen::<f64>() < *chance_per_tick
            }
            TriggerType::MouseHover => false,
            TriggerType::MouseClick => false,
        };

        if triggered {
            if let Some(ref cond) = rule.condition {
                if !state.check_condition(cond) {
                    continue;
                }
            }

            candidates.push((rule.animation.clone(), rule.weight));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    let total_weight: f64 = candidates.iter().map(|(_, w)| w).sum();
    let mut roll = rng.gen::<f64>() * total_weight;

    for (anim, weight) in &candidates {
        roll -= weight;
        if roll <= 0.0 {
            return Some(anim.clone());
        }
    }

    candidates.last().map(|(a, _)| a.clone())
}

pub fn state_file_path() -> Option<std::path::PathBuf> {
    let mut p = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    p.push(".config/tux");
    let _ = std::fs::create_dir_all(&p);
    p.push("pet_state.json");
    Some(p)
}

pub fn load_pet_state() -> PetState {
    if let Some(path) = state_file_path() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str::<PetState>(&text) {
                return state;
            }
        }
    }
    PetState::default()
}

pub fn save_pet_state(state: &PetState) {
    if let Some(path) = state_file_path() {
        if let Ok(s) = serde_json::to_string(state) {
            let _ = std::fs::write(path, s);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterId {
    Tux,
    Cat,
    Dog,
    Rabbit,
}

impl CharacterId {
    pub const ALL: &[CharacterId] = &[
        CharacterId::Tux,
        CharacterId::Cat,
        CharacterId::Dog,
        CharacterId::Rabbit,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            CharacterId::Tux => "Tux",
            CharacterId::Cat => "Cat",
            CharacterId::Dog => "Dog",
            CharacterId::Rabbit => "Rabbit",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            CharacterId::Tux => "tux",
            CharacterId::Cat => "cat",
            CharacterId::Dog => "dog",
            CharacterId::Rabbit => "rabbit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationId {
    Idle,
    Walk,
    Blink,
    Play,
    ChaseButterfly,
}

impl AnimationId {
    pub const ALL: &[AnimationId] = &[
        AnimationId::Idle,
        AnimationId::Walk,
        AnimationId::Blink,
        AnimationId::Play,
        AnimationId::ChaseButterfly,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            AnimationId::Idle => "Idle",
            AnimationId::Walk => "Walk",
            AnimationId::Blink => "Blink",
            AnimationId::Play => "Play",
            AnimationId::ChaseButterfly => "Chase",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            AnimationId::Idle => "idle",
            AnimationId::Walk => "walk",
            AnimationId::Blink => "blink",
            AnimationId::Play => "play",
            AnimationId::ChaseButterfly => "chase",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum MusicCommand {
    Scan { folder: String },
    Play { index: Option<usize> },
    Pause,
    Toggle,
    Next,
    Prev,
    SetVolume { volume: f32 },
    CyclePlayMode,
    ToggleLyric,
    Status,
    Stop,
    GetHotkeys,
    SetHotkey { action: HotkeyAction, hotkey: Option<Hotkey> },
    ResetHotkeys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    Prev,
    Toggle,
    Next,
    Like,
    ToggleLyric,
    ToggleTodo,
    TogglePomodoro,
    ToggleClipboard,
    ToggleWallpaper,
    SwitchWallpaper,
    ScreenshotFull,
    ScreenshotRegion,
}

impl HotkeyAction {
    pub const ALL: &[HotkeyAction] = &[
        HotkeyAction::Prev,
        HotkeyAction::Toggle,
        HotkeyAction::Next,
        HotkeyAction::Like,
        HotkeyAction::ToggleLyric,
        HotkeyAction::ToggleTodo,
        HotkeyAction::TogglePomodoro,
        HotkeyAction::ToggleClipboard,
        HotkeyAction::ToggleWallpaper,
        HotkeyAction::SwitchWallpaper,
        HotkeyAction::ScreenshotFull,
        HotkeyAction::ScreenshotRegion,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            HotkeyAction::Prev           => "上一曲",
            HotkeyAction::Toggle         => "播放 / 暂停",
            HotkeyAction::Next           => "下一曲",
            HotkeyAction::Like           => "喜欢",
            HotkeyAction::ToggleLyric    => "桌面歌词",
            HotkeyAction::ToggleTodo     => "待办",
            HotkeyAction::TogglePomodoro => "番茄钟",
            HotkeyAction::ToggleClipboard => "剪贴板",
            HotkeyAction::ToggleWallpaper => "壁纸面板",
            HotkeyAction::SwitchWallpaper => "切换壁纸",
            HotkeyAction::ScreenshotFull  => "截全屏",
            HotkeyAction::ScreenshotRegion => "截区域",
        }
    }

    pub fn group(&self) -> &'static str {
        match self {
            HotkeyAction::Prev | HotkeyAction::Toggle | HotkeyAction::Next |
            HotkeyAction::Like | HotkeyAction::ToggleLyric => "音乐",
            HotkeyAction::ToggleTodo | HotkeyAction::TogglePomodoro |
            HotkeyAction::ToggleClipboard | HotkeyAction::ToggleWallpaper => "面板",
            HotkeyAction::SwitchWallpaper => "壁纸",
            HotkeyAction::ScreenshotFull | HotkeyAction::ScreenshotRegion => "截图",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub keysym: u32,
    pub mods: u32,
}

impl Hotkey {
    pub fn label(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.mods & 0x40 != 0 { parts.push("Super"); }
        if self.mods & 0x04 != 0 { parts.push("Ctrl"); }
        if self.mods & 0x08 != 0 { parts.push("Alt"); }
        if self.mods & 0x01 != 0 { parts.push("Shift"); }
        let key = keysym_to_label(self.keysym);
        if parts.is_empty() {
            key
        } else {
            format!("{} + {}", parts.join(" + "), key)
        }
    }
}

pub fn keysym_to_label(sym: u32) -> String {
    match sym {
        0x1008FF14 => "MediaPlay".to_string(),
        0x1008FF15 => "MediaStop".to_string(),
        0x1008FF16 => "MediaPrev".to_string(),
        0x1008FF17 => "MediaNext".to_string(),
        0x1008FF11 => "VolDown".to_string(),
        0x1008FF13 => "VolUp".to_string(),
        0x1008FF12 => "Mute".to_string(),
        0xFF0D => "Enter".to_string(),
        0xFF1B => "Esc".to_string(),
        0xFF09 => "Tab".to_string(),
        0x0020 => "Space".to_string(),
        0xFF50 => "Home".to_string(),
        0xFF57 => "End".to_string(),
        0xFF51 => "←".to_string(),
        0xFF52 => "↑".to_string(),
        0xFF53 => "→".to_string(),
        0xFF54 => "↓".to_string(),
        s if (0xFFBE..=0xFFC9).contains(&s) => format!("F{}", s - 0xFFBE + 1),
        s if (0x0061..=0x007A).contains(&s) => {
            char::from_u32(s - 0x20).map(|c| c.to_string()).unwrap_or_else(|| format!("0x{:04X}", s))
        }
        s if (0x0041..=0x005A).contains(&s) => {
            char::from_u32(s).map(|c| c.to_string()).unwrap_or_else(|| format!("0x{:04X}", s))
        }
        s if (0x0030..=0x0039).contains(&s) => {
            char::from_u32(s).map(|c| c.to_string()).unwrap_or_else(|| format!("0x{:04X}", s))
        }
        s => format!("0x{:04X}", s),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HotkeyResponse {
    pub bindings: Vec<(HotkeyAction, Option<Hotkey>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    #[default]
    Sequential,
    RepeatOne,
    Shuffle,
}

impl PlayMode {
    pub fn label(&self) -> &'static str {
        match self {
            PlayMode::Sequential => "顺序",
            PlayMode::RepeatOne => "单曲",
            PlayMode::Shuffle => "随机",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            PlayMode::Sequential => PlayMode::RepeatOne,
            PlayMode::RepeatOne => PlayMode::Shuffle,
            PlayMode::Shuffle => PlayMode::Sequential,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MusicStatus {
    pub folder: Option<String>,
    pub playlist: Vec<TrackInfo>,
    pub current_index: Option<usize>,
    pub playing: bool,
    pub volume: f32,
    pub position_secs: f64,
    pub play_mode: PlayMode,
    pub lyric_visible: bool,
    #[serde(default)]
    pub scanning: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub hue: f32,
    pub alpha: f32,
    pub font_scale: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self { hue: 220.0, alpha: 0.94, font_scale: 1.0 }
    }
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as i32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r1 + m, g1 + m, b1 + m)
}

