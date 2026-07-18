use std::time::{Duration, SystemTime};

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::signature_webview::{SignatureError, SignatureRuntime};
use crate::music::contract::{
    ContractError, EncodedComponent, GdOperation, GdSource, PaginationDecision, PaginationProbe,
    SearchCount, SearchOperation, StopReason, parse_search_page, render_form_body,
};

const GD_API_URL: &str = "https://music.gdstudio.xyz/api.php";
pub const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(6_500);
pub const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolProbeCase {
    SingleCount1000,
    PagedOfficial20,
    RepeatSamePage,
}

#[derive(Debug, Error)]
pub enum GdProbeError {
    #[error("GD probe was cancelled")]
    Cancelled,
    #[error("GD probe response exceeded 5 MiB")]
    ResponseTooLarge,
    #[error("GD probe was rate limited")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("GD probe received HTTP status {0}")]
    HttpStatus(u16),
    #[error("GD probe transport failed")]
    Transport,
    #[error("GD probe response contract failed")]
    Contract,
    #[error("GD signature failed")]
    Signature,
    #[error("GD probe plan is invalid")]
    InvalidPlan,
}

impl From<SignatureError> for GdProbeError {
    fn from(_: SignatureError) -> Self {
        Self::Signature
    }
}

impl From<ContractError> for GdProbeError {
    fn from(_: ContractError) -> Self {
        Self::Contract
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolRequestReport {
    pub page: u16,
    pub requested_count: u16,
    pub raw_records: usize,
    pub valid_songs: usize,
    pub skipped_records: usize,
    pub start_offset_millis: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolProbeReport {
    pub probe_case: ProtocolProbeCase,
    pub source: String,
    pub requested_count: u16,
    pub api_requests: usize,
    pub minimum_start_interval_millis: u64,
    pub unique_songs: usize,
    pub stop_reason: Option<StopReason>,
    pub requests: Vec<ProtocolRequestReport>,
}

struct ProbePlan {
    count: SearchCount,
    pages: Vec<u16>,
    source: GdSource,
    encoded_keyword: EncodedComponent,
    minimum_start_interval: Duration,
}

fn probe_plan(probe_case: ProtocolProbeCase) -> Result<ProbePlan, GdProbeError> {
    let (count, pages) = match probe_case {
        ProtocolProbeCase::SingleCount1000 => (SearchCount::try_from(1_000), vec![1]),
        ProtocolProbeCase::PagedOfficial20 => (SearchCount::try_from(20), (1..=50).collect()),
        ProtocolProbeCase::RepeatSamePage => (SearchCount::try_from(20), vec![1, 1]),
    };
    let count = count.map_err(|_| GdProbeError::InvalidPlan)?;
    let source = GdSource::DEFAULT;
    if source.wire_value() != "netease" {
        return Err(GdProbeError::InvalidPlan);
    }
    Ok(ProbePlan {
        count,
        pages,
        source,
        encoded_keyword: EncodedComponent::encode("\u{5468}\u{6770}\u{4f26}"),
        minimum_start_interval: MIN_REQUEST_INTERVAL,
    })
}

fn parse_retry_after(raw: Option<&str>, now: SystemTime) -> Option<Duration> {
    let raw = raw?;
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let parsed =
        time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc2822).ok()?;
    let now = time::OffsetDateTime::from(now);
    let seconds = parsed.unix_timestamp().saturating_sub(now.unix_timestamp());
    Some(Duration::from_secs(u64::try_from(seconds).unwrap_or(0)))
}

fn classify_http_status(
    status: u16,
    retry_after: Option<&str>,
    now: SystemTime,
) -> Result<(), GdProbeError> {
    if status == 429 {
        return Err(GdProbeError::RateLimited {
            retry_after_seconds: parse_retry_after(retry_after, now)
                .map(|duration| duration.as_secs()),
        });
    }
    if !(200..300).contains(&status) {
        return Err(GdProbeError::HttpStatus(status));
    }
    Ok(())
}

async fn collect_bounded_body<S, B, E>(
    mut stream: S,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, GdProbeError>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    let mut body = Vec::new();
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(GdProbeError::Cancelled),
            next = stream.next() => next,
        };
        let Some(chunk) = next else {
            return Ok(body);
        };
        let chunk = chunk.map_err(|_| GdProbeError::Transport)?;
        let bytes = chunk.as_ref();
        if body.len().saturating_add(bytes.len()) > MAX_RESPONSE_BYTES {
            return Err(GdProbeError::ResponseTooLarge);
        }
        body.extend_from_slice(bytes);
    }
}

fn build_client() -> Result<reqwest::Client, GdProbeError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .tls_backend_rustls()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| GdProbeError::Transport)
}

async fn post_form(
    client: &reqwest::Client,
    form_body: String,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, GdProbeError> {
    let request = client
        .post(GD_API_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body)
        .send();
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(GdProbeError::Cancelled),
        response = request => response.map_err(|_| GdProbeError::Transport)?,
    };
    let status = response.status();
    classify_http_status(
        status.as_u16(),
        response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        SystemTime::now(),
    )?;
    collect_bounded_body(response.bytes_stream(), cancel).await
}

async fn wait_for_request_start(
    previous_start: Option<Instant>,
    cancel: &CancellationToken,
) -> Result<Instant, GdProbeError> {
    if let Some(previous_start) = previous_start {
        tokio::select! {
            _ = cancel.cancelled() => return Err(GdProbeError::Cancelled),
            _ = tokio::time::sleep_until(previous_start + MIN_REQUEST_INTERVAL) => {}
        }
    }
    if cancel.is_cancelled() {
        return Err(GdProbeError::Cancelled);
    }
    Ok(Instant::now())
}

pub async fn run_gd_probe(
    runtime: &SignatureRuntime,
    probe_case: ProtocolProbeCase,
    cancel: &CancellationToken,
) -> Result<ProtocolProbeReport, GdProbeError> {
    let plan = probe_plan(probe_case)?;
    let client = build_client()?;
    let probe_start = Instant::now();
    let mut previous_start = None;
    let mut requests = Vec::new();
    let mut pagination = matches!(probe_case, ProtocolProbeCase::PagedOfficial20)
        .then(|| PaginationProbe::new(1_000, 50));
    let mut stop_reason = None;

    for page in plan.pages.iter().copied() {
        if cancel.is_cancelled() {
            return Err(GdProbeError::Cancelled);
        }
        let operation = GdOperation::Search {
            operation: SearchOperation::Track,
            count: plan.count,
            source: plan.source,
            page,
            name: plan.encoded_keyword.clone(),
        };
        let signature = runtime.sign(operation.signature_input()).await?;
        let form_body = render_form_body(&operation, &signature);
        let request_start = wait_for_request_start(previous_start, cancel).await?;
        previous_start = Some(request_start);
        let body = post_form(&client, form_body, cancel).await?;
        let parsed = parse_search_page(&body)?;
        requests.push(ProtocolRequestReport {
            page,
            requested_count: plan.count.get(),
            raw_records: parsed.raw_records,
            valid_songs: parsed.songs.len(),
            skipped_records: parsed.skipped_records,
            start_offset_millis: u64::try_from(
                request_start.duration_since(probe_start).as_millis(),
            )
            .unwrap_or(u64::MAX),
        });
        if let Some(pagination) = &mut pagination {
            match pagination.push_page(Ok((parsed, false))) {
                PaginationDecision::Continue { .. } => {}
                PaginationDecision::Complete { reason, .. }
                | PaginationDecision::Failed { reason } => {
                    stop_reason = Some(reason);
                    break;
                }
            }
        }
    }

    let unique_songs = pagination.as_ref().map_or_else(
        || requests.iter().map(|request| request.valid_songs).sum(),
        |pagination| pagination.songs.len(),
    );
    Ok(ProtocolProbeReport {
        probe_case,
        source: plan.source.wire_value().into(),
        requested_count: plan.count.get(),
        api_requests: requests.len(),
        minimum_start_interval_millis: u64::try_from(plan.minimum_start_interval.as_millis())
            .unwrap_or(u64::MAX),
        unique_songs,
        stop_reason,
        requests,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use futures_util::stream;
    use tokio_util::sync::CancellationToken;

    use super::{
        MAX_RESPONSE_BYTES, MIN_REQUEST_INTERVAL, ProtocolProbeCase, classify_http_status,
        collect_bounded_body, parse_retry_after, probe_plan,
    };

    #[test]
    fn signature_webview_gd_live_plans_are_fixed_and_typed() {
        let single = probe_plan(ProtocolProbeCase::SingleCount1000).unwrap();
        assert_eq!(single.count.get(), 1_000);
        assert_eq!(single.pages, vec![1]);
        assert_eq!(single.source.wire_value(), "netease");

        let paged = probe_plan(ProtocolProbeCase::PagedOfficial20).unwrap();
        assert_eq!(paged.count.get(), 20);
        assert_eq!(paged.pages, (1..=50).collect::<Vec<_>>());
        assert_eq!(paged.minimum_start_interval, MIN_REQUEST_INTERVAL);

        let repeat = probe_plan(ProtocolProbeCase::RepeatSamePage).unwrap();
        assert_eq!(repeat.count.get(), 20);
        assert_eq!(repeat.pages, vec![1, 1]);
        assert_eq!(repeat.minimum_start_interval, Duration::from_millis(6_500));
        assert_eq!(
            repeat.encoded_keyword.as_str(),
            "%E5%91%A8%E6%9D%B0%E4%BC%A6"
        );
    }

    #[test]
    fn signature_webview_gd_live_retry_after_is_parsed_without_retrying() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            parse_retry_after(Some("7"), now),
            Some(Duration::from_secs(7))
        );
        assert_eq!(parse_retry_after(None, now), None);
        assert_eq!(parse_retry_after(Some("invalid"), now), None);
        assert_eq!(
            parse_retry_after(Some("Tue, 14 Nov 2023 22:13:25 GMT"), now),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            parse_retry_after(Some("Tue, 14 Nov 2023 22:13:15 GMT"), now),
            Some(Duration::ZERO)
        );

        assert!(classify_http_status(200, None, now).is_ok());
        assert!(matches!(
            classify_http_status(429, Some("7"), now),
            Err(super::GdProbeError::RateLimited {
                retry_after_seconds: Some(7)
            })
        ));
        assert!(matches!(
            classify_http_status(429, None, now),
            Err(super::GdProbeError::RateLimited {
                retry_after_seconds: None
            })
        ));
        assert!(matches!(
            classify_http_status(503, None, now),
            Err(super::GdProbeError::HttpStatus(503))
        ));
    }

    #[tokio::test]
    async fn signature_webview_gd_live_stream_is_bounded_and_cancellable() {
        let cancel = CancellationToken::new();
        let body = collect_bounded_body(
            stream::iter([Ok::<_, std::io::Error>(b"ok".to_vec())]),
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(body, b"ok");

        let oversized = collect_bounded_body(
            stream::iter([Ok::<_, std::io::Error>(vec![0; MAX_RESPONSE_BYTES + 1])]),
            &cancel,
        )
        .await;
        assert!(matches!(
            oversized,
            Err(super::GdProbeError::ResponseTooLarge)
        ));

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let result = collect_bounded_body(
            stream::pending::<Result<Vec<u8>, std::io::Error>>(),
            &cancelled,
        )
        .await;
        assert!(matches!(result, Err(super::GdProbeError::Cancelled)));
    }
}
