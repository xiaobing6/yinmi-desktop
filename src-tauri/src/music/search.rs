use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::State;
use thiserror::Error;

use crate::feasibility::signature_webview::{SignatureError, SignatureRuntime};
use crate::music::contract::{
    ContractError, EncodedComponent, GdOperation, GdSource, PaginationDecision, PaginationProbe,
    ParsedSearchPage, ProbeSong, SearchCount, SearchOperation, StopReason,
    parse_search_page_lenient, render_form_body,
};

const GD_API_URL: &str = "https://music.gdstudio.xyz/api.php";
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const MAX_KEYWORD_CHARS: usize = 200;
const MAX_SEARCH_PAGES: u16 = 50;

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
    client: reqwest::Client,
    next_request_id: AtomicU64,
}

impl MusicSearchService {
    pub fn new(runtime: Arc<SignatureRuntime>) -> Result<Self, MusicCommandError> {
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
            client,
            next_request_id: AtomicU64::new(0),
        })
    }

    async fn fetch_search_page(
        &self,
        operation: GdOperation,
    ) -> Result<ParsedSearchPage, SearchError> {
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
        Ok(parse_search_page_lenient(&body)?)
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
                    tokio::time::sleep(Duration::from_millis(125)).await;
                }
                PaginationDecision::Complete { reason, incomplete } => break (reason, incomplete),
                PaginationDecision::Failed { .. } => unreachable!("successful pages cannot fail"),
            }
        };

        let mut songs = pagination.songs;
        songs.truncate(usize::from(count.get()));
        let request_id = self.next_request_id.fetch_add(1, Ordering::AcqRel) + 1;
        Ok(SearchResult {
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
        })
    }
}

#[tauri::command]
pub async fn music_search(
    service: State<'_, Arc<MusicSearchService>>,
    request: SearchRequest,
) -> Result<SearchResult, MusicCommandError> {
    log::info!(
        "搜索开始 source={} mode={:?} count={}",
        request.source.wire_value(),
        request.mode,
        request.count
    );
    match service.search(request).await {
        Ok(result) => {
            log::info!(
                "搜索完成 request_id={} returned={} skipped={} incomplete={}",
                result.request_id,
                result.returned_count,
                result.skipped_records,
                result.incomplete
            );
            Ok(result)
        }
        Err(error) => {
            let command_error = MusicCommandError::from(error);
            log::warn!("搜索失败 code={}", command_error.code);
            Err(command_error)
        }
    }
}
