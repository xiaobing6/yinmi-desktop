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
const STARTUP_PROGRESS_EVENT: &str = "app-startup-progress";
const UPDATE_SAFE_WAIT_TIMEOUT: Duration = Duration::from_secs(20 * 60);
#[cfg(target_os = "windows")]
const MIN_WEBVIEW2_VERSION: [u64; 4] = [111, 0, 1661, 0];

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
    webview_version: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupProgress {
    id: &'static str,
    label: &'static str,
    state: &'static str,
    detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActivityStatus {
    music_download_active: bool,
    update_download_active: bool,
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

fn emit_startup_progress(
    app: &AppHandle,
    id: &'static str,
    label: &'static str,
    state: &'static str,
    detail: Option<String>,
) {
    let _ = app.emit(
        STARTUP_PROGRESS_EVENT,
        StartupProgress {
            id,
            label,
            state,
            detail,
        },
    );
}

#[cfg(target_os = "windows")]
fn check_webview_runtime() -> Result<Option<String>, String> {
    let version = wry::webview_version().map_err(|_| {
        "INIT_WEBVIEW2：未检测到 WebView2 Runtime，请安装最新版：https://go.microsoft.com/fwlink/p/?LinkId=2124703"
            .to_owned()
    })?;
    if !version_at_least(&version, MIN_WEBVIEW2_VERSION) {
        return Err(format!(
            "INIT_WEBVIEW2：当前 WebView2 {version}，最低需要 111.0.1661.0。请升级：https://go.microsoft.com/fwlink/p/?LinkId=2124703"
        ));
    }
    Ok(Some(version))
}

#[cfg(target_os = "macos")]
fn check_webview_runtime() -> Result<Option<String>, String> {
    Ok(wry::webview_version().ok())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn check_webview_runtime() -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(target_os = "windows")]
fn version_at_least(value: &str, minimum: [u64; 4]) -> bool {
    let mut parsed = [0_u64; 4];
    for (index, part) in value.split('.').take(4).enumerate() {
        let digits = part
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        let Ok(number) = digits.parse() else {
            return false;
        };
        parsed[index] = number;
    }
    parsed >= minimum
}

async fn wait_for_update_download(state: &ProductExitState) -> Result<(), String> {
    let started = tokio::time::Instant::now();
    while state.update_active.load(Ordering::Acquire) {
        if started.elapsed() >= UPDATE_SAFE_WAIT_TIMEOUT {
            return Err("UPDATE_INSTALL：等待更新包进入安全状态超时，请稍后重试".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn app_initialize(
    app: AppHandle,
    runtime: State<'_, Arc<SignatureRuntime>>,
) -> Result<StartupReport, String> {
    emit_startup_progress(&app, "log", "运行日志", "working", None);
    let log_directory = app
        .path()
        .app_log_dir()
        .map_err(|_| "INIT_LOG：无法获取日志目录".to_owned())?;
    tokio::fs::create_dir_all(&log_directory)
        .await
        .map_err(|_| "INIT_LOG：无法创建日志目录".to_owned())?;
    emit_startup_progress(
        &app,
        "log",
        "运行日志",
        "done",
        Some(log_directory.to_string_lossy().into_owned()),
    );
    emit_startup_progress(&app, "webview", "系统 WebView", "working", None);
    let webview_version = check_webview_runtime()?;
    emit_startup_progress(
        &app,
        "webview",
        "系统 WebView",
        "done",
        webview_version.clone(),
    );
    emit_startup_progress(&app, "signature", "音乐签名环境", "working", None);
    runtime
        .ensure_initialized()
        .await
        .map_err(|_| "INIT_SIGNATURE：签名运行环境初始化失败".to_owned())?;
    emit_startup_progress(&app, "signature", "音乐签名环境", "done", None);
    emit_startup_progress(&app, "source", "固定音源", "done", Some("10 个".to_owned()));
    emit_startup_progress(&app, "download", "下载引擎", "working", None);
    let default_directory = app
        .path()
        .audio_dir()
        .map_err(|_| "INIT_DOWNLOAD：无法获取系统音乐目录".to_owned())?;
    emit_startup_progress(
        &app,
        "download",
        "下载引擎",
        "done",
        Some(default_directory.to_string_lossy().into_owned()),
    );
    emit_startup_progress(
        &app,
        "update",
        "应用更新",
        "done",
        Some("进入主界面后后台检查".to_owned()),
    );
    log::info!("应用初始化完成");
    Ok(StartupReport {
        version: app.package_info().version.to_string(),
        default_directory: default_directory.to_string_lossy().into_owned(),
        log_directory: log_directory.to_string_lossy().into_owned(),
        webview_version,
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
pub fn app_get_activity_status(
    state: State<'_, Arc<ProductExitState>>,
    download: State<'_, Arc<MusicDownloadService>>,
) -> AppActivityStatus {
    AppActivityStatus {
        music_download_active: download.has_active_batch(),
        update_download_active: state.update_active.load(Ordering::Acquire),
    }
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
    if let Err(error) = wait_for_update_download(state.inner()).await {
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
    cancel_music_downloads: bool,
) -> Result<(), String> {
    if state.update_active.load(Ordering::Acquire) {
        return Err("UPDATE_INSTALL：更新包仍在下载，请等待下载完成".to_owned());
    }
    if download.has_active_batch() && !cancel_music_downloads {
        return Err("UPDATE_INSTALL：仍有歌曲正在下载，需要确认取消后再安装".to_owned());
    }
    state.begin_cleanup();
    if cancel_music_downloads && let Err(error) = stop_downloads(download.inner()).await {
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

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::version_at_least;

    #[test]
    fn compares_webview2_versions_numerically() {
        let minimum = [111, 0, 1661, 0];
        assert!(version_at_least("111.0.1661.0", minimum));
        assert!(version_at_least("141.0.3537.85", minimum));
        assert!(!version_at_least("110.0.9999.99", minimum));
        assert!(!version_at_least("not-a-version", minimum));
    }
}
