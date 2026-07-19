use std::{collections::VecDeque, sync::Mutex, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const REQUEST_LIMIT: usize = 60;
const REQUEST_WINDOW: Duration = Duration::from_secs(60);
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(30);
const RATE_LIMIT_EVENT: &str = "music-rate-limit";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitNotice {
    wait_seconds: u64,
}

#[derive(Default)]
struct RateLimitState {
    requests: VecDeque<Instant>,
    cooldown_until: Option<Instant>,
}

pub(crate) struct MusicRateLimiter {
    app: AppHandle,
    state: Mutex<RateLimitState>,
}

impl MusicRateLimiter {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self {
            app,
            state: Mutex::new(RateLimitState::default()),
        }
    }

    pub(crate) async fn acquire(&self, cancel: Option<&CancellationToken>) -> Result<(), ()> {
        loop {
            let wait = {
                let now = Instant::now();
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                while state
                    .requests
                    .front()
                    .is_some_and(|request| now.duration_since(*request) >= REQUEST_WINDOW)
                {
                    state.requests.pop_front();
                }
                if state.cooldown_until.is_some_and(|until| until <= now) {
                    state.cooldown_until = None;
                }

                let quota_wait = (state.requests.len() >= REQUEST_LIMIT)
                    .then(|| state.requests[0] + REQUEST_WINDOW - now);
                let cooldown_wait = state
                    .cooldown_until
                    .filter(|until| *until > now)
                    .map(|until| until - now);
                match (quota_wait, cooldown_wait) {
                    (Some(left), Some(right)) => Some(left.max(right)),
                    (Some(wait), None) | (None, Some(wait)) => Some(wait),
                    (None, None) => {
                        state.requests.push_back(now);
                        None
                    }
                }
            };

            let Some(wait) = wait else {
                self.emit_wait(0);
                return Ok(());
            };
            let seconds = wait
                .as_secs()
                .saturating_add(u64::from(wait.subsec_nanos() > 0));
            log::info!("音乐接口进入限流冷却，约 {seconds} 秒后继续");
            self.emit_wait(seconds);
            if let Some(cancel) = cancel {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        self.emit_wait(0);
                        return Err(());
                    }
                    _ = tokio::time::sleep(wait) => {}
                }
            } else {
                tokio::time::sleep(wait).await;
            }
        }
    }

    pub(crate) fn observe_too_many_requests(&self, retry_after: Option<&str>) {
        let wait = retry_after
            .map(parse_retry_after_value)
            .unwrap_or(DEFAULT_RETRY_AFTER);
        let until = Instant::now() + wait;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.cooldown_until.is_none_or(|current| current < until) {
            state.cooldown_until = Some(until);
        }
        log::warn!("音乐接口返回 429，进入 {} 秒冷却", wait.as_secs());
    }

    fn emit_wait(&self, wait_seconds: u64) {
        let _ = self
            .app
            .emit(RATE_LIMIT_EVENT, RateLimitNotice { wait_seconds });
    }
}

fn parse_retry_after_value(value: &str) -> Duration {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .map(|seconds| seconds.clamp(1, REQUEST_WINDOW.as_secs()))
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_RETRY_AFTER)
}
