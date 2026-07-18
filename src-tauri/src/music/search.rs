use std::time::Duration;
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use thiserror::Error;

use crate::signature::signature_webview::{SignatureError, SignatureRuntime};
use crate::music::{
    contract::{
        ContractError, EncodedComponent, GdOperation, GdSource, PaginationDecision,
        PaginationProbe, ParsedSearchPage, ProbeSong, SearchCount, SearchOperation, StopReason,
        parse_search_page_lenient, render_form_body,
    },
    rate_limiter::MusicRateLimiter,
};

const GD_API_URL: &str = "https://music.gdstudio.xyz/api.php";
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const MAX_KEYWORD_CHARS: usize = 200;
const MAX_SEARCH_PAGES: u16 = 50;
const SEARCH_COMPLETE_EVENT: &str = "music-search-complete";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRequest {
    pub keyword: String,
    pub source: GdSource,
    pub mode: SearchOperation,
    pub count: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub request_id: u64,
    pub keyword: String,
    pub source: GdSource,
    pub source_name: &'static str,
    pub mode: SearchOperation,
    pub requested_count: u16,
    pub returned_count: usize,
    pub skipped_records: usize,
    pub incomplete: bool,
    pub stop_reason: StopReason,
    pub songs: Vec<ProbeSong>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStateSnapshot {
    pub active: bool,
    pub result: Option<SearchResult>,
}

#[derive(Debug, Error)]
enum SearchError {
    #[error("请输入搜索关键词")]
    EmptyKeyword,
    #[error("搜索关键词不能超过 200 个字符")]
    KeywordTooLong,
    #[error("搜索数量必须在 1 到 1000 之间")]
    InvalidCount,
    #[error("签名页面初始化或签名失败")]
    Signature,
    #[error("音乐接口网络请求失败")]
    Network,
    #[error("音乐接口返回 HTTP {0}")]
    Http(u16),
    #[error("音乐接口响应超过 5 MiB")]
    ResponseTooLarge,
    #[error("音乐接口响应格式不兼容")]
    Schema,
}

impl From<SignatureError> for SearchError {
    fn from(_: SignatureError) -> Self {
        Self::Signature
    }
}

impl From<ContractError> for SearchError {
    fn from(_: ContractError) -> Self {
        Self::Schema
    }
}

#[derive(Clone, Debug, Error, Serialize)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct MusicCommandError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchCompleteEvent {
    result: Option<SearchResult>,
    error: Option<MusicCommandError>,
}

impl MusicCommandError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<SearchError> for MusicCommandError {
    fn from(error: SearchError) -> Self {
        let code = match error {
            SearchError::EmptyKeyword | SearchError::KeywordTooLong | SearchError::InvalidCount => {
                "MUSIC_INPUT"
            }
            SearchError::Signature => "MUSIC_SIGNATURE",
            SearchError::Network | SearchError::Http(_) | SearchError::ResponseTooLarge => {
                "MUSIC_HTTP"
            }
            SearchError::Schema => "MUSIC_SCHEMA",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

pub struct MusicSearchService {
    runtime: Arc<SignatureRuntime>,
    limiter: Arc<MusicRateLimiter>,
    client: reqwest::Client,
    next_request_id: AtomicU64,
    active: AtomicBool,
    latest_snapshot: Mutex<Option<SearchResult>>,
}

struct SearchActivityGuard<'a>(&'a AtomicBool);

impl<'a> SearchActivityGuard<'a> {
    fn begin(active: &'a AtomicBool) -> Self {
        active.store(true, Ordering::Release);
        Self(active)
    }
}

impl Drop for SearchActivityGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl MusicSearchService {
    pub(crate) fn new(
        runtime: Arc<SignatureRuntime>,
        limiter: Arc<MusicRateLimiter>,
    ) -> Result<Self, MusicCommandError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .tls_backend_rustls()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| MusicCommandError::from(SearchError::Network))?;
        Ok(Self {
            runtime,
            limiter,
            client,
            next_request_id: AtomicU64::new(0),
            active: AtomicBool::new(false),
            latest_snapshot: Mutex::new(None),
        })
    }

    async fn fetch_search_page(
        &self,
        operation: GdOperation,
    ) -> Result<ParsedSearchPage, SearchError> {
        for attempt in 0..3 {
            self.limiter
                .acquire(None)
                .await
                .map_err(|_| SearchError::Network)?;
            let signature = self.runtime.sign(operation.signature_input()).await?;
            let response = self
                .client
                .post(GD_API_URL)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(render_form_body(&operation, &signature))
                .send()
                .await
                .map_err(|_| SearchError::Network)?;
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
                return Err(SearchError::Http(response.status().as_u16()));
            }

            let mut body = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| SearchError::Network)?;
                if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err(SearchError::ResponseTooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            return Ok(parse_search_page_lenient(&body)?);
        }
        unreachable!("three rate-limited attempts must return")
    }

    async fn search(&self, request: SearchRequest) -> Result<SearchResult, SearchError> {
        let keyword = request.keyword.trim().to_owned();
        if keyword.is_empty() {
            return Err(SearchError::EmptyKeyword);
        }
        if keyword.chars().count() > MAX_KEYWORD_CHARS {
            return Err(SearchError::KeywordTooLong);
        }
        let count = SearchCount::try_from(request.count).map_err(|_| SearchError::InvalidCount)?;
        *self
            .latest_snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

        self.runtime.ensure_initialized().await?;
        let mut pagination = PaginationProbe::new(usize::from(count.get()), MAX_SEARCH_PAGES);
        let name = EncodedComponent::encode(&keyword);
        let mut page = 1;
        let mut skipped_records: usize = 0;
        let (stop_reason, incomplete) = loop {
            let operation = GdOperation::Search {
                operation: request.mode,
                count,
                source: request.source,
                page,
                name: name.clone(),
            };
            let decision = match self.fetch_search_page(operation).await {
                Ok(parsed) => {
                    skipped_records = skipped_records.saturating_add(parsed.skipped_records);
                    pagination.push_page(Ok((parsed, false)))
                }
                Err(error) => match pagination.push_page(Err(ContractError::InvalidTopLevel)) {
                    PaginationDecision::Failed { .. } => return Err(error),
                    decision => decision,
                },
            };

            match decision {
                PaginationDecision::Continue { next_page } => {
                    page = next_page;
                }
                PaginationDecision::Complete { reason, incomplete } => break (reason, incomplete),
                PaginationDecision::Failed { .. } => unreachable!("successful pages cannot fail"),
            }
        };

        let mut songs = pagination.songs;
        songs.truncate(usize::from(count.get()));
        let request_id = self.next_request_id.fetch_add(1, Ordering::AcqRel) + 1;
        let result = SearchResult {
            request_id,
            keyword,
            source: request.source,
            source_name: request.source.display_name(),
            mode: request.mode,
            requested_count: count.get(),
            returned_count: songs.len(),
            skipped_records,
            incomplete,
            stop_reason,
            songs,
        };
        *self
            .latest_snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result.clone());
        Ok(result)
    }

    pub(crate) fn snapshot(&self) -> SearchStateSnapshot {
        let result = self
            .latest_snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        SearchStateSnapshot {
            active: self.active.load(Ordering::Acquire),
            result,
        }
    }

    pub(crate) fn resolve_download_selection(
        &self,
        request_id: u64,
        song_ids: &[String],
    ) -> Result<(String, GdSource, Vec<ProbeSong>), MusicCommandError> {
        if song_ids.is_empty() || song_ids.len() > 1_000 {
            return Err(MusicCommandError::new(
                "INPUT_INVALID",
                "请选择 1 到 1000 首歌曲后再下载",
            ));
        }
        let selected = song_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .collect::<HashSet<_>>();
        if selected.len() != song_ids.len() {
            return Err(MusicCommandError::new(
                "INPUT_INVALID",
                "下载选择中包含空或重复的歌曲",
            ));
        }
        let snapshot = self.snapshot().result.ok_or_else(|| {
            MusicCommandError::new("MUSIC_SEARCH_STALE", "搜索结果已失效，请重新搜索")
        })?;
        if snapshot.request_id != request_id {
            return Err(MusicCommandError::new(
                "MUSIC_SEARCH_STALE",
                "搜索结果已变化，请重新选择歌曲",
            ));
        }
        let songs = snapshot
            .songs
            .iter()
            .filter(|song| selected.contains(song.id.as_str()) && song.url_id.is_some())
            .cloned()
            .collect::<Vec<_>>();
        if songs.len() != selected.len() {
            return Err(MusicCommandError::new(
                "MUSIC_SEARCH_STALE",
                "部分歌曲不在当前搜索结果中或不可下载",
            ));
        }
        Ok((snapshot.keyword, snapshot.source, songs))
    }
}

#[tauri::command]
pub fn music_get_search_snapshot(
    service: State<'_, Arc<MusicSearchService>>,
) -> SearchStateSnapshot {
    service.snapshot()
}

#[tauri::command]
pub async fn music_search(
    app: AppHandle,
    service: State<'_, Arc<MusicSearchService>>,
    request: SearchRequest,
) -> Result<SearchResult, MusicCommandError> {
    log::info!(
        "搜索开始 source={} mode={:?} count={}",
        request.source.wire_value(),
        request.mode,
        request.count
    );
    let active_guard = SearchActivityGuard::begin(&service.active);
    let search_result = service.search(request).await;
    drop(active_guard);
    match search_result {
        Ok(result) => {
            log::info!(
                "搜索完成 request_id={} returned={} skipped={} incomplete={}",
                result.request_id,
                result.returned_count,
                result.skipped_records,
                result.incomplete
            );
            let _ = app.emit(
                SEARCH_COMPLETE_EVENT,
                SearchCompleteEvent {
                    result: Some(result.clone()),
                    error: None,
                },
            );
            Ok(result)
        }
        Err(error) => {
            let command_error = MusicCommandError::from(error);
            log::warn!("搜索失败 code={}", command_error.code);
            let _ = app.emit(
                SEARCH_COMPLETE_EVENT,
                SearchCompleteEvent {
                    result: None,
                    error: Some(command_error.clone()),
                },
            );
            Err(command_error)
        }
    }
}
