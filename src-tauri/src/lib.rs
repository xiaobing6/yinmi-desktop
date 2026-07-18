mod app_runtime;
pub mod signature;
pub mod music;

use app_runtime::{ProductExitState, handle_product_exit_requested, handle_product_window_event};
use signature::signature_webview::{
    SignatureExitCoordinator, SignatureRuntime, final_exit_cleanup,
};
use music::download::MusicDownloadService;
use music::rate_limiter::MusicRateLimiter;
use music::search::MusicSearchService;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("音觅".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .max_file_size(5 * 1024 * 1024)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());
    let builder = builder
        .setup(|app| {
            let runtime = Arc::new(SignatureRuntime::new(app.handle().clone()));
            let coordinator = Arc::new(SignatureExitCoordinator::new());
            let limiter = Arc::new(MusicRateLimiter::new(app.handle().clone()));
            let search = Arc::new(MusicSearchService::new(
                Arc::clone(&runtime),
                Arc::clone(&limiter),
            )?);
            let download = Arc::new(MusicDownloadService::new(
                Arc::clone(&runtime),
                Arc::clone(&limiter),
            )?);
            let exit_state = Arc::new(ProductExitState::default());
            app.manage(Arc::clone(&runtime));
            app.manage(Arc::clone(&coordinator));
            app.manage(limiter);
            app.manage(search);
            app.manage(download);
            app.manage(exit_state);
            Ok(())
        })
        .on_window_event(handle_product_window_event)
        .invoke_handler(tauri::generate_handler![
            music::search::music_search,
            music::search::music_get_search_snapshot,
            music::download::music_download_batch,
            music::download::music_get_download_snapshot,
            music::download::music_scan_existing,
            music::download::music_retry_failed,
            music::download::music_get_default_directory,
            music::download::music_cancel_current_download,
            music::download::music_cancel_all_downloads,
            music::download::music_open_download_directory,
            app_runtime::app_initialize,
            app_runtime::app_open_log_directory,
            app_runtime::app_cancel_exit,
            app_runtime::app_set_update_active,
            app_runtime::app_get_activity_status,
            app_runtime::app_confirm_exit,
            app_runtime::app_prepare_restart,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("failed to build yinmi");
    app.run(|app_handle, event| {
        match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                handle_product_exit_requested(app_handle, &api);
            }
            tauri::RunEvent::Exit => final_exit_cleanup(),
            _ => {}
        }
    });
}
