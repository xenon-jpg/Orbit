use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Secret values have redacted Debug output and are erased when dropped.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.expose().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
    pub alias: String,
    pub note: String,
    pub favorite: bool,
    pub avatar_url: String,
    pub robux: Option<u64>,
    pub presence: u8,
    pub friends: Option<u64>,
    pub followers: Option<u64>,
    pub premium: Option<bool>,
    pub groups: Vec<String>,
    pub session_status: String,
    pub checked_at: Option<i64>,
    #[serde(default)]
    pub fps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateAccount {
    pub id: String,
    pub cookie: Secret,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,
    pub theme: String,
    pub accent: String,
    pub background: String,
    pub auto_lock_minutes: u32,
    pub multi_instance: bool,
    pub launch_delay_seconds: u64,
    pub max_clients: u32,
    pub auto_rejoin: bool,
    pub auto_restart: bool,
    pub restart_frozen: bool,
    pub frozen_after_seconds: u64,
    pub memory_limit_mb: u64,
    pub anti_afk: bool,
    pub anti_afk_minutes: u64,
    pub tile_columns: u32,
    pub start_small: bool,
    pub protect_shared_login: bool,
    pub check_roblox_updates: bool,
    pub webhook_enabled: bool,
    pub performance: Performance,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "ru".into(),
            theme: "dark".into(),
            accent: "#c3f36b".into(),
            background: String::new(),
            auto_lock_minutes: 15,
            multi_instance: true,
            launch_delay_seconds: 12,
            max_clients: 10,
            auto_rejoin: false,
            auto_restart: false,
            restart_frozen: false,
            frozen_after_seconds: 300,
            memory_limit_mb: 0,
            anti_afk: false,
            anti_afk_minutes: 8,
            tile_columns: 3,
            start_small: false,
            protect_shared_login: false,
            check_roblox_updates: false,
            webhook_enabled: false,
            performance: Performance::default(),
        }
    }
}
impl Settings {
    pub fn validate(&mut self) {
        self.auto_lock_minutes = self.auto_lock_minutes.clamp(1, 120);
        self.launch_delay_seconds = self.launch_delay_seconds.clamp(8, 120);
        self.max_clients = self.max_clients.clamp(1, 32);
        self.frozen_after_seconds = self.frozen_after_seconds.clamp(180, 3600);
        self.anti_afk_minutes = self.anti_afk_minutes.clamp(2, 15);
        self.tile_columns = self.tile_columns.clamp(1, 6);
        if !["ru", "en"].contains(&self.language.as_str()) {
            self.language = "ru".into();
        }
        if !["dark", "light"].contains(&self.theme.as_str()) {
            self.theme = "dark".into();
        }
        if self.accent.len() != 7
            || !self.accent.starts_with('#')
            || !self.accent[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            self.accent = "#c3f36b".into();
        }
        self.performance.validate();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Performance {
    pub preset: String,
    pub fps: u32,
    pub texture_quality: u32,
    pub disable_grass: bool,
    pub gray_sky: bool,
    pub disable_shadows: bool,
    pub cpu_percent: u32,
    pub fair_cpu: bool,
    pub trim_memory: bool,
    pub tiny_windows: bool,
}
impl Default for Performance {
    fn default() -> Self {
        Self {
            preset: "balanced".into(),
            fps: 30,
            texture_quality: 2,
            disable_grass: false,
            gray_sky: false,
            disable_shadows: false,
            cpu_percent: 0,
            fair_cpu: false,
            trim_memory: false,
            tiny_windows: false,
        }
    }
}
impl Performance {
    pub fn validate(&mut self) {
        self.fps = self.fps.clamp(10, 240);
        self.texture_quality = self.texture_quality.min(3);
        self.cpu_percent = self.cpu_percent.min(100);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Place { place_id: u64 },
    Job { place_id: u64, job_id: String },
    Private { place_id: u64, link_code: String },
    Reserved { place_id: u64, access_code: String },
    Follow { user_id: u64 },
}
impl Target {
    pub fn place_id(&self) -> Option<u64> {
        match self {
            Self::Place { place_id }
            | Self::Job { place_id, .. }
            | Self::Private { place_id, .. }
            | Self::Reserved { place_id, .. } => Some(*place_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub account_ids: Vec<String>,
    pub target: String,
    pub same_server: bool,
    pub follow_first: bool,
    pub performance: Option<Performance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    pub id: String,
    pub name: String,
    pub target: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedClient {
    pub id: String,
    pub account_id: String,
    pub pid: u32,
    pub created_at: u64,
    pub tracker_id: u64,
    pub target: Target,
    pub performance: Performance,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultData {
    pub accounts: Vec<Account>,
    pub secrets: Vec<PrivateAccount>,
    pub settings: Settings,
    pub favorites: Vec<Favorite>,
    pub webhook: Secret,
    pub tracked: Vec<TrackedClient>,
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub account_id: String,
    pub target: Target,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
    pub mode: String,
    pub has_recovery: bool,
    pub hello_available: bool,
    pub platform: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub accounts: Vec<Account>,
    pub settings: Settings,
    pub favorites: Vec<Favorite>,
    pub history: Vec<HistoryEntry>,
    pub clients: Vec<ClientView>,
    pub batches: Vec<BatchView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientView {
    pub id: String,
    pub account_id: String,
    pub pid: u32,
    pub state: String,
    pub reason: String,
    pub started_at: i64,
    pub cpu: f32,
    pub memory_mb: u64,
    pub commit_mb: u64,
    pub gpu: Option<f32>,
    pub target: Target,
    pub game_name: String,
    pub restarts: u32,
    pub last_activity: i64,
    pub window_found: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub id: String,
    pub total: usize,
    pub completed: usize,
    pub state: String,
    pub current_account: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub place_id: u64,
    pub universe_id: u64,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub playing: u64,
    pub visits: u64,
    pub max_players: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub id: String,
    pub playing: u64,
    pub max_players: u64,
    pub ping: Option<u64>,
    pub fps: Option<f64>,
    pub private: bool,
    pub name: String,
    pub link_code: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPage {
    pub servers: Vec<ServerInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    pub at: i64,
    pub code: String,
    pub message: String,
}
