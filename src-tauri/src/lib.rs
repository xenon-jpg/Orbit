pub mod commands;
pub mod error;
pub mod login;
pub mod manager;
pub mod model;
pub mod performance;
pub mod platform;
pub mod roblox;
pub mod state;
pub mod target;
pub mod vault;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            // A stale journal is safe to undo only when the file still exactly matches our write.
            if let Err(error) = performance::recover(&directory) {
                eprintln!("Performance recovery requires attention: {error}");
            }
            login::cleanup_stale(directory.join("browser"));
            let state = Arc::new(state::AppState::new(directory)?);
            app.manage(state.clone());
            let handle = app.handle().clone();
            let background_state = state.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
                loop {
                    interval.tick().await;
                    background_state
                        .manager
                        .tick(&handle, &background_state)
                        .await;
                }
            });
            Ok(())
        })
        .invoke_handler(commands::handlers())
        .build(tauri::generate_context!())
        .expect("AltDeck could not initialize")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<Arc<state::AppState>>() {
                    let manager = state.manager.clone();
                    tauri::async_runtime::block_on(async move {
                        manager.shutdown().await;
                    });
                }
            }
        });
}
