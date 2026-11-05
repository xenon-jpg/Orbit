use crate::{
    error::{Error, Result},
    state::AppState,
};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
#[cfg(windows)]
use tauri::{WebviewUrl, WebviewWindowBuilder};

pub struct PendingLogin {
    pub label: String,
    pub directory: PathBuf,
    pub started: u64,
}

pub async fn begin(app: &tauri::AppHandle, state: &AppState) -> Result<String> {
    #[cfg(not(windows))]
    {
        let _ = (app, state);
        return Err(Error::WindowsRequired);
    }
    #[cfg(windows)]
    {
        let mut pending = state.login.lock().await;
        if let Some(previous) = pending.as_ref() {
            if let Some(window) = app.get_webview_window(&previous.label) {
                let _ = window.set_focus();
            }
            return Ok(previous.label.clone());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let label = format!("login-{id}");
        let directory = state.data_dir.join("browser").join(&id);
        std::fs::create_dir_all(&directory)?;
        let url = url::Url::parse("https://www.roblox.com/login").map_err(|_| Error::Internal)?;
        let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
            .title("Sign in to Roblox — AltDeck")
            .inner_size(1000., 760.)
            .center()
            .resizable(true)
            .data_directory(directory.clone())
            .incognito(false)
            .devtools(false)
            .browser_extensions_enabled(false)
            .on_navigation(|url| {
                url.scheme() == "https"
                    && url
                        .host_str()
                        .is_some_and(|h| h == "roblox.com" || h.ends_with(".roblox.com"))
            })
            .build()
            .map_err(|_| Error::Internal)?;
        let _ = window.set_focus();
        *pending = Some(PendingLogin {
            label: label.clone(),
            directory,
            started: now(),
        });
        Ok(label)
    }
}
pub async fn finish(
    app: tauri::AppHandle,
    state: &AppState,
    label: &str,
) -> Result<crate::model::Account> {
    #[cfg(not(windows))]
    {
        let _ = (app, state, label);
        Err(Error::WindowsRequired)
    }
    #[cfg(windows)]
    {
        let mut pending_guard = state.login.lock().await;
        let pending = pending_guard
            .as_ref()
            .filter(|p| p.label == label)
            .map(|p| (p.label.clone(), p.directory.clone()))
            .ok_or(Error::NotFound)?;
        let window = app.get_webview_window(&pending.0).ok_or(Error::NotFound)?;
        let cookie_window = window.clone();
        let cookies = tokio::task::spawn_blocking(move || {
            cookie_window.cookies_for_url(url::Url::parse("https://www.roblox.com/").unwrap())
        })
        .await
        .map_err(|_| Error::Internal)?
        .map_err(|_| Error::NotFound)?;
        let cookie = cookies
            .iter()
            .find(|c| c.name() == ".ROBLOSECURITY")
            .map(|c| c.value().to_owned())
            .ok_or(Error::SessionExpired)?;
        let account = crate::commands::save_new_account(
            &state,
            crate::model::Secret::new(cookie),
            String::new(),
        )
        .await?;
        let _ = window.destroy();
        cleanup(pending.1);
        *pending_guard = None;
        Ok(account)
    }
}
pub async fn cancel(app: &tauri::AppHandle, state: &AppState) -> Result<()> {
    let pending = state.login.lock().await.take();
    if let Some(pending) = pending {
        if let Some(window) = app.get_webview_window(&pending.label) {
            let _ = window.destroy();
        }
        cleanup(pending.directory);
    }
    Ok(())
}
pub fn cleanup(directory: PathBuf) {
    // Only directories below our own browser directory are removable.
    if directory
        .parent()
        .is_some_and(|p| p.file_name().is_some_and(|n| n == "browser"))
    {
        tauri::async_runtime::spawn(async move {
            for _ in 0..10 {
                if !directory.exists() || std::fs::remove_dir_all(&directory).is_ok() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    }
}
pub fn cleanup_stale(root: PathBuf) {
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                cleanup(entry.path());
            }
        }
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
