use std::{
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Window, WindowEvent};

use crate::{
    feasibility::signature_webview::{
        SignatureRuntime, handle_exit_requested, handle_main_window_event,
    },
    music::download::MusicDownloadService,
};

const EXIT_BLOCKED_EVENT: &str = "app-exit-blocked";

#[derive(Default)]
pub struct ProductExitState {
    confirmed: AtomicBool,
    prompt_pending: AtomicBool,
    cleanup_active: AtomicBool,
    update_active: AtomicBool,
}

impl ProductExitState {
    fn is_confirmed(&self) -> bool {
        self.confirmed.load(Ordering::Acquire)
    }

    fn request_prompt(&self) -> bool {
        self.prompt_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn cancel(&self) {
        self.confirmed.store(false, Ordering::Release);
        self.prompt_pending.store(false, Ordering::Release);
        self.cleanup_active.store(false, Ordering::Release);
    }

    fn begin_cleanup(&self) {
        self.confirmed.store(false, Ordering::Release);
        self.prompt_pending.store(true, Ordering::Release);
        self.cleanup_active.store(true, Ordering::Release);
    }

    fn confirm(&self) {
        self.confirmed.store(true, Ordering::Release);
        self.prompt_pending.store(false, Ordering::Release);
        self.cleanup_active.store(false, Ordering::Release);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupReport {
    version: String,
    default_directory: String,
    log_directory: String,
}

fn emit_exit_prompt(app: &AppHandle) {
    let state = app.state::<Arc<ProductExitState>>();
    if state.request_prompt() {
        let _ = app.emit(EXIT_BLOCKED_EVENT, ());
    }
}

fn activity_blocks_exit(app: &AppHandle) -> bool {
    let state = app.state::<Arc<ProductExitState>>();
    if state.is_confirmed() {
        return false;
    }
    state.cleanup_active.load(Ordering::Acquire)
        || state.update_active.load(Ordering::Acquire)
        || app.state::<Arc<MusicDownloadService>>().has_active_batch()
}

pub fn handle_product_window_event(window: &Window, event: &WindowEvent) {
    if window.label() == "main"
        && matches!(event, WindowEvent::CloseRequested { .. })
        && activity_blocks_exit(window.app_handle())
    {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
        }
        emit_exit_prompt(window.app_handle());
        return;
    }
    handle_main_window_event(window, event);
}

pub fn handle_product_exit_requested(app: &AppHandle, api: &tauri::ExitRequestApi) {
    if activity_blocks_exit(app) {
        api.prevent_exit();
        emit_exit_prompt(app);
        return;
    }
    handle_exit_requested(app, api);
}

async fn stop_downloads(download: &MusicDownloadService) -> Result<(), String> {
    download.cancel_all_for_exit();
    for _ in 0..600 {
        if !download.has_active_batch() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("等待下载任务安全停止超时，请稍后重试".to_owned())
}

#[tauri::command]
pub async fn app_initialize(
    app: AppHandle,
    runtime: State<'_, Arc<SignatureRuntime>>,
) -> Result<StartupReport, String> {
    let log_directory = app
        .path()
        .app_log_dir()
        .map_err(|_| "INIT_LOG：无法获取日志目录".to_owned())?;
    tokio::fs::create_dir_all(&log_directory)
        .await
        .map_err(|_| "INIT_LOG：无法创建日志目录".to_owned())?;
    runtime
        .ensure_initialized()
        .await
        .map_err(|_| "INIT_SIGNATURE：签名运行环境初始化失败".to_owned())?;
    let default_directory = app
        .path()
        .audio_dir()
        .map_err(|_| "INIT_DOWNLOAD：无法获取系统音乐目录".to_owned())?;
    log::info!("应用初始化完成");
    Ok(StartupReport {
        version: app.package_info().version.to_string(),
        default_directory: default_directory.to_string_lossy().into_owned(),
        log_directory: log_directory.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn app_cancel_exit(state: State<'_, Arc<ProductExitState>>) {
    state.cancel();
}

#[tauri::command]
pub fn app_set_update_active(state: State<'_, Arc<ProductExitState>>, active: bool) {
    state.update_active.store(active, Ordering::Release);
}

#[tauri::command]
pub async fn app_confirm_exit(
    app: AppHandle,
    state: State<'_, Arc<ProductExitState>>,
    download: State<'_, Arc<MusicDownloadService>>,
) -> Result<(), String> {
    state.begin_cleanup();
    if let Err(error) = stop_downloads(download.inner()).await {
        state.cancel();
        return Err(error);
    }
    state.confirm();
    log::info!("下载任务已停止，继续退出");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn app_prepare_restart(
    state: State<'_, Arc<ProductExitState>>,
    download: State<'_, Arc<MusicDownloadService>>,
    runtime: State<'_, Arc<SignatureRuntime>>,
) -> Result<(), String> {
    state.begin_cleanup();
    if let Err(error) = stop_downloads(download.inner()).await {
        state.cancel();
        return Err(error);
    }
    if runtime.destroy().await.is_err() {
        state.cancel();
        return Err("UPDATE_INSTALL：无法安全关闭签名运行环境".to_owned());
    }
    state.confirm();
    log::info!("更新安装退出屏障已通过");
    Ok(())
}

fn open_path(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer.exe").arg(path).spawn()?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub async fn app_open_log_directory(app: AppHandle) -> Result<String, String> {
    let directory = app
        .path()
        .app_log_dir()
        .map_err(|_| "无法获取日志目录".to_owned())?;
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|_| "无法创建日志目录".to_owned())?;
    open_path(&directory).map_err(|_| "无法打开日志目录".to_owned())?;
    Ok(directory.to_string_lossy().into_owned())
}
