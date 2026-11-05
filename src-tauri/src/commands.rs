use crate::{
    error::{Error, Result},
    model::*,
    state::AppState,
};
use std::sync::{atomic::Ordering, Arc};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn vault_status(state: State<'_, Arc<AppState>>) -> Result<VaultStatus> {
    state.vault.status().await
}
#[tauri::command]
pub async fn vault_initialize(
    state: State<'_, Arc<AppState>>,
    password: Option<String>,
) -> Result<()> {
    state.vault.initialize(password.as_deref()).await?;
    state.manager.restore(&state).await;
    Ok(())
}
#[tauri::command]
pub async fn vault_unlock(state: State<'_, Arc<AppState>>, password: String) -> Result<()> {
    state.vault.initialize(Some(&password)).await?;
    state.manager.restore(&state).await;
    Ok(())
}
#[tauri::command]
pub async fn vault_lock(app: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> Result<()> {
    crate::login::cancel(&app, &state).await?;
    state.vault.lock().await;
    Ok(())
}
#[tauri::command]
pub async fn vault_touch(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.vault.touch().await;
    Ok(())
}
#[tauri::command]
pub async fn vault_set_password(
    state: State<'_, Arc<AppState>>,
    current: Option<String>,
    password: String,
    recovery: bool,
) -> Result<Option<String>> {
    state
        .vault
        .set_password(current.as_deref(), &password, recovery)
        .await
}
#[tauri::command]
pub async fn vault_remove_password(state: State<'_, Arc<AppState>>, current: String) -> Result<()> {
    state.vault.remove_password(&current).await
}
#[tauri::command]
pub async fn vault_recover(
    state: State<'_, Arc<AppState>>,
    code: String,
    password: String,
) -> Result<()> {
    state.vault.recover(&code, &password).await
}
#[tauri::command]
pub async fn vault_backup(state: State<'_, Arc<AppState>>, path: String) -> Result<()> {
    state.vault.read(|_| Ok(())).await?;
    let path = std::path::PathBuf::from(path);
    if path.extension().and_then(|s| s.to_str()) != Some("altdeck") {
        return Err(Error::Invalid("Use the .altdeck backup extension."));
    }
    state.vault.export(&path).await
}
#[tauri::command]
pub async fn snapshot(state: State<'_, Arc<AppState>>) -> Result<Snapshot> {
    let data = state.vault.snapshot_data().await?;
    Ok(Snapshot {
        accounts: data.accounts,
        settings: data.settings,
        favorites: data.favorites,
        history: data.history,
        clients: state.manager.clients().await,
        batches: state.manager.batches().await,
    })
}

pub async fn save_new_account(state: &AppState, cookie: Secret, alias: String) -> Result<Account> {
    let (user_id, username, display_name, cookie) = state.roblox.validate(&cookie).await?;
    if alias.len() > 80 {
        return Err(Error::Invalid("Aliases may contain at most 80 characters."));
    }
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        user_id,
        username: username.clone(),
        display_name,
        alias,
        note: String::new(),
        favorite: false,
        avatar_url: String::new(),
        robux: None,
        presence: 0,
        friends: None,
        followers: None,
        premium: None,
        groups: Vec::new(),
        session_status: "valid".into(),
        checked_at: Some(chrono::Utc::now().timestamp()),
        fps: None,
    };
    state
        .vault
        .with(|data| {
            if let Some(existing) = data.accounts.iter_mut().find(|a| a.user_id == user_id) {
                let id = existing.id.clone();
                data.secrets.retain(|s| s.id != id);
                data.secrets.push(PrivateAccount { id, cookie });
                existing.session_status = "valid".into();
                existing.checked_at = account.checked_at;
                return Ok(existing.clone());
            }
            data.secrets.push(PrivateAccount {
                id: account.id.clone(),
                cookie,
            });
            data.accounts.push(account.clone());
            Ok(account.clone())
        })
        .await
}
#[tauri::command]
pub async fn account_import(
    state: State<'_, Arc<AppState>>,
    cookie: String,
    alias: String,
) -> Result<Account> {
    let result = save_new_account(&state, Secret::new(cookie), alias).await;
    if result.is_err() {
        state
            .diagnostic(
                "account_import",
                "Validation failed. No credential was stored.",
            )
            .await;
    }
    result
}
#[tauri::command]
pub async fn login_begin(app: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> Result<String> {
    state.vault.read(|_| Ok(())).await?;
    crate::login::begin(&app, &state).await
}
#[tauri::command]
pub async fn login_finish(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    label: String,
) -> Result<Account> {
    crate::login::finish(app, &state, &label).await
}
#[tauri::command]
pub async fn login_cancel(app: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> Result<()> {
    crate::login::cancel(&app, &state).await
}
#[tauri::command]
pub async fn account_refresh(state: State<'_, Arc<AppState>>, id: String) -> Result<Account> {
    match state.roblox.refresh(&id).await {
        Ok(account) => Ok(account),
        Err(error) => {
            if matches!(error, Error::SessionExpired | Error::BrowserVerification) {
                let message = error.to_string();
                let _ = state
                    .vault
                    .with(|d| {
                        if let Some(a) = d.accounts.iter_mut().find(|a| a.id == id) {
                            a.session_status = "expired".into();
                        }
                        Ok(())
                    })
                    .await;
                state.diagnostic("session_expired", &message).await;
            }
            Err(error)
        }
    }
}
#[tauri::command]
pub async fn account_update(
    state: State<'_, Arc<AppState>>,
    id: String,
    alias: String,
    note: String,
    favorite: bool,
    fps: Option<u32>,
) -> Result<()> {
    if alias.len() > 80 || note.len() > 4000 || fps.is_some_and(|fps| !(10..=240).contains(&fps)) {
        return Err(Error::Invalid("Invalid account metadata."));
    }
    state
        .vault
        .with(|d| {
            let account = d
                .accounts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or(Error::NotFound)?;
            account.alias = alias;
            account.note = note;
            account.favorite = favorite;
            account.fps = fps;
            Ok(())
        })
        .await
}
#[tauri::command]
pub async fn account_remove(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<()> {
    if let Some(client) = state
        .manager
        .clients()
        .await
        .iter()
        .find(|c| c.account_id == id && c.state != "stopped" && c.state != "failed")
    {
        state.manager.action(&client.id, "close", true).await?;
    }
    let _ = app;
    state
        .vault
        .with(|d| {
            d.accounts.retain(|a| a.id != id);
            d.secrets.retain(|a| a.id != id);
            d.tracked.retain(|c| c.account_id != id);
            Ok(())
        })
        .await
}
#[tauri::command]
pub async fn account_reveal(
    state: State<'_, Arc<AppState>>,
    id: String,
    password: Option<String>,
) -> Result<String> {
    state.vault.reveal(&id, password.as_deref()).await
}
#[tauri::command]
pub async fn account_copy(
    state: State<'_, Arc<AppState>>,
    id: String,
    password: Option<String>,
) -> Result<()> {
    let cookie = Secret::new(state.vault.reveal(&id, password.as_deref()).await?);
    let mut clipboard = arboard::Clipboard::new().map_err(|_| Error::Internal)?;
    clipboard
        .set_text(cookie.expose())
        .map_err(|_| Error::Internal)?;
    let generation = state.clipboard_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        if state.clipboard_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            // Do not erase a newer value copied by the user.
            if clipboard.get_text().ok().as_deref() == Some(cookie.expose()) {
                let _ = clipboard.set_text("");
            }
        }
    });
    Ok(())
}
#[tauri::command]
pub async fn resolve_target(state: State<'_, Arc<AppState>>, input: String) -> Result<Target> {
    state.vault.read(|_| Ok(())).await?;
    state.roblox.resolve_target(&input).await
}
#[tauri::command]
pub async fn game_info(state: State<'_, Arc<AppState>>, place_id: u64) -> Result<GameInfo> {
    state.vault.read(|_| Ok(())).await?;
    state.roblox.game(place_id).await
}
#[tauri::command]
pub async fn server_list(
    state: State<'_, Arc<AppState>>,
    account_id: Option<String>,
    place_id: u64,
    private: bool,
    cursor: Option<String>,
    sort: Option<String>,
) -> Result<ServerPage> {
    let _ = state
        .vault
        .read(|d| {
            Ok(
                if account_id
                    .as_ref()
                    .is_some_and(|id| !d.accounts.iter().any(|a| a.id == *id))
                {
                    Err(Error::NotFound)
                } else {
                    Ok(())
                },
            )
        })
        .await?;
    state
        .roblox
        .servers(
            account_id.as_deref(),
            place_id,
            private,
            &cursor.unwrap_or_default(),
            &sort.unwrap_or("desc".into()),
        )
        .await
}
#[tauri::command]
pub async fn launch_batch(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    options: LaunchOptions,
) -> Result<String> {
    state
        .manager
        .start_batch(app, state.inner().clone(), options)
        .await
}
#[tauri::command]
pub async fn launch_cancel(state: State<'_, Arc<AppState>>, id: String) -> Result<()> {
    state.manager.cancel(&id).await
}
#[tauri::command]
pub async fn client_action(
    state: State<'_, Arc<AppState>>,
    id: String,
    action: String,
) -> Result<()> {
    state.vault.read(|_| Ok(())).await?;
    if ![
        "focus",
        "minimize",
        "close",
        "force-close",
        "small",
        "trim",
        "cpu",
    ]
    .contains(&action.as_str())
    {
        return Err(Error::Invalid("Unknown client action."));
    }
    state.manager.action(&id, &action, action == "close").await
}
#[tauri::command]
pub async fn client_preview(state: State<'_, Arc<AppState>>, id: String) -> Result<String> {
    state.vault.read(|_| Ok(())).await?;
    state.manager.preview(&id).await
}
#[tauri::command]
pub async fn stop_all(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.manager.stop_all().await
}
#[tauri::command]
pub async fn tile_clients(state: State<'_, Arc<AppState>>) -> Result<()> {
    let columns = state.vault.read(|d| Ok(d.settings.tile_columns)).await?;
    state.manager.tile(columns).await
}
#[tauri::command]
pub async fn settings_save(state: State<'_, Arc<AppState>>, mut settings: Settings) -> Result<()> {
    settings.validate();
    if settings.background.len() > 4_000_000
        || (!settings.background.is_empty()
            && !settings.background.starts_with("data:image/png;base64,")
            && !settings.background.starts_with("data:image/jpeg;base64,"))
    {
        return Err(Error::Invalid(
            "Use a PNG or JPEG background smaller than 3 MB.",
        ));
    }
    state
        .vault
        .with(|d| {
            d.settings = settings;
            Ok(())
        })
        .await
}
#[tauri::command]
pub async fn performance_apply(
    state: State<'_, Arc<AppState>>,
    mut profile: Performance,
) -> Result<()> {
    profile.validate();
    if state.manager.active_count().await > 0 {
        return Err(Error::Invalid(
            "Stop clients before changing the global Roblox FPS cap.",
        ));
    }
    #[cfg(windows)]
    {
        let exe = crate::platform::win::roblox_path()?;
        let _ = exe;
        // Rendering flags are applied transactionally at the next launch.
        crate::performance::frame_cap(profile.fps)?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = (state, profile);
        Err(Error::WindowsRequired)
    }
}
#[tauri::command]
pub async fn favorite_save(
    state: State<'_, Arc<AppState>>,
    name: String,
    input: String,
) -> Result<Favorite> {
    let target = state.roblox.resolve_target(&input).await?;
    if name.trim().is_empty() || name.len() > 80 {
        return Err(Error::Invalid(
            "Use a favorite name between 1 and 80 characters.",
        ));
    }
    let mut favorite = Favorite {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        target: input,
        icon: String::new(),
    };
    if let Some(place_id) = target.place_id() {
        favorite.icon = state
            .roblox
            .game(place_id)
            .await
            .map(|g| g.icon)
            .unwrap_or_default();
    }
    state
        .vault
        .with(|d| {
            if d.favorites.len() >= 100 {
                return Err(Error::Invalid("At most 100 favorites are supported."));
            }
            d.favorites.push(favorite.clone());
            Ok(favorite)
        })
        .await
}
#[tauri::command]
pub async fn favorite_remove(state: State<'_, Arc<AppState>>, id: String) -> Result<()> {
    state
        .vault
        .with(|d| {
            d.favorites.retain(|f| f.id != id);
            Ok(())
        })
        .await
}
#[tauri::command]
pub async fn webhook_save(
    state: State<'_, Arc<AppState>>,
    url: String,
    enabled: bool,
) -> Result<()> {
    if enabled || !url.is_empty() {
        crate::roblox::validate_webhook(&url)?;
    }
    state
        .vault
        .with(|d| {
            d.webhook = Secret::new(url);
            d.settings.webhook_enabled = enabled;
            Ok(())
        })
        .await
}
#[tauri::command]
pub async fn webhook_test(state: State<'_, Arc<AppState>>) -> Result<()> {
    state.roblox.webhook_test().await
}
#[tauri::command]
pub async fn diagnostics(state: State<'_, Arc<AppState>>) -> Result<Vec<DiagnosticEvent>> {
    Ok(state.diagnostics.lock().await.iter().cloned().collect())
}
#[tauri::command]
pub async fn roblox_status() -> Result<serde_json::Value> {
    #[cfg(windows)]
    let installed = crate::platform::win::roblox_path()
        .ok()
        .map(|p| p.display().to_string());
    #[cfg(not(windows))]
    let installed: Option<String> = None;
    Ok(
        serde_json::json!({"installed": installed.is_some(), "path": installed, "platformSupported": cfg!(windows)}),
    )
}
#[tauri::command]
pub async fn open_roblox_download(app: tauri::AppHandle) -> Result<()> {
    // Open the official product page rather than executing an unsigned downloaded installer.
    app.opener()
        .open_url("https://www.roblox.com/download", None::<String>)
        .map_err(|_| Error::Internal)
}

pub fn handlers() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool {
    tauri::generate_handler![
        vault_status,
        vault_initialize,
        vault_unlock,
        vault_lock,
        vault_touch,
        vault_set_password,
        vault_remove_password,
        vault_recover,
        vault_backup,
        snapshot,
        account_import,
        login_begin,
        login_finish,
        login_cancel,
        account_refresh,
        account_update,
        account_remove,
        account_reveal,
        account_copy,
        resolve_target,
        game_info,
        server_list,
        launch_batch,
        launch_cancel,
        client_action,
        client_preview,
        stop_all,
        tile_clients,
        settings_save,
        performance_apply,
        favorite_save,
        favorite_remove,
        webhook_save,
        webhook_test,
        diagnostics,
        roblox_status,
        open_roblox_download
    ]
}
