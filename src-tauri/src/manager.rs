use crate::{
    error::{Error, Result},
    model::*,
    roblox::Roblox,
    state::AppState,
    target,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{Emitter, Manager as TauriManager};
use tokio::sync::Mutex;

pub struct Runtime {
    pub tracked: TrackedClient,
    pub state: String,
    pub reason: String,
    pub started_at: i64,
    pub desired: bool,
    pub cpu: f32,
    pub memory_mb: u64,
    pub commit_mb: u64,
    pub restarts: u32,
    pub failures: Vec<i64>,
    pub next_retry: i64,
    pub last_activity: i64,
    pub last_afk: i64,
    pub log: Option<PathBuf>,
    pub log_offset: u64,
    pub disconnected_since: Option<i64>,
    pub hung_samples: u32,
    pub memory_samples: u32,
    pub game_name: String,
    pub layout_applied: bool,
    #[cfg(windows)]
    pub job: Option<crate::platform::win::Handle>,
}
impl Runtime {
    fn view(&self) -> ClientView {
        ClientView {
            id: self.tracked.id.clone(),
            account_id: self.tracked.account_id.clone(),
            pid: self.tracked.pid,
            state: self.state.clone(),
            reason: self.reason.clone(),
            started_at: self.started_at,
            cpu: self.cpu,
            memory_mb: self.memory_mb,
            commit_mb: self.commit_mb,
            gpu: None,
            target: self.tracked.target.clone(),
            game_name: self.game_name.clone(),
            restarts: self.restarts,
            last_activity: self.last_activity,
            window_found: self.window_found(),
            minimized: self.minimized(),
        }
    }
    #[cfg(windows)]
    fn window_found(&self) -> bool {
        !crate::platform::win::windows_for(self.tracked.pid).is_empty()
    }
    #[cfg(not(windows))]
    fn window_found(&self) -> bool {
        false
    }
    #[cfg(windows)]
    fn minimized(&self) -> bool {
        crate::platform::win::windows_for(self.tracked.pid)
            .into_iter()
            .any(|hwnd| {
                unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd) }.as_bool()
            })
    }
    #[cfg(not(windows))]
    fn minimized(&self) -> bool {
        false
    }
}
pub struct Batch {
    pub id: String,
    pub total: usize,
    pub completed: usize,
    pub state: String,
    pub current_account: String,
    pub error: String,
    pub cancel: Arc<AtomicBool>,
}
impl Batch {
    fn view(&self) -> BatchView {
        BatchView {
            id: self.id.clone(),
            total: self.total,
            completed: self.completed,
            state: self.state.clone(),
            current_account: self.current_account.clone(),
            error: self.error.clone(),
        }
    }
}
pub struct Manager {
    pub clients: Mutex<HashMap<String, Runtime>>,
    pub batches: Mutex<Vec<Batch>>,
    launch_gate: Mutex<()>,
    admission_gate: Mutex<()>,
    last_update_check: Mutex<i64>,
    update_notified: Mutex<String>,
    #[cfg(windows)]
    singleton: std::sync::Mutex<Option<crate::platform::win::Singleton>>,
    #[cfg(windows)]
    shared_files: std::sync::Mutex<Vec<std::fs::File>>,
    #[cfg(windows)]
    transaction: Mutex<Option<crate::performance::Transaction>>,
}
impl Manager {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            batches: Mutex::new(Vec::new()),
            launch_gate: Mutex::new(()),
            admission_gate: Mutex::new(()),
            last_update_check: Mutex::new(0),
            update_notified: Mutex::new(String::new()),
            #[cfg(windows)]
            singleton: std::sync::Mutex::new(None),
            #[cfg(windows)]
            shared_files: std::sync::Mutex::new(Vec::new()),
            #[cfg(windows)]
            transaction: Mutex::new(None),
        }
    }
    pub async fn clients(&self) -> Vec<ClientView> {
        let mut clients: Vec<_> = self
            .clients
            .lock()
            .await
            .values()
            .map(Runtime::view)
            .collect();
        clients.sort_by_key(|c| c.started_at);
        clients
    }
    pub async fn batches(&self) -> Vec<BatchView> {
        self.batches.lock().await.iter().map(Batch::view).collect()
    }
    pub async fn active_count(&self) -> usize {
        self.clients
            .lock()
            .await
            .values()
            .filter(|c| c.state != "stopped" && c.state != "failed")
            .count()
    }
    pub async fn cancel(&self, id: &str) -> Result<()> {
        let batches = self.batches.lock().await;
        let batch = batches.iter().find(|b| b.id == id).ok_or(Error::NotFound)?;
        batch.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }
    pub async fn stop_all(&self) -> Result<()> {
        for batch in self.batches.lock().await.iter() {
            batch.cancel.store(true, Ordering::SeqCst);
        }
        let ids: Vec<_> = self.clients.lock().await.keys().cloned().collect();
        for id in ids {
            let _ = self.action(&id, "close", true).await;
        }
        Ok(())
    }
    pub async fn action(&self, id: &str, action: &str, force: bool) -> Result<()> {
        let tracked = {
            let mut clients = self.clients.lock().await;
            let client = clients.get_mut(id).ok_or(Error::ClientGone)?;
            if action == "close" || action == "force-close" {
                client.desired = false;
                client.state = "stopping".into();
            }
            client.tracked.clone()
        };
        #[cfg(windows)]
        {
            use crate::platform::win;
            match action {
                "force-close" => win::terminate(tracked.pid, tracked.created_at)?,
                "close" => {
                    if win::window_action(tracked.pid, tracked.created_at, "close").is_err() {
                        win::terminate(tracked.pid, tracked.created_at)?;
                    } else if force {
                        tokio::time::sleep(Duration::from_secs(4)).await;
                        if win::process_birth(tracked.pid).ok() == Some(tracked.created_at) {
                            let _ = win::terminate(tracked.pid, tracked.created_at);
                        }
                    }
                }
                "trim" => win::trim_memory(tracked.pid, tracked.created_at)?,
                "cpu" => {
                    if let Some(job) = self
                        .clients
                        .lock()
                        .await
                        .get(id)
                        .and_then(|c| c.job.as_ref())
                    {
                        win::set_job_cpu(job, tracked.performance.cpu_percent)?;
                    }
                }
                other => win::window_action(tracked.pid, tracked.created_at, other)?,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (tracked, action, force);
            return Err(Error::WindowsRequired);
        }
        #[cfg(windows)]
        Ok(())
    }
    pub async fn preview(&self, id: &str) -> Result<String> {
        let tracked = self
            .clients
            .lock()
            .await
            .get(id)
            .map(|c| c.tracked.clone())
            .ok_or(Error::ClientGone)?;
        #[cfg(windows)]
        return tokio::task::spawn_blocking(move || {
            crate::platform::win::preview(tracked.pid, tracked.created_at)
        })
        .await
        .map_err(|_| Error::Internal)?;
        #[cfg(not(windows))]
        {
            let _ = tracked;
            Err(Error::WindowsRequired)
        }
    }
    pub async fn tile(&self, columns: u32) -> Result<()> {
        let clients = self.clients.lock().await;
        #[cfg(windows)]
        {
            let pairs: Vec<_> = clients
                .values()
                .filter(|c| c.window_found())
                .map(|c| (c.tracked.pid, c.tracked.created_at))
                .collect();
            return crate::platform::win::tile(&pairs, columns);
        }
        #[cfg(not(windows))]
        {
            let _ = (clients, columns);
            Err(Error::WindowsRequired)
        }
    }

    pub async fn start_batch(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        state: Arc<AppState>,
        options: LaunchOptions,
    ) -> Result<String> {
        let _admission = self.admission_gate.lock().await;
        if options.account_ids.is_empty() || options.account_ids.len() > 32 {
            return Err(Error::Invalid("Select between 1 and 32 accounts."));
        }
        let unique: HashSet<_> = options.account_ids.iter().collect();
        if unique.len() != options.account_ids.len() {
            return Err(Error::Invalid("Select each account only once."));
        }
        let (settings, accounts) = state
            .vault
            .read(|d| Ok((d.settings.clone(), d.accounts.clone())))
            .await?;
        if !cfg!(windows) {
            return Err(Error::WindowsRequired);
        }
        if options
            .account_ids
            .iter()
            .any(|id| !accounts.iter().any(|a| a.id == *id))
        {
            return Err(Error::NotFound);
        }
        let count = self.active_count().await + options.account_ids.len();
        if count > settings.max_clients as usize || (!settings.multi_instance && count > 1) {
            return Err(Error::Invalid(
                "The batch exceeds your client limit or multi-instance is off.",
            ));
        }
        if self
            .batches
            .lock()
            .await
            .iter()
            .any(|b| b.state == "running" || b.state == "queued")
        {
            return Err(Error::Invalid(
                "Wait for the current launch batch to finish.",
            ));
        }
        let mut target = state.roblox.resolve_target(&options.target).await?;
        if options.same_server && options.account_ids.len() > 1 {
            if let Target::Place { place_id } = target {
                target = self
                    .find_server(
                        &state.roblox,
                        &options.account_ids[0],
                        place_id,
                        options.account_ids.len(),
                    )
                    .await?;
            } else if let Target::Follow { user_id } = target {
                let presence = state
                    .roblox
                    .post(
                        &options.account_ids[0],
                        "https://presence.roblox.com/v1/presence/users",
                        serde_json::json!({"userIds":[user_id]}),
                    )
                    .await?;
                let p = &presence["userPresences"][0];
                if p["userPresenceType"].as_u64() == Some(2) {
                    if let (Some(place_id), Some(job_id)) =
                        (p["placeId"].as_u64(), p["gameId"].as_str())
                    {
                        if target::valid_job(job_id) {
                            target = Target::Job {
                                place_id,
                                job_id: job_id.into(),
                            };
                        }
                    }
                }
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let batch = Batch {
            id: id.clone(),
            total: options.account_ids.len(),
            completed: 0,
            state: "queued".into(),
            current_account: String::new(),
            error: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let cancel = batch.cancel.clone();
        {
            let mut batches = self.batches.lock().await;
            batches.push(batch);
            if batches.len() > 20 {
                batches.remove(0);
            }
        }
        let manager = self.clone();
        let returned_id = id.clone();
        tauri::async_runtime::spawn(async move {
            manager
                .run_batch(app, state, id, options, target, settings, cancel)
                .await;
        });
        Ok(returned_id)
    }
    async fn find_server(
        &self,
        api: &Roblox,
        account: &str,
        place_id: u64,
        count: usize,
    ) -> Result<Target> {
        let mut cursor = String::new();
        let mut seen = HashSet::new();
        for _ in 0..4 {
            let page = api
                .servers(Some(account), place_id, false, &cursor, "asc")
                .await?;
            for server in page.servers {
                if seen.insert(server.id.clone())
                    && server.max_players >= server.playing + count as u64
                {
                    return Ok(Target::Job {
                        place_id,
                        job_id: server.id,
                    });
                }
            }
            cursor = page.next_cursor.ok_or(Error::NoServer)?;
        }
        Err(Error::NoServer)
    }
    async fn set_batch(&self, id: &str, f: impl FnOnce(&mut Batch)) {
        if let Some(batch) = self.batches.lock().await.iter_mut().find(|b| b.id == id) {
            f(batch);
        }
    }
    async fn run_batch(
        &self,
        app: tauri::AppHandle,
        state: Arc<AppState>,
        id: String,
        options: LaunchOptions,
        mut target: Target,
        settings: Settings,
        cancel: Arc<AtomicBool>,
    ) {
        self.set_batch(&id, |b| b.state = "running".into()).await;
        let _gate = self.launch_gate.lock().await;
        let mut first_user = None;
        let game_name = match target.place_id() {
            Some(place) => state
                .roblox
                .game(place)
                .await
                .map(|g| g.name)
                .unwrap_or_default(),
            None => String::new(),
        };
        for (index, account_id) in options.account_ids.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                self.set_batch(&id, |b| {
                    b.state = "cancelled".into();
                    b.error = Error::Cancelled.to_string();
                })
                .await;
                break;
            }
            self.set_batch(&id, |b| b.current_account = account_id.clone())
                .await;
            if options.follow_first && index > 0 && !matches!(target, Target::Private { .. }) {
                if let Some(user_id) = first_user {
                    target = Target::Follow { user_id };
                }
            }
            let result = self
                .launch_one(
                    &state,
                    account_id,
                    target.clone(),
                    options
                        .performance
                        .clone()
                        .unwrap_or(settings.performance.clone()),
                    &settings,
                    index == 0,
                    None,
                )
                .await;
            match result {
                Ok(user_id) => {
                    if first_user.is_none() {
                        first_user = Some(user_id);
                    }
                    self.set_batch(&id, |b| b.completed += 1).await;
                    if let Some(client) = self
                        .clients
                        .lock()
                        .await
                        .values_mut()
                        .find(|c| c.tracked.account_id == *account_id)
                    {
                        client.game_name = game_name.clone();
                    }
                    state
                        .roblox
                        .notify(&format!(
                            "AltDeck: launched account {index} of {}.",
                            options.account_ids.len()
                        ))
                        .await;
                }
                Err(error) => {
                    state.diagnostic("launch_failed", &error.to_string()).await;
                    self.set_batch(&id, |b| {
                        b.state = "failed".into();
                        b.error = error.to_string();
                    })
                    .await;
                    state
                        .roblox
                        .notify(&format!("AltDeck batch stopped: {error}"))
                        .await;
                    break;
                }
            }
            if index + 1 < options.account_ids.len() {
                // Cancellation remains responsive during the mandatory stagger.
                for _ in 0..settings.launch_delay_seconds {
                    if cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        self.set_batch(&id, |b| {
            if b.state == "running" {
                b.state = if b.completed == b.total {
                    "complete"
                } else {
                    "cancelled"
                }
                .into();
            }
            b.current_account.clear();
        })
        .await;
        let _ = app.emit("manager-changed", ());
    }
    async fn launch_one(
        &self,
        state: &AppState,
        account_id: &str,
        target: Target,
        mut profile: Performance,
        settings: &Settings,
        first: bool,
        replace_id: Option<&str>,
    ) -> Result<u64> {
        let account = state
            .vault
            .read(|d| {
                d.accounts
                    .iter()
                    .find(|a| a.id == account_id)
                    .cloned()
                    .ok_or(Error::NotFound)
            })
            .await?;
        if let Some(fps) = account.fps {
            profile.fps = fps;
        }
        profile.validate();
        {
            let clients = self.clients.lock().await;
            if clients.values().any(|c| {
                c.tracked.account_id == account_id
                    && c.tracked.id != replace_id.unwrap_or_default()
                    && c.state != "stopped"
                    && c.state != "failed"
            }) {
                return Err(Error::Invalid("This account already has a client."));
            }
        }
        #[cfg(windows)]
        {
            use crate::platform::win;
            let mut system = sysinfo::System::new();
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
            );
            let managed: Vec<_> = self
                .clients
                .lock()
                .await
                .values()
                .map(|c| (c.tracked.pid, c.tracked.created_at))
                .collect();
            let unmanaged = system.processes().values().any(|p| {
                p.name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("RobloxPlayerBeta.exe"))
                    && !p
                        .cmd()
                        .iter()
                        .any(|s| s.to_string_lossy().contains("--launch-to-tray"))
                    && !managed.iter().any(|(pid, birth)| {
                        *pid == p.pid().as_u32() && win::process_birth(*pid).ok() == Some(*birth)
                    })
            });
            if unmanaged {
                return Err(Error::ExistingClients);
            }
            {
                let mut singleton = self.singleton.lock().map_err(|_| Error::Internal)?;
                if singleton.is_none() {
                    *singleton = Some(win::Singleton::acquire(!managed.is_empty())?);
                }
            }
            if settings.protect_shared_login {
                let mut files = self.shared_files.lock().map_err(|_| Error::Internal)?;
                if files.is_empty() {
                    use std::os::windows::fs::OpenOptionsExt;
                    let root =
                        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or(Error::NotInstalled)?)
                            .join("Roblox")
                            .join("LocalStorage");
                    for name in ["RobloxCookies.dat", "appStorage.json"] {
                        let path = root.join(name);
                        if path.exists() {
                            files.push(
                                std::fs::OpenOptions::new()
                                    .read(true)
                                    .write(true)
                                    .share_mode(0)
                                    .open(path)?,
                            );
                        }
                    }
                }
            }
            let executable = win::roblox_path()?;
            let signature_path = executable.clone();
            tokio::task::spawn_blocking(move || win::verify_signature(&signature_path))
                .await
                .map_err(|_| Error::Internal)??;
            let client_dir = executable
                .parent()
                .ok_or(Error::NotInstalled)?
                .to_path_buf();
            let _ = first;
            crate::performance::frame_cap(profile.fps)?;
            let mut transaction = self.transaction.lock().await;
            if let Some(previous) = transaction.take() {
                previous.restore()?;
            }
            *transaction = Some(crate::performance::Transaction::open(
                &client_dir,
                &state.data_dir,
                &profile,
            )?);
            drop(transaction);
            let count = self.active_count().await.max(1) as u32;
            if profile.fair_cpu {
                profile.cpu_percent = (80 / count).clamp(5, 80);
            }
            let ticket = state.roblox.ticket(account_id).await?;
            let tracker = rand::random::<u64>() % 9_000_000_000 + 1_000_000_000;
            let uri = target::launch_uri(
                &ticket,
                &target,
                tracker,
                chrono::Utc::now().timestamp_millis() as u128,
            )?;
            drop(ticket);
            let (pid, job) = win::spawn_client(&uri, profile.cpu_percent)?;
            drop(uri);
            let created_at = win::process_birth(pid)?;
            let tracked = TrackedClient {
                id: uuid::Uuid::new_v4().to_string(),
                account_id: account_id.into(),
                pid,
                created_at,
                tracker_id: tracker,
                target: target.clone(),
                performance: profile,
            };
            let now = chrono::Utc::now().timestamp();
            let runtime = Runtime {
                tracked: tracked.clone(),
                state: "starting".into(),
                reason: String::new(),
                started_at: now,
                desired: true,
                cpu: 0.,
                memory_mb: 0,
                commit_mb: 0,
                restarts: 0,
                failures: Vec::new(),
                next_retry: 0,
                last_activity: now,
                last_afk: now,
                log: None,
                log_offset: 0,
                disconnected_since: None,
                hung_samples: 0,
                memory_samples: 0,
                game_name: String::new(),
                layout_applied: false,
                job: Some(job),
            };
            self.clients
                .lock()
                .await
                .insert(tracked.id.clone(), runtime);
            state
                .vault
                .with(|d| {
                    d.tracked.push(tracked.clone());
                    d.history.insert(
                        0,
                        HistoryEntry {
                            account_id: account_id.into(),
                            target,
                            at: now,
                        },
                    );
                    d.history.truncate(100);
                    Ok(())
                })
                .await?;
            Ok(account.user_id)
        }
        #[cfg(not(windows))]
        {
            let _ = (state, account, target, profile, settings, first);
            Err(Error::WindowsRequired)
        }
    }

    pub async fn restore(self: &Arc<Self>, state: &AppState) {
        #[cfg(windows)]
        {
            let tracked = state
                .vault
                .read(|d| Ok(d.tracked.clone()))
                .await
                .unwrap_or_default();
            let mut recovered = Vec::new();
            for item in tracked {
                if crate::platform::win::process_birth(item.pid).ok() != Some(item.created_at) {
                    continue;
                }
                if let Ok(path) = crate::platform::win::roblox_path() {
                    // A PID and birth time are necessary but not sufficient: require Roblox's path too.
                    let mut system = sysinfo::System::new();
                    let pid = sysinfo::Pid::from_u32(item.pid);
                    system.refresh_processes_specifics(
                        sysinfo::ProcessesToUpdate::Some(&[pid]),
                        true,
                        sysinfo::ProcessRefreshKind::nothing(),
                    );
                    if !system
                        .process(pid)
                        .and_then(|p| p.exe())
                        .is_some_and(|exe| exe == path)
                    {
                        continue;
                    }
                }
                let now = chrono::Utc::now().timestamp();
                let runtime = Runtime {
                    tracked: item.clone(),
                    state: "running".into(),
                    reason: "reattached".into(),
                    started_at: now,
                    desired: true,
                    cpu: 0.,
                    memory_mb: 0,
                    commit_mb: 0,
                    restarts: 0,
                    failures: Vec::new(),
                    next_retry: 0,
                    last_activity: now,
                    last_afk: now,
                    log: None,
                    log_offset: 0,
                    disconnected_since: None,
                    hung_samples: 0,
                    memory_samples: 0,
                    game_name: String::new(),
                    layout_applied: false,
                    job: None,
                };
                recovered.push((item.id, runtime));
            }
            let ids: Vec<_> = recovered.iter().map(|(id, _)| id.clone()).collect();
            let has_recovered = !recovered.is_empty();
            self.clients.lock().await.extend(recovered);
            if has_recovered {
                if let Ok(mut singleton) = self.singleton.lock() {
                    if singleton.is_none() {
                        *singleton = win::Singleton::acquire(true).ok();
                    }
                }
            }
            let _ = state
                .vault
                .with(|d| {
                    d.tracked.retain(|t| ids.contains(&t.id));
                    Ok(())
                })
                .await;
        }
        #[cfg(not(windows))]
        {
            let _ = state;
        }
    }

    pub async fn tick(self: &Arc<Self>, app: &tauri::AppHandle, state: &Arc<AppState>) {
        let data = state.vault.background_data().await.ok();
        if let Some(data) = &data {
            if state
                .vault
                .lock_if_idle(data.settings.auto_lock_minutes)
                .await
            {
                let _ = crate::login::cancel(app, state).await;
                let _ = app.emit("vault-locked", ());
            }
        }
        // Expire abandoned isolated login profiles without ever reading their cookies.
        {
            let mut pending = state.login.lock().await;
            if pending
                .as_ref()
                .is_some_and(|p| crate::login::now() - p.started > 1800)
            {
                if let Some(p) = pending.take() {
                    if let Some(window) = app.get_webview_window(&p.label) {
                        let _ = window.destroy();
                    }
                    crate::login::cleanup(p.directory);
                }
            }
        }
        let now = chrono::Utc::now().timestamp();
        if data
            .as_ref()
            .is_some_and(|d| d.settings.check_roblox_updates)
        {
            let mut last = self.last_update_check.lock().await;
            if now - *last >= 1800 {
                *last = now;
                if matches!(state.roblox.update_available().await, Ok(true)) {
                    let mut notified = self.update_notified.lock().await;
                    if *notified != "available" {
                        *notified = "available".into();
                        state.diagnostic("roblox_update", "A different Roblox version is available. Update through the official Roblox installer.").await;
                        state.roblox.notify("AltDeck: a Roblox update is available. Use the official installer.").await;
                    }
                } else {
                    *self.update_notified.lock().await = "current".into();
                }
            }
        }
        let settings = data
            .as_ref()
            .map(|d| d.settings.clone())
            .unwrap_or_default();
        let mut system = sysinfo::System::new();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory(),
        );
        let cpu_count = system.cpus().len().max(1) as f32;
        let mut relaunch = Vec::new();
        let mut removed = Vec::new();
        {
            let mut clients = self.clients.lock().await;
            for (id, client) in clients.iter_mut() {
                let process = system.process(sysinfo::Pid::from_u32(client.tracked.pid));
                let alive = process
                    .is_some_and(|p| p.start_time() > 0 && self.identity_valid(&client.tracked));
                if !alive {
                    if client.desired
                        && (settings.auto_rejoin || settings.auto_restart)
                        && client.state != "stopping"
                    {
                        client.state = "waiting".into();
                        if client.next_retry == 0 {
                            client.next_retry = now + 15;
                        }
                    } else {
                        client.state = "stopped".into();
                        if !client.desired {
                            removed.push(id.clone());
                        }
                    }
                } else if client.state != "stopping" {
                    if client.state == "starting" && now - client.started_at > 90 {
                        client.state = "running".into();
                    }
                    if client.state == "waiting" {
                        client.state = "running".into();
                        client.next_retry = 0;
                    }
                    client.cpu = process.map(|p| p.cpu_usage() / cpu_count).unwrap_or(0.);
                    if let Some(process) = process {
                        client.memory_mb = process.memory() / 1024 / 1024;
                    }
                    #[cfg(windows)]
                    if let Ok((memory, commit)) =
                        crate::platform::win::memory(client.tracked.pid, client.tracked.created_at)
                    {
                        client.memory_mb = memory;
                        client.commit_mb = commit;
                    }
                    #[cfg(windows)]
                    if !client.layout_applied
                        && client.window_found()
                        && now - client.started_at > 10
                    {
                        if settings.start_small || client.tracked.performance.tiny_windows {
                            let _ = crate::platform::win::window_action(
                                client.tracked.pid,
                                client.tracked.created_at,
                                "small",
                            );
                        }
                        client.layout_applied = true;
                    }
                    self.observe_log(client, &state.data_dir, now);
                    if client.state == "disconnected" && settings.auto_rejoin {
                        if *client.disconnected_since.get_or_insert(now) + 20 < now {
                            client.state = "waiting".into();
                            client.next_retry = now + 5;
                        }
                    }
                    #[cfg(windows)]
                    if settings.restart_frozen {
                        let hung = crate::platform::win::windows_for(client.tracked.pid)
                            .into_iter()
                            .any(|hwnd| {
                                unsafe {
                                    windows::Win32::UI::WindowsAndMessaging::IsHungAppWindow(hwnd)
                                }
                                .as_bool()
                            });
                        client.hung_samples = if hung
                            && now - client.last_activity > settings.frozen_after_seconds as i64
                        {
                            client.hung_samples + 1
                        } else {
                            0
                        };
                        if client.hung_samples >= 3 && settings.auto_restart {
                            client.state = "waiting".into();
                            client.next_retry = now + 5;
                            client.reason = "frozen".into();
                        }
                    }
                    if settings.memory_limit_mb > 0 && settings.auto_restart {
                        client.memory_samples = if client.memory_mb > settings.memory_limit_mb {
                            client.memory_samples + 1
                        } else {
                            0
                        };
                        if client.memory_samples >= 5 {
                            client.state = "waiting".into();
                            client.next_retry = now + 5;
                            client.reason = "memory".into();
                        }
                    }
                    if settings.anti_afk
                        && now - client.last_afk >= settings.anti_afk_minutes as i64 * 60
                        && client.state == "running"
                    {
                        #[cfg(windows)]
                        if crate::platform::win::anti_afk(
                            client.tracked.pid,
                            client.tracked.created_at,
                        )
                        .unwrap_or(false)
                        {
                            client.last_afk = now;
                            state
                                .diagnostic("anti_afk", "Sent foreground keep-alive input.")
                                .await;
                            let _ = app.emit("diagnostic", "anti_afk");
                        }
                    }
                    if client.tracked.performance.trim_memory
                        && client.memory_mb > 1800
                        && now - client.started_at > 120
                    {
                        #[cfg(windows)]
                        {
                            let _ = crate::platform::win::trim_memory(
                                client.tracked.pid,
                                client.tracked.created_at,
                            );
                        }
                    }
                }
                if client.state == "waiting"
                    && client.desired
                    && data.is_some()
                    && client.next_retry <= now
                {
                    client.failures.retain(|t| now - t < 600);
                    if client.failures.len() >= 3 {
                        client.state = "failed".into();
                        client.reason = "breaker".into();
                        client.desired = false;
                    } else {
                        client.next_retry = now + 300;
                        relaunch.push((
                            id.clone(),
                            client.tracked.clone(),
                            client.tracked.performance.clone(),
                        ));
                    }
                }
            }
            for id in &removed {
                clients.remove(id);
            }
        }
        if !removed.is_empty() {
            let _ = state
                .vault
                .with(|d| {
                    d.tracked.retain(|t| !removed.contains(&t.id));
                    Ok(())
                })
                .await;
        }
        for (id, tracked, profile) in relaunch {
            let manager = self.clone();
            let state = state.clone();
            tauri::async_runtime::spawn(async move {
                manager.relaunch(&state, &id, tracked, profile).await;
            });
        }
        #[cfg(windows)]
        {
            let clients = self.clients.lock().await;
            let count = clients
                .values()
                .filter(|c| c.desired && c.state != "stopped" && c.state != "failed")
                .count()
                .max(1);
            for client in clients.values() {
                if client.tracked.performance.fair_cpu {
                    if let Some(job) = &client.job {
                        let _ = crate::platform::win::set_job_cpu(
                            job,
                            (80 / count as u32).clamp(5, 80),
                        );
                    }
                }
            }
            if clients
                .values()
                .all(|c| c.state == "stopped" || c.state == "failed")
            {
                self.shared_files.lock().map(|mut files| files.clear()).ok();
                if let Some(tx) = self.transaction.lock().await.take() {
                    let _ = tx.restore();
                }
            }
        }
        let _ = app.emit("manager-changed", ());
    }
    fn identity_valid(&self, tracked: &TrackedClient) -> bool {
        #[cfg(windows)]
        {
            crate::platform::win::process_birth(tracked.pid).ok() == Some(tracked.created_at)
        }
        #[cfg(not(windows))]
        {
            let _ = tracked;
            false
        }
    }
    async fn relaunch(
        &self,
        state: &AppState,
        id: &str,
        tracked: TrackedClient,
        profile: Performance,
    ) {
        let _gate = self.launch_gate.lock().await;
        {
            let clients = self.clients.lock().await;
            if !clients
                .get(id)
                .is_some_and(|c| c.desired && c.state == "waiting")
            {
                return;
            }
        }
        let old = self
            .clients
            .lock()
            .await
            .get(id)
            .map(|c| (c.restarts, c.failures.clone(), c.last_afk));
        let Some((restarts, mut failures, last_afk)) = old else {
            return;
        };
        if failures.len() >= 3 {
            return;
        }
        #[cfg(windows)]
        if crate::platform::win::process_birth(tracked.pid).ok() == Some(tracked.created_at) {
            let _ = crate::platform::win::terminate(tracked.pid, tracked.created_at);
        }
        self.clients
            .lock()
            .await
            .get_mut(id)
            .map(|c| c.state = "restarting".into());
        let Ok(data) = state.vault.background_data().await else {
            return;
        };
        let settings = data.settings;
        let result = self
            .launch_one(
                state,
                &tracked.account_id,
                tracked.target.clone(),
                profile,
                &settings,
                false,
                Some(&id),
            )
            .await;
        match result {
            Ok(_) => {
                failures.push(chrono::Utc::now().timestamp());
                let previous_name = self
                    .clients
                    .lock()
                    .await
                    .get(id)
                    .map(|c| c.game_name.clone())
                    .unwrap_or_default();
                let mut clients = self.clients.lock().await;
                if let Some(new) = clients
                    .values_mut()
                    .find(|c| c.tracked.account_id == tracked.account_id && c.tracked.id != id)
                {
                    new.restarts = restarts + 1;
                    new.failures = failures;
                    new.last_afk = last_afk;
                    new.game_name = previous_name;
                }
                clients.remove(id);
                drop(clients);
                let _ = state
                    .vault
                    .with(|d| {
                        d.tracked.retain(|t| t.id != id);
                        Ok(())
                    })
                    .await;
                state.roblox.notify("AltDeck restarted a client.").await;
            }
            Err(error) => {
                state.diagnostic("rejoin_retry", &error.to_string()).await;
                if let Some(client) = self.clients.lock().await.get_mut(id) {
                    client.state = "waiting".into();
                    client.next_retry = chrono::Utc::now().timestamp() + 60;
                }
            }
        }
    }
    fn observe_log(&self, client: &mut Runtime, _data_dir: &PathBuf, now: i64) {
        #[cfg(windows)]
        {
            use std::io::{Read, Seek, SeekFrom};
            let Some(local) = std::env::var_os("LOCALAPPDATA") else {
                return;
            };
            let root = PathBuf::from(local).join("Roblox").join("logs");
            if client.log.is_none() {
                let marker = format!("websiteBTId is {}", client.tracked.tracker_id);
                let mut files: Vec<_> = std::fs::read_dir(root)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| {
                        e.file_name().to_string_lossy().contains("_Player_")
                            && e.metadata()
                                .map(|m| m.len() < 32 * 1024 * 1024)
                                .unwrap_or(false)
                    })
                    .filter_map(|e| {
                        e.metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(|t| (t, e.path()))
                    })
                    .filter(|(time, _)| {
                        time.elapsed()
                            .map(|age| age.as_secs() < 300)
                            .unwrap_or(false)
                    })
                    .collect();
                files.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
                for (_, path) in files.into_iter().take(40) {
                    let Ok(mut file) = std::fs::File::open(&path) else {
                        continue;
                    };
                    let mut bytes = vec![0u8; 256 * 1024];
                    let count = file.read(&mut bytes).unwrap_or(0);
                    let text = String::from_utf8_lossy(&bytes[..count]);
                    if text.contains(&marker) {
                        client.log = Some(path);
                        client.log_offset = 0;
                        break;
                    }
                }
            }
            if let Some(path) = &client.log {
                let Ok(mut file) = std::fs::File::open(path) else {
                    return;
                };
                let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                if size <= client.log_offset {
                    return;
                }
                let start = if size - client.log_offset > 256 * 1024 {
                    size - 256 * 1024
                } else {
                    client.log_offset
                };
                if file.seek(SeekFrom::Start(start)).is_err() {
                    return;
                }
                let mut bytes = vec![0u8; (size - start) as usize];
                if file.read_exact(&mut bytes).is_err() {
                    return;
                }
                client.log_offset = size;
                let text = String::from_utf8_lossy(&bytes);
                // Only structural markers leave this function; raw logs and credentials never do.
                if text.contains("Replicator created") || text.contains("Joining game") {
                    client.state = "running".into();
                    client.disconnected_since = None;
                    client.last_activity = now;
                }
                if text.contains("Lost connection with reason")
                    || text.contains("Disconnected from server for reason")
                    || text.contains("Time to disconnect replication data")
                {
                    client.state = "disconnected".into();
                    client.reason = if text.contains("278") {
                        "idle".into()
                    } else {
                        "disconnect".into()
                    };
                    client.disconnected_since.get_or_insert(now);
                }
                if text.contains("[FLog::") {
                    client.last_activity = now;
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (client, now);
        }
    }
    pub async fn shutdown(&self) {
        for batch in self.batches.lock().await.iter() {
            batch.cancel.store(true, Ordering::SeqCst);
        }
        #[cfg(windows)]
        {
            if let Some(tx) = self.transaction.lock().await.take() {
                let _ = tx.restore();
            }
            self.shared_files.lock().map(|mut f| f.clear()).ok();
            self.singleton.lock().map(|mut s| s.take()).ok();
        }
    }
}
