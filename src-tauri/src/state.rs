use crate::{
    error::Result, login::PendingLogin, manager::Manager, model::DiagnosticEvent, roblox::Roblox,
    vault::Vault,
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

pub struct AppState {
    pub vault: Arc<Vault>,
    pub roblox: Arc<Roblox>,
    pub manager: Arc<Manager>,
    pub login: Mutex<Option<PendingLogin>>,
    pub data_dir: PathBuf,
    pub diagnostics: Mutex<VecDeque<DiagnosticEvent>>,
    pub clipboard_generation: std::sync::atomic::AtomicU64,
}
impl AppState {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let vault = Arc::new(Vault::new(data_dir.join("vault.json")));
        let roblox = Arc::new(Roblox::new(vault.clone())?);
        Ok(Self {
            vault,
            roblox,
            manager: Arc::new(Manager::new()),
            login: Mutex::new(None),
            data_dir,
            diagnostics: Mutex::new(VecDeque::new()),
            clipboard_generation: std::sync::atomic::AtomicU64::new(0),
        })
    }
    pub async fn diagnostic(&self, code: &str, message: &str) {
        let mut logs = self.diagnostics.lock().await;
        logs.push_back(DiagnosticEvent {
            at: chrono::Utc::now().timestamp(),
            code: code.chars().take(64).collect(),
            message: message.chars().take(300).collect(),
        });
        while logs.len() > 200 {
            logs.pop_front();
        }
    }
}
