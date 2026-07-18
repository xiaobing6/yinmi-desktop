use std::{
    collections::HashSet,
    ffi::OsStr,
    io::{self, Cursor},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use image::{ImageFormat, ImageReader};
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    tag::Tag,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::feasibility::signature_webview::SignatureRuntime;
use crate::music::{
    contract::{
        AudioAvailability, EncodedComponent, GdOperation, GdSource, ProbeSong,
        parse_audio_response, parse_lyric_response, parse_picture_response, render_form_body,
    },
    network_policy::{self, MediaGetError},
    rate_limiter::MusicRateLimiter,
    search::{MusicCommandError, MusicSearchService, SearchResult},
    storage_space,
};

const GD_API_URL: &str = "https://music.gdstudio.xyz/api.php";
const MAX_API_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const MAX_COVER_BYTES: usize = 20 * 1024 * 1024;
const MAX_COVER_DIMENSION: u32 = 4096;
const MAX_AUDIO_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MIN_FREE_SPACE_BYTES: u64 = 512 * 1024 * 1024;
const PROGRESS_EVENT: &str = "music-download-progress";
const COMPLETE_EVENT: &str = "music-download-complete";
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "m4a", "aac", "ogg", "wav"];

#[cfg(target_os = "windows")]
#[link(name = "Kernel32")]
unsafe extern "system" {
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn renameatx_np(
        from_dir_fd: i32,
        from: *const std::os::raw::c_char,
        to_dir_fd: i32,
        to: *const std::os::raw::c_char,
        flags: u32,
    ) -> i32;
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadBatchRequest {
    pub keyword: String,
    pub source: GdSource,
    pub songs: Vec<ProbeSong>,
    pub bitrate: u16,
    pub embed_cover: bool,
    pub download_lyrics: bool,
    pub base_directory: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadStartRequest {
    pub search_request_id: u64,
    pub song_ids: Vec<String>,
    pub bitrate: u16,
    pub embed_cover: bool,
    pub download_lyrics: bool,
    pub base_directory: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub batch_id: u64,
    pub completed: usize,
    pub total: usize,
    pub current_song_id: String,
    pub current_name: String,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub state: DownloadProgressState,
    pub current_downloaded_bytes: u64,
    pub current_total_bytes: Option<u64>,
    pub bytes_per_second: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadProgressState {
    Preparing,
    Downloading,
    Finished,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadItemState {
    Success,
    Skipped,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItemResult {
    pub song_id: String,
    pub name: String,
    pub state: DownloadItemState,
    pub path: Option<String>,
    pub bytes: u64,
    pub code: Option<&'static str>,
    pub message: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadBatchResult {
    pub batch_id: u64,
    pub directory: String,
    pub total: usize,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub items: Vec<DownloadItemResult>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStateSnapshot {
    pub active: bool,
    pub progress: Option<DownloadProgress>,
    pub active_items: Vec<DownloadItemResult>,
    pub last_result: Option<DownloadBatchResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExistingAudioScanRequest {
    pub search_request_id: u64,
    pub base_directory: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingAudioEntry {
    pub song_id: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingAudioScan {
    pub search_request_id: u64,
    pub directory: String,
    pub items: Vec<ExistingAudioEntry>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelScope {
    Current,
    All,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelDownloadResult {
    pub accepted: bool,
    pub batch_id: Option<u64>,
    pub scope: CancelScope,
}

enum DownloadedFile {
    Success {
        path: PathBuf,
        bytes: u64,
        warnings: Vec<String>,
    },
    Skipped {
        path: PathBuf,
        warnings: Vec<String>,
    },
}

enum LyricWriteResult {
    Written,
    AlreadyExists,
}

enum NoReplaceCommit {
    Committed,
    AlreadyExists,
}

enum CoverFailure {
    Cancelled,
    Warning(MusicCommandError),
    Fatal(MusicCommandError),
}

enum DownloadFailure {
    Cancelled,
    Failed(MusicCommandError),
}

impl From<MusicCommandError> for DownloadFailure {
    fn from(error: MusicCommandError) -> Self {
        Self::Failed(error)
    }
}

impl From<MediaGetError> for DownloadFailure {
    fn from(error: MediaGetError) -> Self {
        match error {
            MediaGetError::Cancelled => Self::Cancelled,
            MediaGetError::Network => Self::Failed(media_network_error()),
        }
    }
}

impl From<DownloadFailure> for CoverFailure {
    fn from(error: DownloadFailure) -> Self {
        match error {
            DownloadFailure::Cancelled => Self::Cancelled,
            DownloadFailure::Failed(error) => Self::Warning(error),
        }
    }
}

impl From<MusicCommandError> for CoverFailure {
    fn from(error: MusicCommandError) -> Self {
        Self::Warning(error)
    }
}

impl From<MediaGetError> for CoverFailure {
    fn from(error: MediaGetError) -> Self {
        match error {
            MediaGetError::Cancelled => Self::Cancelled,
            MediaGetError::Network => Self::Warning(media_network_error()),
        }
    }
}

fn media_network_error() -> MusicCommandError {
    MusicCommandError::new("DOWNLOAD_NETWORK", "下载资源连接失败")
}

struct ActiveBatchControl {
    batch_id: u64,
    cancel_current: CancellationToken,
    cancel_all: bool,
    progress: Option<DownloadProgress>,
    items: Vec<DownloadItemResult>,
}

#[derive(Clone)]
struct RetrySnapshot {
    request: DownloadBatchRequest,
    retryable_ids: HashSet<String>,
}

#[derive(Clone, Copy)]
struct BatchCounters {
    succeeded: usize,
    skipped: usize,
    failed: usize,
    cancelled: usize,
}

struct SongProgress<'a> {
    app: &'a AppHandle,
    service: &'a MusicDownloadService,
    batch_id: u64,
    completed: usize,
    total: usize,
    current_song_id: &'a str,
    current_name: &'a str,
    counters: BatchCounters,
}

struct DownloadSongRequest<'a> {
    directory: &'a Path,
    source: GdSource,
    bitrate: u16,
    embed_cover: bool,
    download_lyrics: bool,
    song: &'a ProbeSong,
}

impl SongProgress<'_> {
    fn emit(
        &self,
        state: DownloadProgressState,
        current_downloaded_bytes: u64,
        current_total_bytes: Option<u64>,
        bytes_per_second: u64,
    ) {
        let snapshot = DownloadProgress {
            batch_id: self.batch_id,
            completed: self.completed,
            total: self.total,
            current_song_id: self.current_song_id.to_owned(),
            current_name: self.current_name.to_owned(),
            succeeded: self.counters.succeeded,
            skipped: self.counters.skipped,
            failed: self.counters.failed,
            cancelled: self.counters.cancelled,
            state,
            current_downloaded_bytes,
            current_total_bytes,
            bytes_per_second,
        };
        self.service.store_progress(snapshot.clone());
        let _ = self.app.emit(PROGRESS_EVENT, snapshot);
    }
}

pub struct MusicDownloadService {
    runtime: Arc<SignatureRuntime>,
    limiter: Arc<MusicRateLimiter>,
    api_client: reqwest::Client,
    queue: tokio::sync::Mutex<()>,
    next_batch_id: AtomicU64,
    next_temp_id: AtomicU64,
    last_directory: Mutex<Option<PathBuf>>,
    active_batch: Mutex<Option<ActiveBatchControl>>,
    last_retry: Mutex<Option<RetrySnapshot>>,
    last_result: Mutex<Option<DownloadBatchResult>>,
    dedupe_scan: Mutex<Option<ExistingAudioScan>>,
}

impl MusicDownloadService {
    pub(crate) fn new(
        runtime: Arc<SignatureRuntime>,
        limiter: Arc<MusicRateLimiter>,
    ) -> Result<Self, MusicCommandError> {
        let api_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .tls_backend_rustls()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| MusicCommandError::new("MUSIC_HTTP", "无法连接音乐服务"))?;
        Ok(Self {
            runtime,
            limiter,
            api_client,
            queue: tokio::sync::Mutex::new(()),
            next_batch_id: AtomicU64::new(0),
            next_temp_id: AtomicU64::new(0),
            last_directory: Mutex::new(None),
            active_batch: Mutex::new(None),
            last_retry: Mutex::new(None),
            last_result: Mutex::new(None),
            dedupe_scan: Mutex::new(None),
        })
    }

    async fn download_batch(
        &self,
        app: &AppHandle,
        request: DownloadBatchRequest,
    ) -> Result<DownloadBatchResult, MusicCommandError> {
        let _queue_guard = self.queue.lock().await;
        let keyword = request.keyword.trim();
        if keyword.is_empty() || request.songs.is_empty() || request.songs.len() > 1_000 {
            return Err(MusicCommandError::new(
                "INPUT_INVALID",
                "请选择 1 到 1000 首歌曲后再下载",
            ));
        }
        if !matches!(request.bitrate, 128 | 192 | 320 | 740 | 999) {
            return Err(MusicCommandError::new(
                "INPUT_INVALID",
                "音质仅支持 128、192、320、740 或 999 kbps",
            ));
        }

        let base = resolve_base_directory(app, &request.base_directory).await?;
        let directory = base.join(sanitize_segment(keyword, 120, "音乐下载"));
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|_| MusicCommandError::new("FS_PATH", "无法创建下载目录"))?;
        match storage_space::available_bytes(&directory).await {
            Ok(Some(available)) if available < MIN_FREE_SPACE_BYTES => {
                return Err(MusicCommandError::new(
                    "FS_SPACE",
                    "下载目录剩余空间不足 512 MiB",
                ));
            }
            Err(_) => log::warn!("无法读取下载目录剩余空间，继续使用现有流式大小上限"),
            _ => {}
        }
        cleanup_stale_temp_files(&directory).await;
        *self
            .last_directory
            .lock()
            .map_err(|_| MusicCommandError::new("FS_PATH", "下载目录状态不可用"))? =
            Some(directory.clone());

        let batch_id = self.next_batch_id.fetch_add(1, Ordering::AcqRel) + 1;
        let request_snapshot = request.clone();
        let total = request.songs.len();
        let mut items = Vec::with_capacity(total);
        let (mut succeeded, mut skipped, mut failed, mut cancelled) = (0, 0, 0, 0);
        let first_cancel = CancellationToken::new();
        *self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(ActiveBatchControl {
            batch_id,
            cancel_current: first_cancel.clone(),
            cancel_all: false,
            progress: None,
            items: Vec::new(),
        });
        *self
            .last_result
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

        for (index, song) in request.songs.into_iter().enumerate() {
            log::info!(
                "下载项目开始 batch_id={} song_id={} position={}/{}",
                batch_id,
                song.id,
                index + 1,
                total
            );
            let cancel = if index == 0 {
                first_cancel.clone()
            } else {
                self.next_song_token(batch_id)
            };
            let counters = BatchCounters {
                succeeded,
                skipped,
                failed,
                cancelled,
            };
            let progress = SongProgress {
                app,
                service: self,
                batch_id,
                completed: index,
                total,
                current_song_id: &song.id,
                current_name: &song.name,
                counters,
            };
            progress.emit(DownloadProgressState::Preparing, 0, None, 0);

            let item = if self.cancel_all_requested(batch_id) {
                cancelled += 1;
                DownloadItemResult {
                    song_id: song.id,
                    name: song.name,
                    state: DownloadItemState::Cancelled,
                    path: None,
                    bytes: 0,
                    code: None,
                    message: Some("已取消全部下载".to_owned()),
                    warnings: Vec::new(),
                }
            } else {
                match self
                    .download_song(
                        DownloadSongRequest {
                            directory: &directory,
                            source: request.source,
                            bitrate: request.bitrate,
                            embed_cover: request.embed_cover,
                            download_lyrics: request.download_lyrics,
                            song: &song,
                        },
                        &cancel,
                        &progress,
                    )
                    .await
                {
                    Ok(DownloadedFile::Success {
                        path,
                        bytes,
                        warnings,
                    }) => {
                        succeeded += 1;
                        DownloadItemResult {
                            song_id: song.id,
                            name: song.name,
                            state: DownloadItemState::Success,
                            path: Some(path.to_string_lossy().into_owned()),
                            bytes,
                            code: None,
                            message: None,
                            warnings,
                        }
                    }
                    Ok(DownloadedFile::Skipped { path, warnings }) => {
                        skipped += 1;
                        DownloadItemResult {
                            song_id: song.id,
                            name: song.name,
                            state: DownloadItemState::Skipped,
                            path: Some(path.to_string_lossy().into_owned()),
                            bytes: 0,
                            code: None,
                            message: Some("文件已存在，未覆盖".to_owned()),
                            warnings,
                        }
                    }
                    Err(DownloadFailure::Cancelled) => {
                        cancelled += 1;
                        DownloadItemResult {
                            song_id: song.id,
                            name: song.name,
                            state: DownloadItemState::Cancelled,
                            path: None,
                            bytes: 0,
                            code: None,
                            message: Some("下载已取消".to_owned()),
                            warnings: Vec::new(),
                        }
                    }
                    Err(DownloadFailure::Failed(error)) => {
                        failed += 1;
                        DownloadItemResult {
                            song_id: song.id,
                            name: song.name,
                            state: DownloadItemState::Failed,
                            path: None,
                            bytes: 0,
                            code: Some(error.code),
                            message: Some(error.message),
                            warnings: Vec::new(),
                        }
                    }
                }
            };
            items.push(item);
            self.store_item_result(batch_id, items[index].clone());
            log::info!(
                "下载项目结束 batch_id={} song_id={} state={:?}",
                batch_id,
                items[index].song_id,
                items[index].state
            );
            SongProgress {
                app,
                service: self,
                batch_id,
                completed: index + 1,
                total,
                current_song_id: &items[index].song_id,
                current_name: &items[index].name,
                counters: BatchCounters {
                    succeeded,
                    skipped,
                    failed,
                    cancelled,
                },
            }
            .emit(
                DownloadProgressState::Finished,
                items[index].bytes,
                Some(items[index].bytes).filter(|bytes| *bytes > 0),
                0,
            );
        }

        let retryable_ids = items
            .iter()
            .filter(|item| {
                matches!(
                    item.state,
                    DownloadItemState::Failed | DownloadItemState::Cancelled
                )
            })
            .map(|item| item.song_id.clone())
            .collect();
        *self
            .last_retry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(RetrySnapshot {
            request: request_snapshot,
            retryable_ids,
        });

        let result = DownloadBatchResult {
            batch_id,
            directory: directory.to_string_lossy().into_owned(),
            total,
            succeeded,
            skipped,
            failed,
            cancelled,
            items,
        };
        *self
            .last_result
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result.clone());
        self.finish_batch(batch_id);
        let _ = app.emit(COMPLETE_EVENT, result.clone());
        Ok(result)
    }

    async fn retry_failed(
        &self,
        app: &AppHandle,
        song_id: Option<String>,
    ) -> Result<DownloadBatchResult, MusicCommandError> {
        let snapshot = self
            .last_retry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| MusicCommandError::new("MUSIC_NO_RETRY", "没有可重试的下载任务"))?;

        let requested_id = song_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        if let Some(id) = requested_id
            && !snapshot.retryable_ids.contains(id)
        {
            return Err(MusicCommandError::new(
                "MUSIC_NO_RETRY",
                "这首歌曲不在最近一次失败或取消的任务中",
            ));
        }

        let mut retry_request = snapshot.request.clone();
        retry_request.songs.retain(|song| {
            snapshot.retryable_ids.contains(&song.id) && requested_id.is_none_or(|id| song.id == id)
        });
        if retry_request.songs.is_empty() {
            return Err(MusicCommandError::new(
                "MUSIC_NO_RETRY",
                "没有可重试的下载任务",
            ));
        }
        let attempted_ids = retry_request
            .songs
            .iter()
            .map(|song| song.id.clone())
            .collect::<HashSet<_>>();
        let result = self.download_batch(app, retry_request).await?;

        let mut retryable_ids = snapshot.retryable_ids;
        retryable_ids.retain(|id| !attempted_ids.contains(id));
        retryable_ids.extend(
            result
                .items
                .iter()
                .filter(|item| {
                    matches!(
                        item.state,
                        DownloadItemState::Failed | DownloadItemState::Cancelled
                    )
                })
                .map(|item| item.song_id.clone()),
        );
        *self
            .last_retry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(RetrySnapshot {
            request: snapshot.request,
            retryable_ids,
        });
        Ok(result)
    }

    async fn download_song(
        &self,
        request: DownloadSongRequest<'_>,
        cancel: &CancellationToken,
        progress: &SongProgress<'_>,
    ) -> Result<DownloadedFile, DownloadFailure> {
        let DownloadSongRequest {
            directory,
            source,
            bitrate,
            embed_cover,
            download_lyrics,
            song,
        } = request;
        if cancel.is_cancelled() {
            return Err(DownloadFailure::Cancelled);
        }
        let file_stem = song_file_stem(song);
        let resource_id = song.url_id.as_deref().unwrap_or(&song.id);
        if resource_id.trim().is_empty() {
            return Err(MusicCommandError::new(
                "MUSIC_QUALITY_UNAVAILABLE",
                "这首歌曲没有可用的音频地址",
            )
            .into());
        }

        let operation = GdOperation::Url {
            id: EncodedComponent::encode(resource_id),
            source,
            bitrate,
        };
        let body = self
            .request_gd_resource(operation, cancel, "获取音频地址")
            .await?;
        let location = match parse_audio_response(&body, u32::from(bitrate))
            .map_err(|_| MusicCommandError::new("MUSIC_SCHEMA", "音频地址格式不兼容"))?
        {
            AudioAvailability::Available(location) => location,
            AudioAvailability::Unavailable(_) => {
                return Err(MusicCommandError::new(
                    "MUSIC_QUALITY_UNAVAILABLE",
                    format!("没有可用的 {bitrate} kbps 音频"),
                )
                .into());
            }
        };

        let reported_size = location.size_bytes;
        let response = network_policy::safe_media_get(location.url, cancel).await?;
        if !response.status().is_success() {
            return Err(media_network_error().into());
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_AUDIO_BYTES)
        {
            return Err(MusicCommandError::new("DOWNLOAD_MEDIA", "音频文件超过 2 GiB 上限").into());
        }

        let total_bytes = response.content_length().or(reported_size);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let url_extension = extension_from_path(response.url().path()).map(str::to_owned);
        if cancel.is_cancelled() {
            return Err(DownloadFailure::Cancelled);
        }
        let (temp_path, mut file) = self.create_temp_file(directory).await?;
        let mut first_bytes = Vec::with_capacity(16);
        let mut written = 0_u64;
        let mut stream = response.bytes_stream();
        let started = Instant::now();
        let mut last_progress = Instant::now();
        progress.emit(DownloadProgressState::Downloading, 0, total_bytes, 0);
        let transfer_result: Result<(), DownloadFailure> = async {
            loop {
                let next = tokio::select! {
                    _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
                    next = stream.next() => next,
                };
                let Some(chunk) = next else { break };
                let chunk = chunk
                    .map_err(|_| MusicCommandError::new("DOWNLOAD_NETWORK", "音频下载中断"))?;
                written = written
                    .checked_add(chunk.len() as u64)
                    .filter(|size| *size <= MAX_AUDIO_BYTES)
                    .ok_or_else(|| {
                        MusicCommandError::new("DOWNLOAD_MEDIA", "音频文件超过 2 GiB 上限")
                    })?;
                if first_bytes.len() < 16 {
                    let remaining = 16 - first_bytes.len();
                    first_bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                }
                file.write_all(&chunk)
                    .await
                    .map_err(|_| MusicCommandError::new("FS_PATH", "写入音频临时文件失败"))?;
                if last_progress.elapsed() >= Duration::from_millis(200)
                    || total_bytes.is_some_and(|total| written >= total)
                {
                    let elapsed = started.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (written as f64 / elapsed) as u64
                    } else {
                        0
                    };
                    progress.emit(
                        DownloadProgressState::Downloading,
                        written,
                        total_bytes,
                        speed,
                    );
                    last_progress = Instant::now();
                }
            }
            if written == 0 {
                return Err(
                    MusicCommandError::new("DOWNLOAD_MEDIA", "下载到的音频文件为空").into(),
                );
            }
            file.flush()
                .await
                .map_err(|_| MusicCommandError::new("FS_PATH", "保存音频文件失败"))?;
            file.sync_all()
                .await
                .map_err(|_| MusicCommandError::new("FS_PATH", "保存音频文件失败"))?;
            Ok(())
        }
        .await;
        drop(file);
        if let Err(error) = transfer_result {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(error);
        }

        if cancel.is_cancelled() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(DownloadFailure::Cancelled);
        }

        let extension = detect_extension(
            &first_bytes,
            content_type.as_deref(),
            url_extension.as_deref(),
        )
        .ok_or_else(|| MusicCommandError::new("DOWNLOAD_MEDIA", "无法识别音频格式"));
        let extension = match extension {
            Ok(extension) => extension,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(error.into());
            }
        };
        let final_path = directory.join(format!("{file_stem}.{extension}"));
        if let Some(path) = find_existing_audio(directory, &file_stem, extension).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            self.record_existing(directory, &song.id, extension);
            return Ok(DownloadedFile::Skipped {
                path,
                warnings: Vec::new(),
            });
        }
        let mut warnings = Vec::new();
        if embed_cover {
            if matches!(extension, "mp3" | "flac") {
                match song.pic_id.as_deref().filter(|id| !id.trim().is_empty()) {
                    Some(pic_id) => {
                        match self
                            .fetch_and_embed_cover(&temp_path, source, pic_id, cancel)
                            .await
                        {
                            Ok(()) => {}
                            Err(CoverFailure::Cancelled) => {
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return Err(DownloadFailure::Cancelled);
                            }
                            Err(CoverFailure::Warning(error)) => {
                                warnings.push(format!("封面写入失败：{}", error.message));
                            }
                            Err(CoverFailure::Fatal(error)) => {
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return Err(DownloadFailure::Failed(error));
                            }
                        }
                    }
                    None => warnings.push("歌曲没有可用的封面信息".to_owned()),
                }
            } else {
                warnings.push(format!("{extension} 格式暂不支持写入封面"));
            }
        }
        if cancel.is_cancelled() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(DownloadFailure::Cancelled);
        }

        match commit_no_replace(&temp_path, &final_path).await {
            Ok(NoReplaceCommit::Committed) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                self.record_existing(directory, &song.id, extension);
                if download_lyrics {
                    match song.lyric_id.as_deref().filter(|id| !id.trim().is_empty()) {
                        Some(lyric_id) => {
                            match self
                                .fetch_and_write_lyric(&final_path, source, lyric_id, cancel)
                                .await
                            {
                                Ok(LyricWriteResult::Written) => {}
                                Ok(LyricWriteResult::AlreadyExists) => {
                                    warnings.push("歌词已存在，未覆盖".to_owned());
                                }
                                Err(DownloadFailure::Cancelled) => {
                                    warnings.push("歌词下载已取消，音频已保存".to_owned());
                                }
                                Err(DownloadFailure::Failed(error)) => {
                                    warnings.push(format!("歌词下载失败：{}", error.message));
                                }
                            }
                        }
                        None => warnings.push("歌曲没有可用的歌词信息".to_owned()),
                    }
                }
                Ok(DownloadedFile::Success {
                    path: final_path,
                    bytes: written,
                    warnings,
                })
            }
            Ok(NoReplaceCommit::AlreadyExists) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                self.record_existing(directory, &song.id, extension);
                Ok(DownloadedFile::Skipped {
                    path: final_path,
                    warnings,
                })
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                Err(
                    MusicCommandError::new("FS_COMMIT_UNSUPPORTED", "文件系统无法安全提交下载文件")
                        .into(),
                )
            }
        }
    }

    async fn request_gd_resource(
        &self,
        operation: GdOperation,
        cancel: &CancellationToken,
        action: &'static str,
    ) -> Result<Vec<u8>, DownloadFailure> {
        for attempt in 0..3 {
            self.limiter
                .acquire(Some(cancel))
                .await
                .map_err(|_| DownloadFailure::Cancelled)?;
            tokio::select! {
                _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
                result = self.runtime.ensure_initialized() => result
                    .map_err(|_| MusicCommandError::new("MUSIC_SIGNATURE", format!("{action}时签名页面初始化失败")))?,
            }
            let signature = tokio::select! {
                _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
                result = self.runtime.sign(operation.signature_input()) => result
                    .map_err(|_| MusicCommandError::new("MUSIC_SIGNATURE", format!("{action}时签名失败")))?,
            };
            let response = tokio::select! {
                _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
                result = self
                    .api_client
                    .post(GD_API_URL)
                    .header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(render_form_body(&operation, &signature))
                    .send() => result.map_err(|_| MusicCommandError::new("MUSIC_HTTP", format!("{action}失败")))?,
            };
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok());
                self.limiter.observe_too_many_requests(retry_after);
                if attempt < 2 {
                    continue;
                }
            }
            if !response.status().is_success() {
                return Err(MusicCommandError::new(
                    "MUSIC_HTTP",
                    format!("{action}返回 HTTP {}", response.status().as_u16()),
                )
                .into());
            }
            return read_limited(response, MAX_API_RESPONSE_BYTES, cancel).await;
        }
        unreachable!("three rate-limited attempts must return")
    }

    async fn fetch_and_embed_cover(
        &self,
        audio_path: &Path,
        source: GdSource,
        picture_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(), CoverFailure> {
        let operation = GdOperation::Pic {
            id: EncodedComponent::encode(picture_id),
            source,
            size: 1000,
        };
        let body = self
            .request_gd_resource(operation, cancel, "获取封面地址")
            .await
            .map_err(CoverFailure::from)?;
        let location = parse_picture_response(&body)
            .map_err(|_| MusicCommandError::new("MUSIC_SCHEMA", "封面地址格式不兼容"))?;
        let response = network_policy::safe_media_get(location.url, cancel).await?;
        if !response.status().is_success() {
            return Err(media_network_error().into());
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_COVER_BYTES as u64)
        {
            return Err(MusicCommandError::new("DOWNLOAD_MEDIA", "封面超过 20 MiB 上限").into());
        }
        let bytes = read_limited(response, MAX_COVER_BYTES, cancel)
            .await
            .map_err(CoverFailure::from)?;
        let mime_type = validate_cover_image(&bytes)?;

        let directory = audio_path
            .parent()
            .ok_or_else(|| MusicCommandError::new("FS_PATH", "无法确定封面备份目录"))?;
        let (backup_path, mut backup_file) = self.create_temp_file(directory).await?;
        let mut source_file = match tokio::fs::File::open(audio_path).await {
            Ok(file) => file,
            Err(_) => {
                let _ = tokio::fs::remove_file(&backup_path).await;
                return Err(CoverFailure::Warning(MusicCommandError::new(
                    "FS_PATH",
                    "无法读取封面写入前的音频文件",
                )));
            }
        };
        let backup_result = tokio::select! {
            _ = cancel.cancelled() => Err(CoverFailure::Cancelled),
            result = tokio::io::copy(&mut source_file, &mut backup_file) => result
                .map(|_| ())
                .map_err(|_| CoverFailure::Warning(MusicCommandError::new("FS_PATH", "无法备份封面写入前的音频文件"))),
        };
        if let Err(error) = backup_result {
            drop(backup_file);
            let _ = tokio::fs::remove_file(&backup_path).await;
            return Err(error);
        }
        if backup_file.flush().await.is_err() || backup_file.sync_all().await.is_err() {
            drop(backup_file);
            let _ = tokio::fs::remove_file(&backup_path).await;
            return Err(CoverFailure::Warning(MusicCommandError::new(
                "FS_PATH",
                "无法保存封面写入前的音频备份",
            )));
        }
        drop(backup_file);
        drop(source_file);
        if cancel.is_cancelled() {
            let _ = tokio::fs::remove_file(&backup_path).await;
            return Err(CoverFailure::Cancelled);
        }

        let path = audio_path.to_owned();
        let embed_result =
            tokio::task::spawn_blocking(move || embed_cover(&path, bytes, mime_type))
                .await
                .map_err(|_| MusicCommandError::new("DOWNLOAD_MEDIA", "封面写入任务异常"))
                .and_then(|result| result);
        match embed_result {
            Ok(()) => {
                let _ = tokio::fs::remove_file(&backup_path).await;
                Ok(())
            }
            Err(embed_error) => {
                if restore_audio_backup(audio_path, &backup_path).await.is_ok() {
                    Err(CoverFailure::Warning(embed_error))
                } else {
                    let _ = tokio::fs::remove_file(audio_path).await;
                    let _ = tokio::fs::remove_file(&backup_path).await;
                    Err(CoverFailure::Fatal(MusicCommandError::new(
                        "DOWNLOAD_MEDIA",
                        "封面写入失败且无法恢复原始音频，已放弃该歌曲",
                    )))
                }
            }
        }
    }

    async fn fetch_and_write_lyric(
        &self,
        audio_path: &Path,
        source: GdSource,
        lyric_id: &str,
        cancel: &CancellationToken,
    ) -> Result<LyricWriteResult, DownloadFailure> {
        let operation = GdOperation::Lyric {
            id: EncodedComponent::encode(lyric_id),
            source,
        };
        let body = self
            .request_gd_resource(operation, cancel, "获取歌词")
            .await?;
        let lyric = parse_lyric_response(&body)
            .map_err(|_| MusicCommandError::new("MUSIC_SCHEMA", "歌词格式不兼容"))?
            .original
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| MusicCommandError::new("MUSIC_SCHEMA", "没有可用的原始歌词"))?;
        if cancel.is_cancelled() {
            return Err(DownloadFailure::Cancelled);
        }

        let directory = audio_path
            .parent()
            .ok_or_else(|| MusicCommandError::new("FS_PATH", "无法确定歌词保存目录"))?;
        let final_path = audio_path.with_extension("lrc");
        let (temp_path, mut file) = self.create_temp_file(directory).await?;
        let write_result: Result<(), MusicCommandError> = async {
            file.write_all(lyric.as_bytes())
                .await
                .map_err(|_| MusicCommandError::new("FS_PATH", "写入歌词临时文件失败"))?;
            file.flush()
                .await
                .map_err(|_| MusicCommandError::new("FS_PATH", "保存歌词失败"))?;
            file.sync_all()
                .await
                .map_err(|_| MusicCommandError::new("FS_PATH", "保存歌词失败"))?;
            Ok(())
        }
        .await;
        drop(file);
        if let Err(error) = write_result {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(error.into());
        }
        if cancel.is_cancelled() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(DownloadFailure::Cancelled);
        }
        match commit_no_replace(&temp_path, &final_path).await {
            Ok(NoReplaceCommit::Committed) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                Ok(LyricWriteResult::Written)
            }
            Ok(NoReplaceCommit::AlreadyExists) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                Ok(LyricWriteResult::AlreadyExists)
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                Err(
                    MusicCommandError::new("FS_COMMIT_UNSUPPORTED", "文件系统无法安全提交歌词文件")
                        .into(),
                )
            }
        }
    }

    async fn create_temp_file(
        &self,
        directory: &Path,
    ) -> Result<(PathBuf, tokio::fs::File), MusicCommandError> {
        for _ in 0..16 {
            let id = self.next_temp_id.fetch_add(1, Ordering::AcqRel) + 1;
            let path = directory.join(format!(".yinmi-{}-{id}.part", std::process::id()));
            match tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .await
            {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    return Err(MusicCommandError::new("FS_PATH", "无法创建音频临时文件"));
                }
            }
        }
        Err(MusicCommandError::new("FS_PATH", "无法分配音频临时文件"))
    }

    fn store_progress(&self, progress: DownloadProgress) {
        let mut active = self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(control) = active
            .as_mut()
            .filter(|control| control.batch_id == progress.batch_id)
        {
            control.progress = Some(progress);
        }
    }

    fn store_item_result(&self, batch_id: u64, item: DownloadItemResult) {
        let mut active = self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(control) = active
            .as_mut()
            .filter(|control| control.batch_id == batch_id)
        {
            control.items.push(item);
        }
    }

    pub(crate) fn snapshot(&self) -> DownloadStateSnapshot {
        let active = self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let progress = active.as_ref().and_then(|control| control.progress.clone());
        let active_items = active
            .as_ref()
            .map(|control| control.items.clone())
            .unwrap_or_default();
        let is_active = active.is_some();
        drop(active);
        let last_result = self
            .last_result
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        DownloadStateSnapshot {
            active: is_active,
            progress,
            active_items,
            last_result,
        }
    }

    async fn scan_existing(
        &self,
        app: &AppHandle,
        search: &SearchResult,
        base_directory: &str,
    ) -> Result<ExistingAudioScan, MusicCommandError> {
        let base = resolve_base_directory(app, base_directory).await?;
        let directory = base.join(sanitize_segment(&search.keyword, 120, "音乐下载"));
        let mut file_names = HashSet::new();
        match tokio::fs::read_dir(&directory).await {
            Ok(mut entries) => {
                while let Some(entry) = entries
                    .next_entry()
                    .await
                    .map_err(|_| MusicCommandError::new("FS_PATH", "无法扫描下载目录"))?
                {
                    if entry
                        .file_type()
                        .await
                        .map(|kind| kind.is_file())
                        .unwrap_or(false)
                        && let Some(name) = entry.file_name().to_str()
                    {
                        file_names.insert(name.to_owned());
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(MusicCommandError::new("FS_PATH", "无法扫描下载目录")),
        }

        let items = search
            .songs
            .iter()
            .filter_map(|song| {
                let stem = song_file_stem(song);
                let extensions = AUDIO_EXTENSIONS
                    .iter()
                    .filter(|extension| file_names.contains(&format!("{stem}.{extension}")))
                    .map(|extension| (*extension).to_owned())
                    .collect::<Vec<_>>();
                (!extensions.is_empty()).then(|| ExistingAudioEntry {
                    song_id: song.id.clone(),
                    extensions,
                })
            })
            .collect();
        let scan = ExistingAudioScan {
            search_request_id: search.request_id,
            directory: directory.to_string_lossy().into_owned(),
            items,
        };
        *self
            .dedupe_scan
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(scan.clone());
        Ok(scan)
    }

    fn record_existing(&self, directory: &Path, song_id: &str, extension: &str) {
        let directory = directory.to_string_lossy();
        let mut scan = self
            .dedupe_scan
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(scan) = scan
            .as_mut()
            .filter(|scan| scan.directory == directory.as_ref())
        else {
            return;
        };
        if let Some(item) = scan.items.iter_mut().find(|item| item.song_id == song_id) {
            if !item.extensions.iter().any(|value| value == extension) {
                item.extensions.push(extension.to_owned());
            }
        } else {
            scan.items.push(ExistingAudioEntry {
                song_id: song_id.to_owned(),
                extensions: vec![extension.to_owned()],
            });
        }
    }

    fn next_song_token(&self, batch_id: u64) -> CancellationToken {
        let token = CancellationToken::new();
        let mut active = self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(control) = active
            .as_mut()
            .filter(|control| control.batch_id == batch_id)
        {
            control.cancel_current = token.clone();
            if control.cancel_all {
                token.cancel();
            }
        } else {
            token.cancel();
        }
        token
    }

    fn cancel_all_requested(&self, batch_id: u64) -> bool {
        self.active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .is_none_or(|control| control.batch_id != batch_id || control.cancel_all)
    }

    fn finish_batch(&self, batch_id: u64) {
        let mut active = self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active
            .as_ref()
            .is_some_and(|control| control.batch_id == batch_id)
        {
            *active = None;
        }
    }

    fn cancel(&self, scope: CancelScope) -> CancelDownloadResult {
        let mut active = self
            .active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(control) = active.as_mut() else {
            return CancelDownloadResult {
                accepted: false,
                batch_id: None,
                scope,
            };
        };
        if matches!(scope, CancelScope::All) {
            control.cancel_all = true;
        }
        control.cancel_current.cancel();
        CancelDownloadResult {
            accepted: true,
            batch_id: Some(control.batch_id),
            scope,
        }
    }

    pub fn has_active_batch(&self) -> bool {
        self.active_batch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub fn cancel_all_for_exit(&self) -> bool {
        self.cancel(CancelScope::All).accepted
    }

    fn open_last_directory(&self) -> Result<(), MusicCommandError> {
        let directory = self
            .last_directory
            .lock()
            .map_err(|_| MusicCommandError::new("FS_PATH", "下载目录状态不可用"))?
            .clone()
            .ok_or_else(|| MusicCommandError::new("FS_PATH", "还没有可打开的下载目录"))?;
        if !directory.is_dir() {
            return Err(MusicCommandError::new("FS_PATH", "下载目录不存在"));
        }
        open_directory(&directory)
    }
}

fn song_file_stem(song: &ProbeSong) -> String {
    let id = sanitize_segment(&song.id, 60, "unknown");
    let name = sanitize_segment(&song.name, 140, "未命名歌曲");
    format!("【{id}】{name}")
}

async fn find_existing_audio(directory: &Path, stem: &str, extension: &str) -> Option<PathBuf> {
    let path = directory.join(format!("{stem}.{extension}"));
    tokio::fs::symlink_metadata(&path).await.ok().map(|_| path)
}

async fn restore_audio_backup(audio_path: &Path, backup_path: &Path) -> io::Result<()> {
    match tokio::fs::remove_file(audio_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match commit_no_replace(backup_path, audio_path).await? {
        NoReplaceCommit::Committed => Ok(()),
        NoReplaceCommit::AlreadyExists => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "audio temp path was recreated while restoring backup",
        )),
    }
}

async fn commit_no_replace(source: &Path, destination: &Path) -> io::Result<NoReplaceCommit> {
    let source = source.to_owned();
    let destination = destination.to_owned();
    tokio::task::spawn_blocking(move || commit_no_replace_sync(&source, &destination))
        .await
        .map_err(|_| io::Error::other("no-replace file commit task failed"))?
}

#[cfg(target_os = "windows")]
fn commit_no_replace_sync(source: &Path, destination: &Path) -> io::Result<NoReplaceCommit> {
    use std::os::windows::ffi::OsStrExt;

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // flags=0 deliberately omits MOVEFILE_REPLACE_EXISTING.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) } != 0 {
        return Ok(NoReplaceCommit::Committed);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::AlreadyExists
        || matches!(error.raw_os_error(), Some(80 | 183))
    {
        Ok(NoReplaceCommit::AlreadyExists)
    } else {
        Err(error)
    }
}

#[cfg(target_os = "macos")]
fn commit_no_replace_sync(source: &Path, destination: &Path) -> io::Result<NoReplaceCommit> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    const AT_FDCWD: i32 = -2;
    const RENAME_EXCL: u32 = 0x0000_0004;
    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination path contains NUL")
    })?;
    if unsafe {
        renameatx_np(
            AT_FDCWD,
            source.as_ptr(),
            AT_FDCWD,
            destination.as_ptr(),
            RENAME_EXCL,
        )
    } == 0
    {
        return Ok(NoReplaceCommit::Committed);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::AlreadyExists {
        Ok(NoReplaceCommit::AlreadyExists)
    } else {
        Err(error)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn commit_no_replace_sync(source: &Path, destination: &Path) -> io::Result<NoReplaceCommit> {
    use std::io::{Read, Write};

    let mut source_file = std::fs::File::open(source)?;
    let mut destination_file = match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(NoReplaceCommit::AlreadyExists);
        }
        Err(error) => return Err(error),
    };
    let copy_result = (|| -> io::Result<()> {
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let read = source_file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            destination_file.write_all(&buffer[..read])?;
        }
        destination_file.flush()?;
        destination_file.sync_all()
    })();
    drop(destination_file);
    if let Err(error) = copy_result {
        let _ = std::fs::remove_file(destination);
        return Err(error);
    }
    let _ = std::fs::remove_file(source);
    Ok(NoReplaceCommit::Committed)
}

async fn resolve_base_directory(
    app: &AppHandle,
    requested: &str,
) -> Result<PathBuf, MusicCommandError> {
    let requested = requested.trim();
    if requested.is_empty() {
        return app
            .path()
            .audio_dir()
            .map_err(|_| MusicCommandError::new("FS_PATH", "无法获取系统音乐目录"));
    }

    let path = PathBuf::from(requested);
    if !path.is_absolute() {
        return Err(MusicCommandError::new(
            "FS_PATH",
            "自定义下载目录必须是绝对路径",
        ));
    }
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| MusicCommandError::new("FS_PATH", "自定义下载目录不存在"))?;
    if !metadata.is_dir() {
        return Err(MusicCommandError::new("FS_PATH", "自定义下载路径不是目录"));
    }
    tokio::fs::canonicalize(path)
        .await
        .map_err(|_| MusicCommandError::new("FS_PATH", "无法读取自定义下载目录"))
}

async fn cleanup_stale_temp_files(directory: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(".yinmi-") || !name.ends_with(".part") {
            continue;
        }
        let is_stale = entry
            .metadata()
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= STALE_TEMP_AGE);
        if is_stale {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

fn validate_cover_image(bytes: &[u8]) -> Result<MimeType, MusicCommandError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| MusicCommandError::new("DOWNLOAD_MEDIA", "无法识别封面图片"))?;
    let format = reader
        .format()
        .filter(|format| matches!(format, ImageFormat::Jpeg | ImageFormat::Png))
        .ok_or_else(|| MusicCommandError::new("DOWNLOAD_MEDIA", "封面仅支持 JPEG 或 PNG"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| MusicCommandError::new("DOWNLOAD_MEDIA", "封面图片已损坏"))?;
    if width == 0 || height == 0 || width > MAX_COVER_DIMENSION || height > MAX_COVER_DIMENSION {
        return Err(MusicCommandError::new(
            "DOWNLOAD_MEDIA",
            "封面尺寸不能超过 4096 × 4096",
        ));
    }
    image::load_from_memory_with_format(bytes, format)
        .map_err(|_| MusicCommandError::new("DOWNLOAD_MEDIA", "封面图片已损坏"))?;
    Ok(match format {
        ImageFormat::Jpeg => MimeType::Jpeg,
        ImageFormat::Png => MimeType::Png,
        _ => unreachable!("format was restricted above"),
    })
}

fn embed_cover(
    audio_path: &Path,
    bytes: Vec<u8>,
    mime_type: MimeType,
) -> Result<(), MusicCommandError> {
    let mut tagged_file = lofty::read_from_path(audio_path)
        .map_err(|_| MusicCommandError::new("DOWNLOAD_MEDIA", "无法读取音频标签"))?;
    let picture = Picture::unchecked(bytes)
        .pic_type(PictureType::CoverFront)
        .mime_type(mime_type)
        .build();
    if let Some(tag) = tagged_file.primary_tag_mut() {
        tag.push_picture(picture);
    } else {
        let mut tag = Tag::new(tagged_file.primary_tag_type());
        tag.push_picture(picture);
        tagged_file.insert_tag(tag);
    }
    tagged_file
        .save_to_path(audio_path, WriteOptions::default())
        .map_err(|_| MusicCommandError::new("DOWNLOAD_MEDIA", "无法写入音频封面"))
}

async fn read_limited(
    response: reqwest::Response,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, DownloadFailure> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
            next = stream.next() => next,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|_| MusicCommandError::new("MUSIC_HTTP", "音乐服务响应中断"))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(MusicCommandError::new("MUSIC_HTTP", "音乐服务响应过大").into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn sanitize_segment(value: &str, max_bytes: usize, fallback: &str) -> String {
    let mut output = String::new();
    let mut previous_replacement = false;
    for character in value.trim().chars() {
        let forbidden = character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            );
        if forbidden {
            if !previous_replacement {
                output.push('_');
            }
            previous_replacement = true;
        } else {
            output.push(character);
            previous_replacement = false;
        }
    }
    let output = output.trim_matches([' ', '.']).to_owned();
    let output = truncate_utf8(&output, max_bytes);
    let mut output = if output.is_empty() {
        fallback.to_owned()
    } else {
        output
    };
    let reserved = output
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || reserved
            .strip_prefix("COM")
            .or_else(|| reserved.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
    {
        output.insert(0, '_');
    }
    output
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    let mut end = 0;
    for (index, character) in value.char_indices() {
        let next = index + character.len_utf8();
        if next > max_bytes {
            break;
        }
        end = next;
    }
    value[..end].to_owned()
}

fn extension_from_path(path: &str) -> Option<&'static str> {
    match Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => Some("mp3"),
        Some("flac") => Some("flac"),
        Some("m4a" | "mp4") => Some("m4a"),
        Some("aac") => Some("aac"),
        Some("ogg" | "opus") => Some("ogg"),
        Some("wav") => Some("wav"),
        _ => None,
    }
}

fn detect_extension(
    first: &[u8],
    content_type: Option<&str>,
    url_extension: Option<&str>,
) -> Option<&'static str> {
    if first.starts_with(b"fLaC") {
        return Some("flac");
    }
    if first
        .get(..2)
        .is_some_and(|bytes| bytes[0] == 0xff && bytes[1] & 0xf6 == 0xf0)
    {
        return Some("aac");
    }
    if first.starts_with(b"ID3")
        || first.get(..2).is_some_and(|bytes| {
            bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0 && bytes[1] & 0x06 != 0
        })
    {
        return Some("mp3");
    }
    if first.starts_with(b"OggS") {
        return Some("ogg");
    }
    if first.len() >= 12 && &first[..4] == b"RIFF" && &first[8..12] == b"WAVE" {
        return Some("wav");
    }
    if first.len() >= 8 && &first[4..8] == b"ftyp" {
        return Some("m4a");
    }
    let mime = content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default();
    match mime {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/mp4" | "audio/x-m4a" | "video/mp4" => Some("m4a"),
        "audio/aac" => Some("aac"),
        "audio/ogg" | "application/ogg" => Some("ogg"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        _ => match url_extension {
            Some("mp3") => Some("mp3"),
            Some("flac") => Some("flac"),
            Some("m4a") => Some("m4a"),
            Some("aac") => Some("aac"),
            Some("ogg") => Some("ogg"),
            Some("wav") => Some("wav"),
            _ => None,
        },
    }
}

#[cfg(target_os = "windows")]
fn open_directory(directory: &Path) -> Result<(), MusicCommandError> {
    Command::new("explorer.exe")
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|_| MusicCommandError::new("FS_PATH", "无法打开下载目录"))
}

#[cfg(target_os = "macos")]
fn open_directory(directory: &Path) -> Result<(), MusicCommandError> {
    Command::new("open")
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|_| MusicCommandError::new("FS_PATH", "无法打开下载目录"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn open_directory(directory: &Path) -> Result<(), MusicCommandError> {
    Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|_| MusicCommandError::new("FS_PATH", "无法打开下载目录"))
}

#[tauri::command]
pub async fn music_download_batch(
    app: AppHandle,
    service: State<'_, Arc<MusicDownloadService>>,
    search: State<'_, Arc<MusicSearchService>>,
    request: DownloadStartRequest,
) -> Result<DownloadBatchResult, MusicCommandError> {
    let (keyword, source, songs) =
        search.resolve_download_selection(request.search_request_id, &request.song_ids)?;
    let request = DownloadBatchRequest {
        keyword,
        source,
        songs,
        bitrate: request.bitrate,
        embed_cover: request.embed_cover,
        download_lyrics: request.download_lyrics,
        base_directory: request.base_directory,
    };
    log::info!(
        "下载批次开始 source={} total={} bitrate={} cover={} lyric={}",
        request.source.wire_value(),
        request.songs.len(),
        request.bitrate,
        request.embed_cover,
        request.download_lyrics
    );
    match service.download_batch(&app, request).await {
        Ok(result) => {
            log::info!(
                "下载批次完成 batch_id={} success={} skipped={} failed={} cancelled={}",
                result.batch_id,
                result.succeeded,
                result.skipped,
                result.failed,
                result.cancelled
            );
            Ok(result)
        }
        Err(error) => {
            log::warn!("下载批次失败 code={}", error.code);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn music_retry_failed(
    app: AppHandle,
    service: State<'_, Arc<MusicDownloadService>>,
    song_id: Option<String>,
) -> Result<DownloadBatchResult, MusicCommandError> {
    log::info!("重试下载 target={}", song_id.as_deref().unwrap_or("all"));
    service.retry_failed(&app, song_id).await
}

#[tauri::command]
pub fn music_get_default_directory(app: AppHandle) -> Result<String, MusicCommandError> {
    app.path()
        .audio_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|_| MusicCommandError::new("FS_PATH", "无法获取系统音乐目录"))
}

#[tauri::command]
pub fn music_cancel_current_download(
    service: State<'_, Arc<MusicDownloadService>>,
) -> CancelDownloadResult {
    let result = service.cancel(CancelScope::Current);
    log::info!("取消当前下载 accepted={}", result.accepted);
    result
}

#[tauri::command]
pub fn music_cancel_all_downloads(
    service: State<'_, Arc<MusicDownloadService>>,
) -> CancelDownloadResult {
    let result = service.cancel(CancelScope::All);
    log::info!("取消全部下载 accepted={}", result.accepted);
    result
}

#[tauri::command]
pub fn music_open_download_directory(
    service: State<'_, Arc<MusicDownloadService>>,
) -> Result<(), MusicCommandError> {
    service.open_last_directory()
}

#[tauri::command]
pub fn music_get_download_snapshot(
    service: State<'_, Arc<MusicDownloadService>>,
) -> DownloadStateSnapshot {
    service.snapshot()
}

#[tauri::command]
pub async fn music_scan_existing(
    app: AppHandle,
    service: State<'_, Arc<MusicDownloadService>>,
    search: State<'_, Arc<MusicSearchService>>,
    request: ExistingAudioScanRequest,
) -> Result<ExistingAudioScan, MusicCommandError> {
    let snapshot = search.snapshot().result.ok_or_else(|| {
        MusicCommandError::new("MUSIC_SEARCH_STALE", "搜索结果已失效，请重新搜索")
    })?;
    if snapshot.request_id != request.search_request_id {
        return Err(MusicCommandError::new(
            "MUSIC_SEARCH_STALE",
            "搜索结果已变化，请重新扫描",
        ));
    }
    service
        .scan_existing(&app, &snapshot, &request.base_directory)
        .await
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{detect_extension, find_existing_audio};

    #[test]
    fn distinguishes_adts_aac_from_mpeg_audio() {
        assert_eq!(
            detect_extension(&[0xff, 0xf1, 0x50, 0x80], None, None),
            Some("aac")
        );
        assert_eq!(
            detect_extension(&[0xff, 0xfb, 0x90, 0x64], None, None),
            Some("mp3")
        );
    }

    #[tokio::test]
    async fn existing_mp3_does_not_block_flac() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("yinmi-dedupe-{unique}"));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(directory.join("song.mp3"), b"existing")
            .await
            .unwrap();

        assert!(
            find_existing_audio(&directory, "song", "flac")
                .await
                .is_none()
        );
        assert!(
            find_existing_audio(&directory, "song", "mp3")
                .await
                .is_some()
        );

        tokio::fs::remove_dir_all(directory).await.unwrap();
    }
}
