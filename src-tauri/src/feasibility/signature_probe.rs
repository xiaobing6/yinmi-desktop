use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tauri::Manager;
use tokio::sync::Notify;
use url::Url;

use super::{signature_webview::SignatureError, webview_resource_policy::IsolationCounterSnapshot};

pub const RESOURCE_VECTORS: [&str; 20] = [
    "document",
    "iframe",
    "script",
    "style",
    "image",
    "media",
    "fetch",
    "xhr",
    "worker",
    "service_worker",
    "websocket",
    "sse",
    "beacon",
    "redirect",
    "popup",
    "download",
    "top_level_data",
    "top_level_blob",
    "top_level_file",
    "top_level_custom_protocol",
];

pub const FIXED_SCENARIO_IDS: [&str; 6] = [
    "policy-registration-fault",
    "initialization-finished-delay-past-20s",
    "sign-callback-delay-past-5s",
    "destroy-during-pending-policy",
    "late-callback-after-new-generation",
    "main-close-state-machine-seam",
];

pub const SERVICE_WORKER_API_ABSENT_EXPRESSION: &str = r#"!("serviceWorker" in navigator)"#;

pub const AUTORUN_ENV: &str = "YINMI_FEASIBILITY_SIGNATURE_AUTORUN";
pub const TRACE_ENDPOINT_ENV: &str = "YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT";
pub const RUN_ID_ENV: &str = "YINMI_FEASIBILITY_SIGNATURE_RUN_ID";
const CANARY_IDLE_DURATION_MS: u64 = 600_000;
const LIFECYCLE_CYCLE_COUNT: usize = 20;
const SIGNATURE_PLATFORM_IDS: [&str; 4] = [
    "windows-10-webview2-111-x64",
    "windows-11-x64",
    "macos-13-intel",
    "macos-current-arm64",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlledCanaryConfigWire {
    run_id: String,
    phase: String,
    platform_id: String,
    control_origin: String,
    allowed_origin: String,
    blocked_http_origin: String,
    blocked_https_origin: String,
    blocked_ws_origin: String,
    blocked_wss_origin: String,
    idle_duration_ms: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ControlledCanaryConfig {
    pub(crate) run_id: String,
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    pub(crate) platform_id: String,
    pub(crate) control_origin: Url,
    pub(crate) allowed_origin: Url,
    pub(crate) blocked_http_origin: Url,
    pub(crate) blocked_https_origin: Url,
    pub(crate) blocked_ws_origin: Url,
    pub(crate) blocked_wss_origin: Url,
    pub(crate) idle_duration_ms: u64,
}

fn exact_loopback_origin(raw: &str, scheme: &str) -> Result<Url, SignatureError> {
    let parsed = Url::parse(raw)
        .map_err(|_| SignatureError::Webview("invalid controlled canary origin".into()))?;
    if parsed.scheme() != scheme
        || parsed.host_str() != Some("127.0.0.1")
        || parsed.port().is_none()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(SignatureError::Webview(format!(
            "controlled canary {scheme} origin must be an exact IPv4 loopback origin"
        )));
    }
    Ok(parsed)
}

pub(crate) fn parse_controlled_canary_config(
    raw: &str,
    expected_run_id: &str,
    expected_phase: AutorunPhase,
) -> Result<ControlledCanaryConfig, SignatureError> {
    let wire: ControlledCanaryConfigWire = serde_json::from_str(raw)
        .map_err(|_| SignatureError::Webview("invalid controlled canary configuration".into()))?;
    if wire.run_id.len() != 32
        || !wire
            .run_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SignatureError::Webview(
            "controlled canary run ID must be lowercase 128-bit hex".into(),
        ));
    }
    if wire.run_id != expected_run_id || wire.phase != expected_phase.as_str() {
        return Err(SignatureError::Webview(
            "controlled canary config correlation failed".into(),
        ));
    }
    if !SIGNATURE_PLATFORM_IDS.contains(&wire.platform_id.as_str()) {
        return Err(SignatureError::Webview(
            "controlled canary platform ID is not frozen".into(),
        ));
    }
    if wire.idle_duration_ms != CANARY_IDLE_DURATION_MS {
        return Err(SignatureError::Webview(
            "controlled canary idle duration must be exactly ten minutes".into(),
        ));
    }
    let control_origin = exact_loopback_origin(&wire.control_origin, "http")?;
    let allowed_origin = exact_loopback_origin(&wire.allowed_origin, "https")?;
    let blocked_http_origin = exact_loopback_origin(&wire.blocked_http_origin, "http")?;
    let blocked_https_origin = exact_loopback_origin(&wire.blocked_https_origin, "https")?;
    let blocked_ws_origin = exact_loopback_origin(&wire.blocked_ws_origin, "ws")?;
    let blocked_wss_origin = exact_loopback_origin(&wire.blocked_wss_origin, "wss")?;
    if control_origin.port() != blocked_http_origin.port()
        || control_origin.port() != blocked_ws_origin.port()
        || blocked_https_origin.port() != blocked_wss_origin.port()
        || allowed_origin.port() == blocked_https_origin.port()
    {
        return Err(SignatureError::Webview(
            "controlled canary origin port correlation failed".into(),
        ));
    }
    Ok(ControlledCanaryConfig {
        run_id: wire.run_id,
        platform_id: wire.platform_id,
        control_origin,
        allowed_origin,
        blocked_http_origin,
        blocked_https_origin,
        blocked_ws_origin,
        blocked_wss_origin,
        idle_duration_ms: wire.idle_duration_ms,
    })
}

const WRITE_MARKER_PHASE: &str = "write-marker-and-close-main";
const VERIFY_MARKER_PHASE: &str = "verify-marker-absent";
static IPC_CANARY_COUNT: AtomicU64 = AtomicU64::new(0);
const IPC_CANARY_READY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum IpcCanaryReadinessState {
    Inactive,
    Armed,
    BaselineIssued,
    ReadyAccepted,
    Sealed,
    Failed,
}

impl IpcCanaryReadinessState {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            value if value == Self::Inactive as u8 => Some(Self::Inactive),
            value if value == Self::Armed as u8 => Some(Self::Armed),
            value if value == Self::BaselineIssued as u8 => Some(Self::BaselineIssued),
            value if value == Self::ReadyAccepted as u8 => Some(Self::ReadyAccepted),
            value if value == Self::Sealed as u8 => Some(Self::Sealed),
            value if value == Self::Failed as u8 => Some(Self::Failed),
            _ => None,
        }
    }
}

struct IpcCanaryReadinessGate {
    state: AtomicU8,
    run_id: Mutex<Option<String>>,
    notify: Notify,
}

impl IpcCanaryReadinessGate {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(IpcCanaryReadinessState::Inactive as u8),
            run_id: Mutex::new(None),
            notify: Notify::const_new(),
        }
    }

    fn arm(&self, run_id: &str) -> Result<(), SignatureError> {
        self.state
            .compare_exchange(
                IpcCanaryReadinessState::Inactive as u8,
                IpcCanaryReadinessState::Armed as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| {
                SignatureError::Webview("IPC canary readiness gate already used".into())
            })?;
        *self
            .run_id
            .lock()
            .map_err(|_| SignatureError::Webview("IPC canary readiness gate poisoned".into()))? =
            Some(run_id.to_owned());
        Ok(())
    }

    fn accept_canary(&self) {
        if self
            .state
            .compare_exchange(
                IpcCanaryReadinessState::BaselineIssued as u8,
                IpcCanaryReadinessState::ReadyAccepted as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.notify.notify_one();
        }
    }

    fn validate_run_id(&self, run_id: &str) -> Result<(), SignatureError> {
        let armed_run_id = self
            .run_id
            .lock()
            .map_err(|_| SignatureError::Webview("IPC canary readiness gate poisoned".into()))?;
        if armed_run_id.as_deref() != Some(run_id) {
            return Err(SignatureError::Webview(
                "IPC canary readiness run ID mismatch".into(),
            ));
        }
        Ok(())
    }

    async fn await_ready(&self, run_id: &str) -> Result<(), SignatureError> {
        self.validate_run_id(run_id)?;
        self.state
            .compare_exchange(
                IpcCanaryReadinessState::Armed as u8,
                IpcCanaryReadinessState::BaselineIssued as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| {
                SignatureError::Webview("IPC canary readiness gate was not armed".into())
            })?;

        if IPC_CANARY_COUNT.load(Ordering::Acquire) > 0 {
            self.accept_canary();
        }

        let accepted = tokio::time::timeout(IPC_CANARY_READY_DEADLINE, async {
            loop {
                let notified = self.notify.notified();
                match IpcCanaryReadinessState::from_u8(self.state.load(Ordering::Acquire)) {
                    Some(IpcCanaryReadinessState::ReadyAccepted) => break,
                    Some(IpcCanaryReadinessState::Failed | IpcCanaryReadinessState::Sealed)
                    | None => {
                        return Err(SignatureError::Webview(
                            "IPC canary readiness gate entered an invalid state".into(),
                        ));
                    }
                    _ => notified.await,
                }
            }
            Ok(())
        })
        .await;

        match accepted {
            Ok(Ok(())) => {
                self.state
                    .compare_exchange(
                        IpcCanaryReadinessState::ReadyAccepted as u8,
                        IpcCanaryReadinessState::Sealed as u8,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .map_err(|_| {
                        SignatureError::Webview(
                            "IPC canary readiness gate could not be sealed".into(),
                        )
                    })?;
                Ok(())
            }
            Ok(Err(error)) => Err(error),
            Err(_) => {
                let _ = self.state.compare_exchange(
                    IpcCanaryReadinessState::BaselineIssued as u8,
                    IpcCanaryReadinessState::Failed as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                Err(SignatureError::Webview(
                    "IPC canary readiness timed out after 15 seconds".into(),
                ))
            }
        }
    }
}

static IPC_CANARY_READINESS: IpcCanaryReadinessGate = IpcCanaryReadinessGate::new();

pub const RESOURCE_VECTOR_TRIGGERS: [(&str, &str); 20] = [
    (
        "document",
        r#"append <object type="text/html" data=BLOCKED_HTTPS_URL>"#,
    ),
    ("iframe", r#"append <iframe src=BLOCKED_HTTPS_URL>"#),
    ("script", r#"append <script src=BLOCKED_HTTPS_URL>"#),
    (
        "style",
        r#"append <link rel="stylesheet" href=BLOCKED_HTTPS_URL>"#,
    ),
    ("image", "set new Image().src = BLOCKED_HTTPS_URL"),
    (
        "media",
        r#"append <audio preload="auto" src=BLOCKED_HTTPS_URL> and call load()"#,
    ),
    (
        "fetch",
        r#"await fetch(BLOCKED_HTTPS_URL, { mode: "no-cors", cache: "no-store" })"#,
    ),
    ("xhr", "XMLHttpRequest GET BLOCKED_HTTPS_URL"),
    (
        "worker",
        "create a blob Worker whose only statement is importScripts(BLOCKED_HTTPS_URL)",
    ),
    (
        "service_worker",
        "register same-origin /sw.js; install fetches BLOCKED_HTTPS_URL",
    ),
    ("websocket", "new WebSocket(BLOCKED_WSS_URL)"),
    ("sse", "new EventSource(BLOCKED_HTTPS_SSE_URL)"),
    (
        "beacon",
        "navigator.sendBeacon(BLOCKED_HTTPS_URL, one fixed byte)",
    ),
    (
        "redirect",
        "fetch ALLOWED_HTTPS_URL/redirect/one through /redirect/two",
    ),
    (
        "popup",
        r#"window.open(BLOCKED_HTTPS_URL, "_blank", "noopener")"#,
    ),
    (
        "download",
        "click a connected <a download href=BLOCKED_HTTPS_URL>",
    ),
    (
        "top_level_data",
        r#"location.assign("data:text/html,yinmi-probe")"#,
    ),
    (
        "top_level_blob",
        "create text/html Blob URL, location.assign(url), then revoke it",
    ),
    (
        "top_level_file",
        r#"location.assign("file:///yinmi-feasibility-denied")"#,
    ),
    (
        "top_level_custom_protocol",
        r#"location.assign("yinmi-feasibility-denied://probe")"#,
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutorunPhase {
    WriteMarkerAndCloseMain,
    VerifyMarkerAbsent,
}

impl AutorunPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WriteMarkerAndCloseMain => WRITE_MARKER_PHASE,
            Self::VerifyMarkerAbsent => VERIFY_MARKER_PHASE,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AutorunConfig {
    pub phase: AutorunPhase,
    pub endpoint: Url,
    pub run_id: String,
}

pub fn parse_autorun_environment_values(
    phase: Option<&str>,
    endpoint: Option<&str>,
    run_id: Option<&str>,
) -> Result<Option<AutorunConfig>, SignatureError> {
    if phase.is_none() && endpoint.is_none() && run_id.is_none() {
        return Ok(None);
    }
    let phase = match phase {
        Some(WRITE_MARKER_PHASE) => AutorunPhase::WriteMarkerAndCloseMain,
        Some(VERIFY_MARKER_PHASE) => AutorunPhase::VerifyMarkerAbsent,
        _ => {
            return Err(SignatureError::Webview(
                "invalid lifecycle autorun phase".into(),
            ));
        }
    };
    let endpoint = Url::parse(
        endpoint
            .ok_or_else(|| SignatureError::Webview("missing lifecycle trace endpoint".into()))?,
    )
    .map_err(|_| SignatureError::Webview("invalid lifecycle trace endpoint".into()))?;
    if endpoint.scheme() != "http"
        || endpoint.host_str() != Some("127.0.0.1")
        || endpoint.port().is_none()
        || endpoint.path() != "/"
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
    {
        return Err(SignatureError::Webview(
            "lifecycle trace endpoint must be an IPv4 loopback origin".into(),
        ));
    }
    let run_id =
        run_id.ok_or_else(|| SignatureError::Webview("missing lifecycle run ID".into()))?;
    if run_id.len() != 32
        || !run_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SignatureError::Webview("invalid lifecycle run ID".into()));
    }
    Ok(Some(AutorunConfig {
        phase,
        endpoint,
        run_id: run_id.into(),
    }))
}

pub fn autorun_configuration_from_environment() -> Result<Option<AutorunConfig>, SignatureError> {
    let phase = std::env::var(AUTORUN_ENV).ok();
    let endpoint = std::env::var(TRACE_ENDPOINT_ENV).ok();
    let run_id = std::env::var(RUN_ID_ENV).ok();
    parse_autorun_environment_values(phase.as_deref(), endpoint.as_deref(), run_id.as_deref())
}

pub fn increment_ipc_canary() -> u64 {
    let count = IPC_CANARY_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    IPC_CANARY_READINESS.accept_canary();
    count
}

pub(crate) fn reset_ipc_canary() -> u64 {
    IPC_CANARY_COUNT.swap(0, Ordering::AcqRel)
}

pub(crate) fn ipc_canary_snapshot() -> u64 {
    IPC_CANARY_COUNT.load(Ordering::Acquire)
}

async fn await_ipc_canary_readiness(run_id: &str) -> Result<(), SignatureError> {
    IPC_CANARY_READINESS.await_ready(run_id).await
}

#[cfg(target_os = "macos")]
fn translated_process() -> Result<Option<bool>, SignatureError> {
    use std::{ffi::CString, io, mem, ptr};

    let name = CString::new("sysctl.proc_translated")
        .map_err(|_| SignatureError::Webview("invalid translation sysctl name".into()))?;
    let mut value: libc::c_int = 0;
    let mut value_size = mem::size_of::<libc::c_int>();
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            ptr::from_mut(&mut value).cast(),
            &mut value_size,
            ptr::null_mut(),
            0,
        )
    };
    if status == 0 {
        if value_size != mem::size_of::<libc::c_int>() || !matches!(value, 0 | 1) {
            return Err(SignatureError::Webview(
                "translation sysctl returned malformed data".into(),
            ));
        }
        return Ok(Some(value != 0));
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ENOENT) {
        Ok(Some(false))
    } else {
        Err(SignatureError::Webview(
            "translation sysctl query failed".into(),
        ))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleProcessInfo<'a> {
    run_id: &'a str,
    phase: &'a str,
    binary_target_os: &'static str,
    binary_target_arch: &'static str,
    translated_process: Option<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum AutorunKind {
    Isolation,
    Lifecycle,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProcessInfoAck {
    accepted: String,
    run_id: String,
    kind: AutorunKind,
    phase: String,
}

fn parse_process_info_ack(
    body: &[u8],
    expected_run_id: &str,
    expected_phase: AutorunPhase,
) -> Result<AutorunKind, SignatureError> {
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "process-info acknowledgement exceeded 4 KiB".into(),
        ));
    }
    let ack: ProcessInfoAck = serde_json::from_slice(body)
        .map_err(|_| SignatureError::Webview("process-info acknowledgement was invalid".into()))?;
    if ack.accepted != "process-info"
        || ack.run_id != expected_run_id
        || ack.phase != expected_phase.as_str()
    {
        return Err(SignatureError::Webview(
            "process-info acknowledgement correlation failed".into(),
        ));
    }
    Ok(ack.kind)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleEvent<'a> {
    run_id: &'a str,
    phase: &'a str,
    event: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanaryConfigRequest<'a> {
    run_id: &'a str,
    phase: &'a str,
}

#[derive(Deserialize)]
struct MarkerEvaluation {
    status: String,
    matches: bool,
}

async fn post_trace<T: Serialize>(
    client: &reqwest::Client,
    endpoint: &Url,
    path: &str,
    body: &T,
) -> Result<(), SignatureError> {
    let target = endpoint
        .join(path)
        .map_err(|_| SignatureError::Webview("lifecycle trace route failed".into()))?;
    let body = serde_json::to_vec(body)
        .map_err(|_| SignatureError::Webview("lifecycle trace serialization failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "lifecycle trace submission exceeded 4 KiB".into(),
        ));
    }
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|_| SignatureError::Webview("lifecycle trace delivery failed".into()))?;
    if !response.status().is_success() {
        return Err(SignatureError::Webview(
            "lifecycle trace recorder rejected a submission".into(),
        ));
    }
    let response_body = response
        .bytes()
        .await
        .map_err(|_| SignatureError::Webview("lifecycle trace acknowledgement failed".into()))?;
    if response_body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "lifecycle trace acknowledgement exceeded 4 KiB".into(),
        ));
    }
    Ok(())
}

async fn post_process_info(
    client: &reqwest::Client,
    config: &AutorunConfig,
    translated_process: Option<bool>,
) -> Result<AutorunKind, SignatureError> {
    let target = config
        .endpoint
        .join("process-info")
        .map_err(|_| SignatureError::Webview("process-info route failed".into()))?;
    let body = serde_json::to_vec(&LifecycleProcessInfo {
        run_id: &config.run_id,
        phase: config.phase.as_str(),
        binary_target_os: std::env::consts::OS,
        binary_target_arch: std::env::consts::ARCH,
        translated_process,
    })
    .map_err(|_| SignatureError::Webview("process-info serialization failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "process-info submission exceeded 4 KiB".into(),
        ));
    }
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|_| SignatureError::Webview("process-info delivery failed".into()))?;
    if !response.status().is_success() {
        return Err(SignatureError::Webview(
            "process-info recorder rejected the child".into(),
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| SignatureError::Webview("process-info acknowledgement failed".into()))?;
    parse_process_info_ack(&body, &config.run_id, config.phase)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IsolationReportSubmission<'a> {
    run_id: &'a str,
    phase: &'a str,
    report: &'a IsolationReport,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IsolationReportAck {
    accepted: String,
    run_id: String,
    kind: AutorunKind,
    phase: String,
}

async fn post_isolation_report(
    client: &reqwest::Client,
    config: &AutorunConfig,
    report: &IsolationReport,
) -> Result<(), SignatureError> {
    let target = config
        .endpoint
        .join("isolation-report")
        .map_err(|_| SignatureError::Webview("isolation report route failed".into()))?;
    let body = serde_json::to_vec(&IsolationReportSubmission {
        run_id: &config.run_id,
        phase: config.phase.as_str(),
        report,
    })
    .map_err(|_| SignatureError::Webview("isolation report serialization failed".into()))?;
    if body.len() > 256 * 1024 {
        return Err(SignatureError::Webview(
            "isolation report exceeded 256 KiB".into(),
        ));
    }
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|_| SignatureError::Webview("isolation report delivery failed".into()))?;
    if !response.status().is_success() {
        return Err(SignatureError::Webview(
            "isolation report recorder rejected the report".into(),
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| SignatureError::Webview("isolation report acknowledgement failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "isolation report acknowledgement exceeded 4 KiB".into(),
        ));
    }
    let ack: IsolationReportAck = serde_json::from_slice(&body).map_err(|_| {
        SignatureError::Webview("isolation report acknowledgement was invalid".into())
    })?;
    if ack.accepted != "isolation-report"
        || ack.run_id != config.run_id
        || ack.kind != AutorunKind::Isolation
        || ack.phase != config.phase.as_str()
    {
        return Err(SignatureError::Webview(
            "isolation report acknowledgement correlation failed".into(),
        ));
    }
    Ok(())
}

async fn controlled_canary_config_from_trace() -> Result<ControlledCanaryConfig, SignatureError> {
    let autorun = autorun_configuration_from_environment()?
        .ok_or_else(|| SignatureError::Webview("controlled canary runner is required".into()))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| SignatureError::Webview("controlled canary client failed".into()))?;
    let target = autorun
        .endpoint
        .join("canary-config")
        .map_err(|_| SignatureError::Webview("controlled canary config route failed".into()))?;
    let request = serde_json::to_vec(&CanaryConfigRequest {
        run_id: &autorun.run_id,
        phase: autorun.phase.as_str(),
    })
    .map_err(|_| SignatureError::Webview("controlled canary request failed".into()))?;
    if request.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "controlled canary request exceeded 4 KiB".into(),
        ));
    }
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request)
        .send()
        .await
        .map_err(|_| SignatureError::Webview("controlled canary runner is required".into()))?;
    if !response.status().is_success() {
        return Err(SignatureError::Webview(
            "controlled canary runner is required".into(),
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| SignatureError::Webview("controlled canary config read failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "controlled canary config exceeded 4 KiB".into(),
        ));
    }
    let raw = std::str::from_utf8(&body)
        .map_err(|_| SignatureError::Webview("controlled canary config was not UTF-8".into()))?;
    parse_controlled_canary_config(raw, &autorun.run_id, autorun.phase)
}

async fn post_lifecycle_event(
    client: &reqwest::Client,
    config: &AutorunConfig,
    event: &'static str,
) -> Result<(), SignatureError> {
    post_trace(
        client,
        &config.endpoint,
        "events",
        &LifecycleEvent {
            run_id: &config.run_id,
            phase: config.phase.as_str(),
            event,
        },
    )
    .await
}

fn lifecycle_marker_script(config: &AutorunConfig) -> String {
    let key = serde_json::to_string("yinmi-feasibility-signature-restart-marker")
        .expect("fixed marker key serializes");
    let value = serde_json::to_string(&config.run_id).expect("validated run ID serializes");
    match config.phase {
        AutorunPhase::WriteMarkerAndCloseMain => format!(
            r#"(() => {{
  try {{
    localStorage.setItem({key}, {value});
    return {{ status: "ok", matches: localStorage.getItem({key}) === {value} }};
  }} catch (_) {{
    return {{ status: "error", matches: false }};
  }}
}})()"#
        ),
        AutorunPhase::VerifyMarkerAbsent => format!(
            r#"(() => {{
  try {{
    return {{ status: "ok", matches: localStorage.getItem({key}) === null }};
  }} catch (_) {{
    return {{ status: "error", matches: false }};
  }}
}})()"#
        ),
    }
}

async fn run_lifecycle_autorun(
    app: tauri::AppHandle<tauri::Wry>,
    runtime: std::sync::Arc<super::signature_webview::SignatureRuntime>,
    coordinator: std::sync::Arc<super::signature_webview::SignatureExitCoordinator>,
    config: AutorunConfig,
) -> Result<(), SignatureError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| SignatureError::Webview("lifecycle trace client failed".into()))?;
    #[cfg(windows)]
    let translated = None;
    #[cfg(target_os = "macos")]
    let translated = translated_process()?;
    #[cfg(not(any(windows, target_os = "macos")))]
    let translated = None;
    let kind = post_process_info(&client, &config, translated).await?;
    if kind == AutorunKind::Isolation {
        if config.phase != AutorunPhase::WriteMarkerAndCloseMain {
            return Err(SignatureError::Webview(
                "isolation pre-stage requires the write-marker autorun phase".into(),
            ));
        }
        await_ipc_canary_readiness(&config.run_id).await?;
        let report = run_isolation_probe(&runtime).await?;
        post_isolation_report(&client, &config, &report).await?;
        app.exit(0);
        return Ok(());
    }
    if !coordinator.arm_lifecycle_probe() {
        return Err(SignatureError::Webview(
            "lifecycle exit barrier was already armed".into(),
        ));
    }
    post_lifecycle_event(&client, &config, "process-started").await?;

    runtime.initialize().await?;
    post_lifecycle_event(&client, &config, "active-host-ready").await?;
    let marker_raw = runtime
        .evaluate_probe_script(lifecycle_marker_script(&config))
        .await?;
    let marker: MarkerEvaluation = parse_probe_evaluation(&marker_raw)?;
    if marker.status != "ok" || !marker.matches {
        return Err(SignatureError::Webview(
            "lifecycle persistence predicate failed".into(),
        ));
    }
    post_lifecycle_event(
        &client,
        &config,
        match config.phase {
            AutorunPhase::WriteMarkerAndCloseMain => "marker-written",
            AutorunPhase::VerifyMarkerAbsent => "marker-absent",
        },
    )
    .await?;
    post_lifecycle_event(&client, &config, "main-close-requested").await?;
    app.get_window("main")
        .ok_or_else(|| SignatureError::Webview("main window is unavailable".into()))?
        .close()
        .map_err(|_| SignatureError::Webview("main close request failed".into()))?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        coordinator.wait_lifecycle_probe_cleanup(),
    )
    .await
    .map_err(|_| SignatureError::DestroyTimeout)??;

    for event in [
        "host-destroyed",
        "manager-host-absent",
        "policy-cleanup-acknowledged",
        "policy-tombstones-empty",
        "tls-entry-absent",
        "app-exit-invoked",
    ] {
        post_lifecycle_event(&client, &config, event).await?;
    }
    app.exit(0);
    Ok(())
}

pub fn start_lifecycle_autorun(
    app: tauri::AppHandle<tauri::Wry>,
    runtime: std::sync::Arc<super::signature_webview::SignatureRuntime>,
    coordinator: std::sync::Arc<super::signature_webview::SignatureExitCoordinator>,
) -> Result<(), SignatureError> {
    let Some(config) = autorun_configuration_from_environment()? else {
        return Ok(());
    };
    IPC_CANARY_READINESS.arm(&config.run_id)?;
    let failure_app = app.clone();
    let failure_runtime = std::sync::Arc::clone(&runtime);
    let failure_coordinator = std::sync::Arc::clone(&coordinator);
    tauri::async_runtime::spawn(async move {
        if run_lifecycle_autorun(app, runtime, coordinator, config)
            .await
            .is_err()
        {
            let _ = failure_runtime.destroy().await;
            failure_coordinator.complete_lifecycle_probe_cleanup();
            failure_app.exit(1);
        }
    });
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceVectorResult {
    pub runtime_attempted: bool,
    pub availability_outcome: String,
    pub deterministic_barrier_seam_covered: bool,
    pub expected_barrier: String,
    pub enforced_barrier: String,
    pub barrier_evidence_mode: String,
    pub counterfactual_server_hits: Option<u64>,
    pub allowed_redirect_hop_hits: u64,
    pub server_hits: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceWorkerObservation {
    ApiPresence(bool),
    Rejected,
    TimedOut,
    CspBlocked,
    ScriptFailed,
}

pub fn service_worker_availability(
    observation: ServiceWorkerObservation,
) -> Result<&'static str, SignatureError> {
    match observation {
        ServiceWorkerObservation::ApiPresence(false) => Ok("service-worker-api-absent"),
        ServiceWorkerObservation::ApiPresence(true) => Ok("available"),
        ServiceWorkerObservation::Rejected
        | ServiceWorkerObservation::TimedOut
        | ServiceWorkerObservation::CspBlocked
        | ServiceWorkerObservation::ScriptFailed => Err(SignatureError::Webview(
            "service-worker vector failed after the API was present".into(),
        )),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedScenarioReport {
    pub id: &'static str,
    pub generation: u64,
    pub operation_id: u64,
    pub ordered_actor_events: Vec<String>,
    pub terminal_state: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FixedScenarioChecks {
    timeout_check: bool,
    retry_check: bool,
    policy_fault_invalidates_instance: bool,
    late_callback_isolated: bool,
    destroy_confirmed_before_retry: bool,
    resource_policy_cleanup_acknowledged: bool,
    policy_tombstones_empty_before_exit: bool,
}

fn scenario_event_index(report: &FixedScenarioReport, event: &str) -> Option<usize> {
    report
        .ordered_actor_events
        .iter()
        .position(|candidate| candidate == event)
}

fn derive_fixed_scenario_checks(
    reports: &[FixedScenarioReport],
) -> Result<FixedScenarioChecks, SignatureError> {
    if reports.len() != FIXED_SCENARIO_IDS.len() {
        return Err(SignatureError::Webview(
            "fixed scenario matrix was incomplete".into(),
        ));
    }
    let mut generations = std::collections::BTreeSet::new();
    for (index, report) in reports.iter().enumerate() {
        if report.id != FIXED_SCENARIO_IDS[index]
            || report.terminal_state != "destroy-confirmed"
            || !generations.insert(report.generation)
        {
            return Err(SignatureError::Webview(
                "fixed scenario identity or terminal state failed".into(),
            ));
        }
    }
    let first_five = &reports[..5];
    let destroy_before_retry = first_five.iter().all(|report| {
        scenario_event_index(report, "teardown-complete")
            .zip(scenario_event_index(report, "retry-ready"))
            .is_some_and(|(destroyed, retry)| destroyed < retry)
    });
    let cleanup = first_five.iter().all(|report| {
        let audit_events = [
            "native-destroyed",
            "manager-host-absent",
            "policy-cleanup-acknowledged",
            "policy-tombstones-empty",
        ];
        let complete = scenario_event_index(report, "teardown-complete");
        complete.is_some_and(|complete| {
            audit_events.iter().all(|event| {
                report
                    .ordered_actor_events
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| candidate == event)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>()
                    .as_slice()
                    .first()
                    .is_some_and(|index| {
                        report
                            .ordered_actor_events
                            .iter()
                            .filter(|candidate| *candidate == event)
                            .count()
                            == 1
                            && *index < complete
                    })
            }) && report
                .ordered_actor_events
                .iter()
                .filter(|event| event.as_str() == "teardown-complete")
                .count()
                == 1
        })
    });
    let tombstones = first_five
        .iter()
        .all(|report| scenario_event_index(report, "policy-tombstones-empty").is_some());
    let timeout_check = scenario_event_index(&reports[1], "initialization-timeout-observed")
        .is_some()
        && scenario_event_index(&reports[2], "sign-timeout-observed").is_some();
    let retry_check = first_five
        .iter()
        .all(|report| scenario_event_index(report, "retry-destroyed").is_some());
    let policy_fault =
        scenario_event_index(&reports[0], "policy-registration-fault-observed").is_some();
    let late_callback = scenario_event_index(&reports[4], "new-generation-ready")
        .zip(scenario_event_index(&reports[4], "late-callback-isolated"))
        .is_some_and(|(generation, callback)| generation < callback);
    let main_close = scenario_event_index(&reports[5], "would-exit-blocked")
        .zip(scenario_event_index(&reports[5], "would-exit-released"))
        .is_some_and(|(blocked, released)| blocked < released);
    let checks = FixedScenarioChecks {
        timeout_check,
        retry_check,
        policy_fault_invalidates_instance: policy_fault,
        late_callback_isolated: late_callback,
        destroy_confirmed_before_retry: destroy_before_retry,
        resource_policy_cleanup_acknowledged: cleanup,
        policy_tombstones_empty_before_exit: tombstones && main_close,
    };
    if !checks.timeout_check
        || !checks.retry_check
        || !checks.policy_fault_invalidates_instance
        || !checks.late_callback_isolated
        || !checks.destroy_confirmed_before_retry
        || !checks.resource_policy_cleanup_acknowledged
        || !checks.policy_tombstones_empty_before_exit
    {
        return Err(SignatureError::Webview(
            "fixed scenario traces did not derive every mandatory check".into(),
        ));
    }
    Ok(checks)
}

trait FixedScenarioDriver {
    async fn run_scenario(
        &mut self,
        id: &'static str,
    ) -> Result<FixedScenarioReport, SignatureError>;
}

async fn execute_fixed_scenario_matrix<D: FixedScenarioDriver>(
    driver: &mut D,
) -> Result<(Vec<FixedScenarioReport>, FixedScenarioChecks), SignatureError> {
    let mut reports = Vec::with_capacity(FIXED_SCENARIO_IDS.len());
    for id in FIXED_SCENARIO_IDS {
        let report = driver.run_scenario(id).await?;
        if report.id != id {
            return Err(SignatureError::Webview(
                "fixed scenario driver returned the wrong ID".into(),
            ));
        }
        reports.push(report);
    }
    let checks = derive_fixed_scenario_checks(&reports)?;
    Ok((reports, checks))
}

struct RuntimeFixedScenarioDriver<'a> {
    runtime: &'a super::signature_webview::SignatureRuntime,
    retry_profile: super::signature_host::RawHostProfile,
}

fn append_verified_teardown_audit(
    audit: &super::signature_host::GenerationTeardownAudit,
    events: &mut Vec<String>,
) -> Result<(), SignatureError> {
    if !audit.is_complete_and_unique() {
        return Err(SignatureError::Webview(
            "fixed scenario teardown audit was incomplete or duplicated".into(),
        ));
    }
    events.push("generation-invalidated".into());
    events.extend(audit.ordered_event_names().into_iter().map(str::to_string));
    Ok(())
}

impl RuntimeFixedScenarioDriver<'_> {
    fn fault_profile(
        &self,
        fault: super::signature_host::ScenarioFault,
    ) -> super::signature_host::RawHostProfile {
        self.retry_profile.clone().with_scenario_fault(fault)
    }

    fn append_destroy_audit(
        &self,
        generation: u64,
        operation_id: u64,
        events: &mut Vec<String>,
    ) -> Result<(), SignatureError> {
        if self.runtime.is_active() || super::signature_host::signature_slot_active() {
            return Err(SignatureError::Webview(
                "fixed scenario returned before native teardown acknowledgement".into(),
            ));
        }
        let audit = self
            .runtime
            .take_teardown_audit(generation, operation_id)
            .ok_or_else(|| {
                SignatureError::Webview(
                    "fixed scenario has no generation-scoped teardown audit".into(),
                )
            })?;
        append_verified_teardown_audit(&audit, events)
    }

    async fn retry_after_destroy(&self, events: &mut Vec<String>) -> Result<(), SignatureError> {
        let retry = self
            .runtime
            .initialize_with_profile(self.retry_profile.clone())
            .await?;
        events.push("retry-ready".into());
        if retry.current_url != self.retry_profile.navigation_url {
            return Err(SignatureError::OriginRejected);
        }
        self.runtime.destroy().await?;
        if self.runtime.is_active() || super::signature_host::signature_slot_active() {
            return Err(SignatureError::Webview(
                "fixed scenario retry did not destroy its native actor".into(),
            ));
        }
        events.push("retry-destroyed".into());
        Ok(())
    }

    fn report(
        id: &'static str,
        generation: u64,
        operation_id: u64,
        events: Vec<String>,
    ) -> FixedScenarioReport {
        FixedScenarioReport {
            id,
            generation,
            operation_id,
            ordered_actor_events: events,
            terminal_state: "destroy-confirmed".into(),
        }
    }

    async fn initialization_failure(
        &self,
        id: &'static str,
        fault: super::signature_host::ScenarioFault,
        expected_event: &'static str,
        expect_timeout: bool,
    ) -> Result<FixedScenarioReport, SignatureError> {
        let before = self.runtime.scenario_ids();
        let error = match self
            .runtime
            .initialize_with_profile(self.fault_profile(fault))
            .await
        {
            Err(error) => error,
            Ok(_) => {
                return Err(SignatureError::Webview(
                    "fixed initialization fault unexpectedly succeeded".into(),
                ));
            }
        };
        if expect_timeout && !matches!(error, SignatureError::Timeout) {
            return Err(SignatureError::Webview(
                "fixed initialization delay did not hit the real timeout".into(),
            ));
        }
        if !expect_timeout
            && !error
                .to_string()
                .contains("injected policy registration failure")
        {
            return Err(SignatureError::Webview(
                "fixed policy registration fault was not observed".into(),
            ));
        }
        let audit = self.runtime.take_latest_teardown_audit().ok_or_else(|| {
            SignatureError::Webview("fixed initialization fault has no teardown audit".into())
        })?;
        if audit.generation <= before.0 || audit.operation_id <= before.1 {
            return Err(SignatureError::Webview(
                "fixed initialization fault audit was stale".into(),
            ));
        }
        let mut events = vec!["scenario-started".into(), expected_event.into()];
        if self.runtime.is_active() || super::signature_host::signature_slot_active() {
            return Err(SignatureError::Webview(
                "fixed initialization fault returned before native teardown acknowledgement".into(),
            ));
        }
        append_verified_teardown_audit(&audit, &mut events)?;
        self.retry_after_destroy(&mut events).await?;
        Ok(Self::report(
            id,
            audit.generation,
            audit.operation_id,
            events,
        ))
    }

    async fn sign_timeout(&self, id: &'static str) -> Result<FixedScenarioReport, SignatureError> {
        use crate::music::contract::EncodedComponent;

        let initialized = self
            .runtime
            .initialize_with_profile(
                self.fault_profile(super::signature_host::ScenarioFault::SignCallbackDelay),
            )
            .await?;
        let error = match self
            .runtime
            .sign_text(&EncodedComponent::encode("yinmi-fixed-sign-timeout"))
            .await
        {
            Err(error) => error,
            Ok(_) => {
                return Err(SignatureError::Webview(
                    "fixed sign delay unexpectedly succeeded".into(),
                ));
            }
        };
        if !matches!(error, SignatureError::Timeout) {
            return Err(SignatureError::Webview(
                "fixed sign delay did not hit the real timeout".into(),
            ));
        }
        let mut events = vec!["scenario-started".into(), "sign-timeout-observed".into()];
        self.append_destroy_audit(
            initialized.generation,
            initialized.operation_id,
            &mut events,
        )?;
        self.retry_after_destroy(&mut events).await?;
        Ok(Self::report(
            id,
            initialized.generation,
            initialized.operation_id,
            events,
        ))
    }

    async fn destroy_pending(
        &self,
        id: &'static str,
    ) -> Result<FixedScenarioReport, SignatureError> {
        let profile =
            self.fault_profile(super::signature_host::ScenarioFault::HoldBeforePolicyRegistration);
        let mut initialization = Box::pin(self.runtime.initialize_with_profile(profile));
        tokio::select! {
            stage = self.runtime.wait_scenario_stage(
                super::signature_host::SCENARIO_STAGE_PENDING_POLICY,
            ) => stage?,
            result = &mut initialization => {
                return Err(SignatureError::Webview(format!(
                    "pending-policy scenario completed before destroy: {result:?}"
                )));
            }
        }
        let (generation, operation_id) = self.runtime.scenario_ids();
        let mut events = vec![
            "scenario-started".into(),
            "pending-policy-observed".into(),
            "destroy-requested".into(),
        ];
        self.runtime.destroy().await?;
        if initialization.await.is_ok() {
            return Err(SignatureError::Webview(
                "pending-policy initialization survived destroy".into(),
            ));
        }
        self.append_destroy_audit(generation, operation_id, &mut events)?;
        self.retry_after_destroy(&mut events).await?;
        Ok(Self::report(id, generation, operation_id, events))
    }

    async fn late_callback(&self, id: &'static str) -> Result<FixedScenarioReport, SignatureError> {
        use crate::music::contract::EncodedComponent;

        let initialized = self
            .runtime
            .initialize_with_profile(
                self.fault_profile(super::signature_host::ScenarioFault::SignCallbackDelay),
            )
            .await?;
        let error = match self
            .runtime
            .sign_text(&EncodedComponent::encode("yinmi-fixed-late-callback"))
            .await
        {
            Err(error) => error,
            Ok(_) => {
                return Err(SignatureError::Webview(
                    "fixed late callback unexpectedly succeeded".into(),
                ));
            }
        };
        if !matches!(error, SignatureError::Timeout) {
            return Err(SignatureError::Webview(
                "fixed late callback did not first time out".into(),
            ));
        }
        let mut events = vec!["scenario-started".into(), "sign-timeout-observed".into()];
        self.append_destroy_audit(
            initialized.generation,
            initialized.operation_id,
            &mut events,
        )?;
        let retry = self
            .runtime
            .initialize_with_profile(self.retry_profile.clone())
            .await?;
        if retry.generation <= initialized.generation {
            return Err(SignatureError::StaleCallback);
        }
        events.extend(["retry-ready".into(), "new-generation-ready".into()]);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(6);
        loop {
            if super::signature_host::last_isolated_delayed_callback_generation()
                == initialized.generation
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(SignatureError::Timeout);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        events.push("late-callback-isolated".into());
        self.runtime
            .sign_text(&EncodedComponent::encode(
                "yinmi-new-generation-still-ready",
            ))
            .await?;
        events.push("new-generation-sign-succeeded".into());
        self.runtime.destroy().await?;
        events.push("retry-destroyed".into());
        Ok(Self::report(
            id,
            initialized.generation,
            initialized.operation_id,
            events,
        ))
    }

    async fn main_close_seam(
        &self,
        id: &'static str,
    ) -> Result<FixedScenarioReport, SignatureError> {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };

        let initialized = self
            .runtime
            .initialize_with_profile(self.retry_profile.clone())
            .await?;
        let coordinator = super::signature_webview::SignatureExitCoordinator::new();
        if !coordinator.try_begin() {
            return Err(SignatureError::Webview(
                "main-close coordinator did not arm".into(),
            ));
        }
        let exited = Arc::new(AtomicBool::new(false));
        let blocked_exit = Arc::clone(&exited);
        if super::signature_webview::maybe_complete_teardown(&coordinator, false, move || {
            blocked_exit.store(true, Ordering::Release);
        }) || exited.load(Ordering::Acquire)
        {
            return Err(SignatureError::Webview(
                "main-close coordinator exited before cleanup".into(),
            ));
        }
        let mut events = vec!["scenario-started".into(), "would-exit-blocked".into()];
        self.runtime.destroy().await?;
        self.append_destroy_audit(
            initialized.generation,
            initialized.operation_id,
            &mut events,
        )?;
        if !coordinator.try_begin() {
            return Err(SignatureError::Webview(
                "main-close coordinator did not re-arm".into(),
            ));
        }
        let released_exit = Arc::clone(&exited);
        if !super::signature_webview::maybe_complete_teardown(&coordinator, true, move || {
            released_exit.store(true, Ordering::Release);
        }) || !exited.load(Ordering::Acquire)
        {
            return Err(SignatureError::Webview(
                "main-close coordinator did not release after cleanup".into(),
            ));
        }
        events.push("would-exit-released".into());
        Ok(Self::report(
            id,
            initialized.generation,
            initialized.operation_id,
            events,
        ))
    }
}

impl FixedScenarioDriver for RuntimeFixedScenarioDriver<'_> {
    async fn run_scenario(
        &mut self,
        id: &'static str,
    ) -> Result<FixedScenarioReport, SignatureError> {
        match id {
            "policy-registration-fault" => {
                self.initialization_failure(
                    id,
                    super::signature_host::ScenarioFault::PolicyRegistrationFailure,
                    "policy-registration-fault-observed",
                    false,
                )
                .await
            }
            "initialization-finished-delay-past-20s" => {
                self.initialization_failure(
                    id,
                    super::signature_host::ScenarioFault::InitializationFinishedDelay,
                    "initialization-timeout-observed",
                    true,
                )
                .await
            }
            "sign-callback-delay-past-5s" => self.sign_timeout(id).await,
            "destroy-during-pending-policy" => self.destroy_pending(id).await,
            "late-callback-after-new-generation" => self.late_callback(id).await,
            "main-close-state-machine-seam" => self.main_close_seam(id).await,
            _ => Err(SignatureError::Webview("unknown fixed scenario ID".into())),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSignatureChecks {
    pub raw_wry_host: bool,
    pub tauri_globals_absent: bool,
    pub application_initialization_scripts_absent: bool,
    pub application_ipc_handler_absent: bool,
    pub inert_wry_shim_present: bool,
    pub hidden_ipc_canary_delta_zero: bool,
    pub hidden_ipc_produced_no_response: bool,
    pub app_state_unchanged: bool,
    pub capability_match_absent: bool,
    pub policy_installed_before_first_network_navigation: bool,
    pub official_finished_before_polling: bool,
    pub official_only_origins: bool,
    pub storage_non_persistent: bool,
    pub timeout_check: bool,
    pub retry_check: bool,
    pub policy_fault_invalidates_instance: bool,
    pub late_callback_isolated: bool,
    pub destroy_confirmed_before_retry: bool,
    pub resource_policy_cleanup_acknowledged: bool,
    pub policy_tombstones_empty_before_exit: bool,
    pub lifecycle_no_monotonic_growth: bool,
    pub no_orphan_host_windows: bool,
    pub visible_window_leak_absent: bool,
    pub unexpected_activation_absent: bool,
    pub ordinary_exit_cleanup_acknowledged: bool,
    pub uses_tauri_managed_web_view: bool,
    pub new_instance_storage_recovered: bool,
    pub restart_storage_recovered: bool,
    pub cross_origin_canary_server_hits: u64,
    pub blocked_canary_attempts: Option<u64>,
    pub resource_vector_results: BTreeMap<String, ResourceVectorResult>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolationReport {
    pub generation: u64,
    pub operation_id: u64,
    pub platform_id: String,
    pub host_platform: String,
    pub host_arch: String,
    pub os_version: String,
    pub binary_target_os: String,
    pub binary_target_arch: String,
    pub translated_process: Option<bool>,
    pub webview_runtime_version: String,
    pub runtime_mode: String,
    pub resource_policy_mode: String,
    pub strong_source_kinds_interface_available: Option<bool>,
    pub current_url: String,
    pub final_url: String,
    pub counters: IsolationCounterSnapshot,
    pub host_labels_after_destroy: Vec<String>,
    pub fixed_scenarios: Vec<FixedScenarioReport>,
    pub checks: PlatformSignatureChecks,
}

fn seal_isolation_report<T>(
    report: T,
    assert_cleanup_callbacks_clean: impl FnOnce() -> Result<(), SignatureError>,
) -> Result<T, SignatureError> {
    assert_cleanup_callbacks_clean()?;
    Ok(report)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeObservation {
    status: String,
    tauri_globals_absent: bool,
    inert_wry_shim_present: bool,
    ipc_post_accepted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResponseObservation {
    status: String,
    response_observed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AsyncStorageState {
    status: String,
    done: bool,
    error: bool,
    local_recovered: Option<bool>,
    session_recovered: Option<bool>,
    cookie_recovered: Option<bool>,
    cache_recovered: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserTriggerState {
    status: String,
    done: bool,
    attempted: bool,
    availability_outcome: String,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CanarySnapshot {
    run_id: String,
    mode: String,
    vector: String,
    direct_hits: u64,
    allowed_redirect_hop_hits: u64,
    browser_preflight_hits: u64,
    websocket_handshakes: u64,
    sleep_wake_observed: bool,
    browser_process_baseline: u64,
    browser_process_current: u64,
    visible_window_leak_observed: bool,
    unexpected_activation_observed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanaryCoordinate<'a> {
    run_id: &'a str,
    mode: &'a str,
    vector: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanaryRunCorrelation<'a> {
    run_id: &'a str,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum CanaryCompletionBarrier {
    Pending {
        retry_after_ms: u64,
    },
    Complete {
        retry_after_ms: u64,
        snapshot: CanarySnapshot,
    },
}

fn canary_client() -> Result<reqwest::Client, SignatureError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| SignatureError::Webview("controlled canary client failed".into()))
}

async fn reset_canary(
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
) -> Result<(), SignatureError> {
    let target = config
        .control_origin
        .join("canary/reset")
        .map_err(|_| SignatureError::Webview("controlled canary reset route failed".into()))?;
    let body = serde_json::to_vec(&CanaryCoordinate {
        run_id: &config.run_id,
        mode,
        vector,
    })
    .map_err(|_| SignatureError::Webview("controlled canary reset failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "controlled canary reset exceeded 4 KiB".into(),
        ));
    }
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|_| SignatureError::Webview("controlled canary reset delivery failed".into()))?;
    if response.status() != reqwest::StatusCode::NO_CONTENT {
        return Err(SignatureError::Webview(
            "controlled canary reset was rejected".into(),
        ));
    }
    Ok(())
}

async fn complete_canary_trigger(
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
) -> Result<(), SignatureError> {
    let target = config
        .control_origin
        .join("canary/complete")
        .map_err(|_| SignatureError::Webview("controlled canary completion route failed".into()))?;
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(
            serde_json::to_vec(&CanaryCoordinate {
                run_id: &config.run_id,
                mode,
                vector,
            })
            .map_err(|_| SignatureError::Webview("controlled canary completion failed".into()))?,
        )
        .send()
        .await
        .map_err(|_| {
            SignatureError::Webview("controlled canary completion delivery failed".into())
        })?;
    if response.status() != reqwest::StatusCode::NO_CONTENT {
        return Err(SignatureError::Webview(
            "controlled canary completion was rejected".into(),
        ));
    }
    Ok(())
}

async fn canary_snapshot(
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
) -> Result<CanarySnapshot, SignatureError> {
    let mut target = config
        .control_origin
        .join("canary/snapshot")
        .map_err(|_| SignatureError::Webview("controlled canary snapshot route failed".into()))?;
    target
        .query_pairs_mut()
        .append_pair("runId", &config.run_id)
        .append_pair("mode", mode)
        .append_pair("vector", vector);
    let response = client.get(target).send().await.map_err(|_| {
        SignatureError::Webview("controlled canary snapshot delivery failed".into())
    })?;
    if !response.status().is_success() {
        return Err(SignatureError::Webview(
            "controlled canary snapshot was rejected".into(),
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| SignatureError::Webview("controlled canary snapshot read failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "controlled canary snapshot exceeded 4 KiB".into(),
        ));
    }
    let snapshot: CanarySnapshot = serde_json::from_slice(&body)
        .map_err(|_| SignatureError::Webview("invalid controlled canary snapshot".into()))?;
    if snapshot.run_id != config.run_id || snapshot.mode != mode || snapshot.vector != vector {
        return Err(SignatureError::Webview(
            "controlled canary snapshot correlation failed".into(),
        ));
    }
    Ok(snapshot)
}

async fn wait_for_canary_completion_barrier(
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
    require_hit: bool,
) -> Result<CanarySnapshot, SignatureError> {
    let deadline = tokio::time::Instant::now()
        + super::signature_webview::CALL_TIMEOUT
        + std::time::Duration::from_secs(2);
    loop {
        let mut target = config.control_origin.join("canary/barrier").map_err(|_| {
            SignatureError::Webview("controlled canary completion barrier route failed".into())
        })?;
        target
            .query_pairs_mut()
            .append_pair("runId", &config.run_id)
            .append_pair("mode", mode)
            .append_pair("vector", vector);
        let response = client.get(target).send().await.map_err(|_| {
            SignatureError::Webview("controlled canary completion barrier failed".into())
        })?;
        if !response.status().is_success() {
            return Err(SignatureError::Webview(
                "controlled canary completion barrier was rejected".into(),
            ));
        }
        let body = response.bytes().await.map_err(|_| {
            SignatureError::Webview("controlled canary completion barrier read failed".into())
        })?;
        if body.len() > 4 * 1024 {
            return Err(SignatureError::Webview(
                "controlled canary completion barrier exceeded 4 KiB".into(),
            ));
        }
        let barrier: CanaryCompletionBarrier = serde_json::from_slice(&body).map_err(|_| {
            SignatureError::Webview("invalid controlled canary completion barrier".into())
        })?;
        match barrier {
            CanaryCompletionBarrier::Complete {
                retry_after_ms,
                snapshot,
            } => {
                if retry_after_ms != 0
                    || snapshot.run_id != config.run_id
                    || snapshot.mode != mode
                    || snapshot.vector != vector
                {
                    return Err(SignatureError::Webview(
                        "controlled canary completion barrier correlation failed".into(),
                    ));
                }
                if require_hit && snapshot.direct_hits == 0 && snapshot.browser_preflight_hits == 0
                {
                    return Err(SignatureError::Webview(
                        "controlled canary expected hit was not observed before completion".into(),
                    ));
                }
                return Ok(snapshot);
            }
            CanaryCompletionBarrier::Pending { retry_after_ms } => {
                if retry_after_ms == 0 || retry_after_ms > 5_000 {
                    return Err(SignatureError::Webview(
                        "controlled canary completion retry was invalid".into(),
                    ));
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(SignatureError::Webview(
                        "controlled canary completion silence timed out".into(),
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(retry_after_ms.min(250))).await;
            }
        }
    }
}

async fn seal_protected_canary(
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
) -> Result<(), SignatureError> {
    let target = config
        .control_origin
        .join("canary/protected-seal")
        .map_err(|_| SignatureError::Webview("protected canary seal route failed".into()))?;
    let response = client
        .post(target)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(
            serde_json::to_vec(&CanaryRunCorrelation {
                run_id: &config.run_id,
            })
            .map_err(|_| SignatureError::Webview("protected canary seal body failed".into()))?,
        )
        .send()
        .await
        .map_err(|_| SignatureError::Webview("protected canary seal delivery failed".into()))?;
    if response.status() != reqwest::StatusCode::NO_CONTENT {
        return Err(SignatureError::Webview(
            "protected canary zero-hit seal was rejected".into(),
        ));
    }
    Ok(())
}

async fn verify_protected_canary_seal(
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
) -> Result<(), SignatureError> {
    let mut target = config
        .control_origin
        .join("canary/protected-verify")
        .map_err(|_| SignatureError::Webview("protected canary verify route failed".into()))?;
    target
        .query_pairs_mut()
        .append_pair("runId", &config.run_id);
    let response =
        client.get(target).send().await.map_err(|_| {
            SignatureError::Webview("protected canary verify delivery failed".into())
        })?;
    if !response.status().is_success() {
        return Err(SignatureError::Webview(
            "protected canary zero-hit verification was rejected".into(),
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| SignatureError::Webview("protected canary verify body failed".into()))?;
    if body.len() > 4 * 1024 {
        return Err(SignatureError::Webview(
            "protected canary verify body exceeded 4 KiB".into(),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| SignatureError::Webview("protected canary verify body failed".into()))?;
    if value != serde_json::json!({ "verified": true }) {
        return Err(SignatureError::Webview(
            "protected canary zero-hit verification failed".into(),
        ));
    }
    Ok(())
}

fn parse_probe_evaluation<T: DeserializeOwned>(raw: &str) -> Result<T, SignatureError> {
    serde_json::from_str(raw).map_err(|_| SignatureError::Evaluation)
}

fn bridge_start_script(payload: &str) -> String {
    let payload = serde_json::to_string(payload).expect("probe payload serializes");
    format!(
        r#"(() => {{
  try {{
    const names = Object.getOwnPropertyNames(globalThis);
    const tauriGlobalsAbsent =
      !("__TAURI__" in globalThis) &&
      !("__TAURI_INTERNALS__" in globalThis) &&
      !("isTauri" in globalThis) &&
      !names.some((name) => name.toLowerCase().includes("tauri"));
    const shim = globalThis.ipc;
    const inertWryShimPresent = !!shim && typeof shim.postMessage === "function";
    const state = {{ responseObserved: false }};
    state.onMessage = () => {{ state.responseObserved = true; }};
    globalThis.addEventListener("message", state.onMessage);
    globalThis.__yinmiRawWryProbe = state;
    let ipcPostAccepted = false;
    if (inertWryShimPresent) {{
      shim.postMessage({payload});
      ipcPostAccepted = true;
    }}
    return {{ status: "ok", tauriGlobalsAbsent, inertWryShimPresent, ipcPostAccepted }};
  }} catch (_) {{
    return {{ status: "error", tauriGlobalsAbsent: false, inertWryShimPresent: false, ipcPostAccepted: false }};
  }}
}})()"#
    )
}

fn bridge_finish_script() -> String {
    r#"(() => {
  try {
    const state = globalThis.__yinmiRawWryProbe;
    const responseObserved = !state || state.responseObserved === true;
    if (state?.onMessage) globalThis.removeEventListener("message", state.onMessage);
    delete globalThis.__yinmiRawWryProbe;
    return { status: "ok", responseObserved };
  } catch (_) {
    return { status: "error", responseObserved: true };
  }
})()"#
        .into()
}

fn storage_write_start_script(key: &str, value: &str) -> String {
    let key = serde_json::to_string(key).expect("probe key serializes");
    let value = serde_json::to_string(value).expect("probe value serializes");
    format!(
        r#"(() => {{
  const state = {{ done: false, error: false }};
  globalThis.__yinmiStorageProbe = state;
  (async () => {{
    localStorage.setItem({key}, {value});
    sessionStorage.setItem({key}, {value});
    document.cookie = `${{{key}}}=${{encodeURIComponent({value})}}; SameSite=Strict; path=/`;
    if (!("caches" in globalThis)) throw new Error("cache-api-absent");
    const cache = await caches.open({key});
    await cache.put(new Request(`/${{{key}}}`), new Response({value}));
    state.done = true;
  }})().catch(() => {{ state.error = true; state.done = true; }});
  return {{ status: "ok", done: false, error: false }};
}})()"#
    )
}

fn storage_read_start_script(key: &str, value: &str) -> String {
    let key = serde_json::to_string(key).expect("probe key serializes");
    let value = serde_json::to_string(value).expect("probe value serializes");
    format!(
        r#"(() => {{
  const state = {{ done: false, error: false }};
  globalThis.__yinmiStorageProbe = state;
  (async () => {{
    state.localRecovered = localStorage.getItem({key}) === {value};
    state.sessionRecovered = sessionStorage.getItem({key}) === {value};
    state.cookieRecovered = document.cookie.split("; ").some((item) => item.startsWith(`${{{key}}}=`));
    if (!("caches" in globalThis)) throw new Error("cache-api-absent");
    const response = await caches.match(new Request(`/${{{key}}}`));
    state.cacheRecovered = response ? (await response.text()) === {value} : false;
    state.done = true;
  }})().catch(() => {{ state.error = true; state.done = true; }});
  return {{ status: "ok", done: false, error: false }};
}})()"#
    )
}

fn storage_state_script() -> String {
    r#"(() => {
  try {
    const state = globalThis.__yinmiStorageProbe;
    if (!state) return { status: "error", done: true, error: true };
    return {
      status: "ok",
      done: state.done === true,
      error: state.error === true,
      localRecovered: state.localRecovered,
      sessionRecovered: state.sessionRecovered,
      cookieRecovered: state.cookieRecovered,
      cacheRecovered: state.cacheRecovered
    };
  } catch (_) {
    return { status: "error", done: true, error: true };
  }
})()"#
        .into()
}

fn tagged_canary_url(
    origin: &Url,
    path: &str,
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
) -> Result<String, SignatureError> {
    let mut url = origin
        .join(path)
        .map_err(|_| SignatureError::Webview("controlled canary URL failed".into()))?;
    url.query_pairs_mut()
        .append_pair("runId", &config.run_id)
        .append_pair("mode", mode)
        .append_pair("vector", vector);
    Ok(url.into())
}

fn browser_trigger_start_script(
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
) -> Result<String, SignatureError> {
    if !RESOURCE_VECTORS.contains(&vector) {
        return Err(SignatureError::Webview(
            "controlled canary vector is not frozen".into(),
        ));
    }
    let blocked_path = format!("blocked/{vector}");
    let sse_path = format!("sse/{vector}");
    let ws_path = format!("ws/{vector}");
    let input = serde_json::json!({
        "vector": vector,
        "blockedUrl": tagged_canary_url(
            &config.blocked_https_origin,
            &blocked_path,
            config,
            mode,
            vector,
        )?,
        "blockedHttpUrl": tagged_canary_url(
            &config.blocked_http_origin,
            &blocked_path,
            config,
            mode,
            vector,
        )?,
        "blockedWsUrl": tagged_canary_url(
            &config.blocked_ws_origin,
            &ws_path,
            config,
            mode,
            vector,
        )?,
        "blockedWssUrl": tagged_canary_url(
            &config.blocked_wss_origin,
            &ws_path,
            config,
            mode,
            vector,
        )?,
        "blockedSseUrl": tagged_canary_url(
            &config.blocked_https_origin,
            &sse_path,
            config,
            mode,
            vector,
        )?,
        "redirectUrl": tagged_canary_url(
            &config.allowed_origin,
            "redirect/one",
            config,
            mode,
            vector,
        )?,
        "serviceWorkerUrl": tagged_canary_url(
            &config.allowed_origin,
            "sw.js",
            config,
            mode,
            vector,
        )?,
    });
    const SCRIPT: &str = r#"(() => {
  const config = CANARY_INPUT_JSON;
  const state = {
    status: "ok",
    done: false,
    attempted: false,
    availabilityOutcome: "available",
    error: null
  };
  globalThis.__yinmiCanaryVector = state;
  const settle = (promise, milliseconds = 750) => Promise.race([
    Promise.resolve(promise).catch(() => undefined),
    new Promise((resolve) => setTimeout(resolve, milliseconds))
  ]);
  const waitForElement = (element) => settle(new Promise((resolve) => {
    element.addEventListener("load", resolve, { once: true });
    element.addEventListener("error", resolve, { once: true });
  }));
  (async () => {
    state.attempted = true;
    switch (config.vector) {
      case "document": {
        const element = document.createElement("object");
        element.type = "text/html";
        element.data = config.blockedUrl;
        document.body.append(element);
        await waitForElement(element);
        element.remove();
        break;
      }
      case "iframe": {
        const element = document.createElement("iframe");
        element.src = config.blockedUrl;
        document.body.append(element);
        await waitForElement(element);
        element.remove();
        break;
      }
      case "script": {
        const element = document.createElement("script");
        element.src = config.blockedUrl;
        document.body.append(element);
        await waitForElement(element);
        element.remove();
        break;
      }
      case "style": {
        const element = document.createElement("link");
        element.rel = "stylesheet";
        element.href = config.blockedUrl;
        document.head.append(element);
        await waitForElement(element);
        element.remove();
        break;
      }
      case "image": {
        const element = new Image();
        element.src = config.blockedUrl;
        await waitForElement(element);
        break;
      }
      case "media": {
        const element = document.createElement("audio");
        element.preload = "auto";
        element.src = config.blockedUrl;
        document.body.append(element);
        element.load();
        await settle(new Promise((resolve) => {
          element.addEventListener("loadeddata", resolve, { once: true });
          element.addEventListener("error", resolve, { once: true });
        }));
        element.remove();
        break;
      }
      case "fetch":
        await settle(fetch(config.blockedUrl, { mode: "no-cors", cache: "no-store" }));
        break;
      case "xhr":
        await settle(new Promise((resolve) => {
          const request = new XMLHttpRequest();
          request.open("GET", config.blockedUrl);
          request.onloadend = resolve;
          request.onerror = resolve;
          request.send();
        }));
        break;
      case "worker": {
        const source = `importScripts(${JSON.stringify(config.blockedUrl)});`;
        const objectUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
        const worker = new Worker(objectUrl);
        await settle(new Promise((resolve) => {
          worker.onmessage = resolve;
          worker.onerror = resolve;
        }));
        worker.terminate();
        URL.revokeObjectURL(objectUrl);
        break;
      }
      case "service_worker": {
        if (!("serviceWorker" in navigator)) {
          state.availabilityOutcome = "service-worker-api-absent";
          break;
        }
        try {
          const registration = await navigator.serviceWorker.register(
            config.serviceWorkerUrl,
            { scope: "/yinmi-canary-service-worker/" }
          );
          await registration.update();
          const worker =
            registration.installing || registration.waiting || registration.active;
          if (worker && worker.state !== "installed" && worker.state !== "activated") {
            await new Promise((resolve, reject) => {
              const timer = setTimeout(
                () => reject(new Error("service-worker-terminal-timeout")),
                3000
              );
              worker.addEventListener("statechange", () => {
                if (worker.state === "installed" || worker.state === "activated") {
                  clearTimeout(timer);
                  resolve();
                } else if (worker.state === "redundant") {
                  clearTimeout(timer);
                  reject(new Error("service-worker-became-redundant"));
                }
              });
            });
          }
          if (!(await registration.unregister())) {
            throw new Error("service-worker-unregister-failed");
          }
        } catch (_) {
          state.error = "service-worker-registration-rejected";
        }
        break;
      }
      case "websocket": {
        const socket = new WebSocket(config.blockedWssUrl);
        await settle(new Promise((resolve) => {
          socket.onopen = resolve;
          socket.onerror = resolve;
          socket.onclose = resolve;
        }));
        try { socket.close(); } catch (_) {}
        break;
      }
      case "sse": {
        const source = new EventSource(config.blockedSseUrl);
        await settle(new Promise((resolve) => {
          source.onopen = resolve;
          source.onerror = resolve;
        }));
        source.close();
        break;
      }
      case "beacon":
        navigator.sendBeacon(config.blockedUrl, new Uint8Array([1]));
        await new Promise((resolve) => setTimeout(resolve, 250));
        break;
      case "redirect":
        await settle(fetch(config.redirectUrl, { mode: "no-cors", cache: "no-store" }));
        break;
      case "popup": {
        const popup = window.open(config.blockedUrl, "_blank", "noopener");
        try { popup?.close(); } catch (_) {}
        break;
      }
      case "download": {
        const link = document.createElement("a");
        link.download = "yinmi-canary";
        link.href = config.blockedUrl;
        document.body.append(link);
        link.click();
        link.remove();
        break;
      }
      case "top_level_data":
        setTimeout(() => location.assign("data:text/html,yinmi-probe"), 0);
        break;
      case "top_level_blob": {
        const objectUrl = URL.createObjectURL(new Blob(["yinmi-probe"], { type: "text/html" }));
        setTimeout(() => {
          location.assign(objectUrl);
          setTimeout(() => URL.revokeObjectURL(objectUrl), 0);
        }, 0);
        break;
      }
      case "top_level_file":
        setTimeout(() => location.assign("file:///yinmi-feasibility-denied"), 0);
        break;
      case "top_level_custom_protocol":
        setTimeout(() => location.assign("yinmi-feasibility-denied://probe"), 0);
        break;
      default:
        throw new Error("unknown frozen vector");
    }
  })().catch(() => {
    state.status = "error";
  }).finally(() => {
    state.done = true;
  });
  return { status: "ok", done: false, attempted: false, availabilityOutcome: "available" };
})()"#;
    Ok(SCRIPT.replacen("CANARY_INPUT_JSON", &input.to_string(), 1))
}

fn browser_trigger_state_script() -> String {
    r#"(() => {
  const state = globalThis.__yinmiCanaryVector;
  if (!state) return { status: "error", done: true, attempted: false, availabilityOutcome: "probe-error", error: "probe-state-missing" };
  return {
    status: state.status,
    done: state.done === true,
    attempted: state.attempted === true,
    availabilityOutcome: state.availabilityOutcome,
    error: state.error
  };
})()"#
        .into()
}

async fn run_browser_trigger(
    runtime: &super::signature_webview::SignatureRuntime,
    config: &ControlledCanaryConfig,
    mode: &str,
    vector: &str,
) -> Result<BrowserTriggerState, SignatureError> {
    runtime
        .evaluate_probe_script(browser_trigger_start_script(config, mode, vector)?)
        .await?;
    let deadline = tokio::time::Instant::now() + super::signature_webview::CALL_TIMEOUT;
    loop {
        let raw = runtime
            .evaluate_probe_script(browser_trigger_state_script())
            .await?;
        let state: BrowserTriggerState = parse_probe_evaluation(&raw)?;
        if state.done {
            if !state.attempted || state.status != "ok" || state.error.is_some() {
                return Err(SignatureError::Webview(format!(
                    "controlled canary vector {vector} did not execute"
                )));
            }
            if state.availability_outcome != "available"
                && !(vector == "service_worker"
                    && state.availability_outcome == "service-worker-api-absent")
            {
                return Err(SignatureError::Webview(format!(
                    "controlled canary vector {vector} had an invalid availability outcome"
                )));
            }
            return Ok(state);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SignatureError::Timeout);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn run_browser_preflight(
    runtime: &super::signature_webview::SignatureRuntime,
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
) -> Result<(), SignatureError> {
    reset_canary(client, config, "preflight", "preflight").await?;
    let https_url = tagged_canary_url(
        &config.blocked_https_origin,
        "preflight",
        config,
        "preflight",
        "preflight",
    )?;
    let wss_url = tagged_canary_url(
        &config.blocked_wss_origin,
        "ws/preflight",
        config,
        "preflight",
        "preflight",
    )?;
    let input = serde_json::json!({ "httpsUrl": https_url, "wssUrl": wss_url });
    let script = format!(
        r#"(() => {{
  const input = {input};
  const state = {{ done: false }};
  globalThis.__yinmiCanaryPreflight = state;
  (async () => {{
    await fetch(input.httpsUrl, {{ mode: "no-cors", cache: "no-store" }});
    await new Promise((resolve, reject) => {{
      const socket = new WebSocket(input.wssUrl);
      const timer = setTimeout(() => {{ socket.close(); reject(new Error("wss-timeout")); }}, 2000);
      socket.onopen = () => {{ clearTimeout(timer); socket.close(); resolve(); }};
      socket.onerror = () => {{ clearTimeout(timer); reject(new Error("wss-error")); }};
    }});
    state.done = true;
  }})().catch(() => {{ state.done = true; state.error = true; }});
  return {{ status: "ok" }};
}})()"#,
        input = input
    );
    runtime.evaluate_probe_script(script).await?;
    let deadline = tokio::time::Instant::now() + super::signature_webview::CALL_TIMEOUT;
    loop {
        let raw = runtime
            .evaluate_probe_script(
                r#"(() => ({
  status: "ok",
  done: globalThis.__yinmiCanaryPreflight?.done === true,
  error: globalThis.__yinmiCanaryPreflight?.error === true
}))()"#
                    .into(),
            )
            .await?;
        #[derive(Deserialize)]
        struct PreflightState {
            status: String,
            done: bool,
            error: bool,
        }
        let state: PreflightState = parse_probe_evaluation(&raw)?;
        if state.done {
            if state.status != "ok" || state.error {
                return Err(SignatureError::Webview(
                    "controlled canary browser preflight failed".into(),
                ));
            }
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SignatureError::Timeout);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    complete_canary_trigger(client, config, "preflight", "preflight").await?;
    let snapshot =
        wait_for_canary_completion_barrier(client, config, "preflight", "preflight", true).await?;
    if snapshot.browser_preflight_hits == 0 || snapshot.websocket_handshakes == 0 {
        return Err(SignatureError::Webview(
            "controlled canary preflight did not prove HTTPS and WSS reachability".into(),
        ));
    }
    Ok(())
}

async fn wait_for_storage_state(
    runtime: &super::signature_webview::SignatureRuntime,
) -> Result<AsyncStorageState, SignatureError> {
    let deadline = tokio::time::Instant::now() + super::signature_webview::CALL_TIMEOUT;
    loop {
        let raw = runtime
            .evaluate_probe_script(storage_state_script())
            .await?;
        let state: AsyncStorageState = parse_probe_evaluation(&raw)?;
        if state.status != "ok" || state.error {
            return Err(SignatureError::Webview(
                "browser persistence probe failed".into(),
            ));
        }
        if state.done {
            return Ok(state);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SignatureError::Timeout);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn expected_barrier(vector: &str) -> &'static str {
    match vector {
        "popup" => "new-window-handler",
        "download" => "download-handler",
        "top_level_data" | "top_level_blob" | "top_level_file" | "top_level_custom_protocol" => {
            "navigation-handler"
        }
        #[cfg(windows)]
        _ => "webview2-web-resource-requested",
        #[cfg(target_os = "macos")]
        _ => "wk-content-rule-list",
        #[cfg(not(any(windows, target_os = "macos")))]
        _ => "unsupported-platform-resource-policy",
    }
}

fn counter_delta(after: u64, before: u64) -> Result<u64, SignatureError> {
    after
        .checked_sub(before)
        .ok_or_else(|| SignatureError::Webview("controlled canary native counter regressed".into()))
}

fn derive_resource_row(
    vector: &str,
    trigger: &BrowserTriggerState,
    counterfactual: Option<&CanarySnapshot>,
    protected: &CanarySnapshot,
    before: IsolationCounterSnapshot,
    after: IsolationCounterSnapshot,
) -> Result<ResourceVectorResult, SignatureError> {
    if !trigger.attempted || trigger.status != "ok" || trigger.error.is_some() {
        return Err(SignatureError::Webview(format!(
            "controlled canary vector {vector} was not attempted"
        )));
    }
    if protected.direct_hits != 0 {
        return Err(SignatureError::Webview(format!(
            "controlled canary vector {vector} reached the blocked server"
        )));
    }
    let redirect_hops = if vector == "redirect" { 2 } else { 0 };
    if protected.allowed_redirect_hop_hits != redirect_hops {
        return Err(SignatureError::Webview(format!(
            "controlled canary vector {vector} had the wrong redirect-hop count"
        )));
    }
    let absent_service_worker =
        vector == "service_worker" && trigger.availability_outcome == "service-worker-api-absent";
    let resource_request = RESOURCE_VECTORS[..14].contains(&vector);
    let (evidence_mode, counterfactual_server_hits) = if absent_service_worker {
        ("deterministic-seam-only", None)
    } else if resource_request {
        let counterfactual = counterfactual.ok_or_else(|| {
            SignatureError::Webview(format!(
                "controlled canary vector {vector} is missing its counterfactual"
            ))
        })?;
        if counterfactual.direct_hits == 0 {
            return Err(SignatureError::Webview(format!(
                "controlled canary vector {vector} counterfactual did not reach the server"
            )));
        }
        #[cfg(windows)]
        {
            if counter_delta(after.resource_canary_hits, before.resource_canary_hits)? == 0 {
                return Err(SignatureError::Webview(format!(
                    "controlled canary vector {vector} produced no WebView2 callback"
                )));
            }
            ("native-callback", None)
        }
        #[cfg(target_os = "macos")]
        {
            ("paired-counterfactual", Some(counterfactual.direct_hits))
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            ("paired-counterfactual", Some(counterfactual.direct_hits))
        }
    } else {
        let callback_delta = match vector {
            "popup" => counter_delta(after.blocked_new_windows, before.blocked_new_windows)?,
            "download" => counter_delta(after.blocked_downloads, before.blocked_downloads)?,
            "top_level_data"
            | "top_level_blob"
            | "top_level_file"
            | "top_level_custom_protocol" => {
                counter_delta(after.blocked_navigations, before.blocked_navigations)?
            }
            _ => 0,
        };
        if callback_delta == 0 {
            return Err(SignatureError::Webview(format!(
                "controlled canary vector {vector} produced no handler callback"
            )));
        }
        ("handler-callback", None)
    };
    let barrier = expected_barrier(vector);
    Ok(ResourceVectorResult {
        runtime_attempted: true,
        availability_outcome: trigger.availability_outcome.clone(),
        deterministic_barrier_seam_covered: true,
        expected_barrier: barrier.into(),
        enforced_barrier: barrier.into(),
        barrier_evidence_mode: evidence_mode.into(),
        counterfactual_server_hits,
        allowed_redirect_hop_hits: protected.allowed_redirect_hop_hits,
        server_hits: protected.direct_hits,
    })
}

trait ResourceMatrixDriver {
    async fn destroy(&mut self) -> Result<(), SignatureError>;
    async fn initialize(
        &mut self,
        profile: super::signature_host::RawHostProfile,
    ) -> Result<super::signature_webview::SignatureInitReport, SignatureError>;
    async fn preflight(&mut self) -> Result<(), SignatureError>;
    async fn reset(&mut self, mode: &str, vector: &str) -> Result<(), SignatureError>;
    async fn trigger(
        &mut self,
        mode: &str,
        vector: &str,
    ) -> Result<BrowserTriggerState, SignatureError>;
    async fn complete(&mut self, mode: &str, vector: &str) -> Result<(), SignatureError>;
    async fn observation(
        &mut self,
        mode: &str,
        vector: &str,
        wait_for_direct_hit: bool,
    ) -> Result<CanarySnapshot, SignatureError>;
    async fn host_snapshot(
        &mut self,
    ) -> Result<super::signature_host::RawHostProbeSnapshot, SignatureError>;
    async fn seal_protected(&mut self) -> Result<(), SignatureError>;
    async fn verify_protected_seal(&mut self) -> Result<(), SignatureError>;
}

struct LiveResourceMatrixDriver<'a> {
    runtime: &'a super::signature_webview::SignatureRuntime,
    client: &'a reqwest::Client,
    config: &'a ControlledCanaryConfig,
}

impl ResourceMatrixDriver for LiveResourceMatrixDriver<'_> {
    async fn destroy(&mut self) -> Result<(), SignatureError> {
        self.runtime.destroy().await
    }

    async fn initialize(
        &mut self,
        profile: super::signature_host::RawHostProfile,
    ) -> Result<super::signature_webview::SignatureInitReport, SignatureError> {
        self.runtime.initialize_with_profile(profile).await
    }

    async fn preflight(&mut self) -> Result<(), SignatureError> {
        run_browser_preflight(self.runtime, self.client, self.config).await
    }

    async fn reset(&mut self, mode: &str, vector: &str) -> Result<(), SignatureError> {
        reset_canary(self.client, self.config, mode, vector).await
    }

    async fn trigger(
        &mut self,
        mode: &str,
        vector: &str,
    ) -> Result<BrowserTriggerState, SignatureError> {
        run_browser_trigger(self.runtime, self.config, mode, vector).await
    }

    async fn complete(&mut self, mode: &str, vector: &str) -> Result<(), SignatureError> {
        complete_canary_trigger(self.client, self.config, mode, vector).await
    }

    async fn observation(
        &mut self,
        mode: &str,
        vector: &str,
        wait_for_direct_hit: bool,
    ) -> Result<CanarySnapshot, SignatureError> {
        wait_for_canary_completion_barrier(
            self.client,
            self.config,
            mode,
            vector,
            wait_for_direct_hit,
        )
        .await
    }

    async fn host_snapshot(
        &mut self,
    ) -> Result<super::signature_host::RawHostProbeSnapshot, SignatureError> {
        self.runtime.probe_host_snapshot().await
    }

    async fn seal_protected(&mut self) -> Result<(), SignatureError> {
        seal_protected_canary(self.client, self.config).await
    }

    async fn verify_protected_seal(&mut self) -> Result<(), SignatureError> {
        verify_protected_canary_seal(self.client, self.config).await
    }
}

async fn collect_resource_matrix<D: ResourceMatrixDriver>(
    driver: &mut D,
    counterfactual_profile: super::signature_host::RawHostProfile,
    protected_profile: super::signature_host::RawHostProfile,
) -> Result<
    (
        super::signature_webview::SignatureInitReport,
        super::signature_host::RawHostProbeSnapshot,
        BTreeMap<String, ResourceVectorResult>,
    ),
    SignatureError,
> {
    driver.destroy().await?;
    let result = async {
        driver.initialize(counterfactual_profile).await?;
        driver.preflight().await?;
        let mut counterfactuals: BTreeMap<String, (BrowserTriggerState, CanarySnapshot)> =
            BTreeMap::new();
        for vector in RESOURCE_VECTORS[..14].iter().copied() {
            driver.reset("counterfactual", vector).await?;
            let trigger = driver.trigger("counterfactual", vector).await?;
            if !trigger.attempted || trigger.status != "ok" || trigger.error.is_some() {
                return Err(SignatureError::Webview(format!(
                    "counterfactual vector {vector} did not execute"
                )));
            }
            let absent = vector == "service_worker"
                && trigger.availability_outcome == "service-worker-api-absent";
            driver.complete("counterfactual", vector).await?;
            let snapshot = driver
                .observation("counterfactual", vector, !absent)
                .await?;
            if !absent && snapshot.direct_hits == 0 {
                return Err(SignatureError::Webview(format!(
                    "counterfactual vector {vector} did not reach the blocked server"
                )));
            }
            if vector == "redirect" && snapshot.allowed_redirect_hop_hits != 2 {
                return Err(SignatureError::Webview(
                    "counterfactual redirect did not traverse both allowed hops".into(),
                ));
            }
            counterfactuals.insert(vector.into(), (trigger, snapshot));
        }
        driver.destroy().await?;

        let initialization = driver.initialize(protected_profile).await?;
        let mut rows = BTreeMap::new();
        for vector in RESOURCE_VECTORS {
            driver.reset("protected", vector).await?;
            let before = driver.host_snapshot().await?.counters;
            let trigger = driver.trigger("protected", vector).await?;
            if let Some((counterfactual_trigger, _)) = counterfactuals.get(vector)
                && counterfactual_trigger.availability_outcome != trigger.availability_outcome
            {
                return Err(SignatureError::Webview(format!(
                    "controlled canary vector {vector} availability changed between profiles"
                )));
            }
            driver.complete("protected", vector).await?;
            let protected = driver.observation("protected", vector, false).await?;
            let after = driver.host_snapshot().await?.counters;
            let counterfactual = counterfactuals.get(vector).map(|(_, snapshot)| snapshot);
            rows.insert(
                vector.into(),
                derive_resource_row(vector, &trigger, counterfactual, &protected, before, after)?,
            );
        }
        let snapshot = driver.host_snapshot().await?;
        driver.seal_protected().await?;
        driver.destroy().await?;
        driver.verify_protected_seal().await?;
        Ok((initialization, snapshot, rows))
    }
    .await;
    if result.is_err() {
        let _ = driver.destroy().await;
    }
    result
}

async fn collect_controlled_resource_matrix(
    runtime: &super::signature_webview::SignatureRuntime,
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
    counterfactual_profile: super::signature_host::RawHostProfile,
    protected_profile: super::signature_host::RawHostProfile,
) -> Result<
    (
        super::signature_webview::SignatureInitReport,
        super::signature_host::RawHostProbeSnapshot,
        BTreeMap<String, ResourceVectorResult>,
    ),
    SignatureError,
> {
    let mut driver = LiveResourceMatrixDriver {
        runtime,
        client,
        config,
    };
    collect_resource_matrix(&mut driver, counterfactual_profile, protected_profile).await
}

trait LifecycleStressDriver {
    async fn reset(&mut self) -> Result<(), SignatureError>;
    async fn initialize(
        &mut self,
    ) -> Result<super::signature_webview::SignatureInitReport, SignatureError>;
    async fn sign(&mut self) -> Result<(), SignatureError>;
    async fn host_snapshot(
        &mut self,
    ) -> Result<super::signature_host::RawHostProbeSnapshot, SignatureError>;
    async fn destroy(&mut self) -> Result<(), SignatureError>;
    async fn invariant_snapshot(&mut self) -> Result<LifecycleInvariantSnapshot, SignatureError>;
}

#[derive(Clone, Debug)]
struct LifecycleInvariantSnapshot {
    browser_process_baseline: u64,
    browser_process_current: u64,
    visible_window_leak_observed: bool,
    unexpected_activation_observed: bool,
    sleep_wake_observed: bool,
    slot_active: bool,
    host_windows_absent: bool,
    policy_store: super::webview_resource_policy::PolicyStoreSnapshot,
}

fn validate_lifecycle_invariant_sample(
    baseline: &LifecycleInvariantSnapshot,
    sample: &LifecycleInvariantSnapshot,
    require_sleep_wake: bool,
) -> Result<(), SignatureError> {
    if baseline.policy_store.has_signature_residue()
        || sample.policy_store.has_signature_residue()
        || sample.policy_store != baseline.policy_store
    {
        return Err(SignatureError::Webview(
            "lifecycle policy-store identifiers or tombstones did not return to baseline".into(),
        ));
    }
    if sample.browser_process_baseline != baseline.browser_process_baseline
        || sample.browser_process_current > baseline.browser_process_current
    {
        return Err(SignatureError::Webview(
            "lifecycle browser process count grew above the pre-probe baseline".into(),
        ));
    }
    if sample.slot_active || !sample.host_windows_absent {
        return Err(SignatureError::Webview(
            "lifecycle destroy left an actor slot or manager host window".into(),
        ));
    }
    if sample.visible_window_leak_observed || sample.unexpected_activation_observed {
        return Err(SignatureError::Webview(
            "lifecycle platform monitor observed a visible window or activation leak".into(),
        ));
    }
    if require_sleep_wake && !sample.sleep_wake_observed {
        return Err(SignatureError::Webview(
            "lifecycle platform monitor did not observe an OS suspend/resume pair".into(),
        ));
    }
    Ok(())
}

trait LifecycleClock {
    async fn sleep(&mut self, duration: std::time::Duration);
}

struct RealLifecycleClock;

impl LifecycleClock for RealLifecycleClock {
    async fn sleep(&mut self, duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }
}

struct LiveLifecycleStressDriver<'a> {
    runtime: &'a super::signature_webview::SignatureRuntime,
    client: &'a reqwest::Client,
    config: &'a ControlledCanaryConfig,
    profile: super::signature_host::RawHostProfile,
}

impl LifecycleStressDriver for LiveLifecycleStressDriver<'_> {
    async fn reset(&mut self) -> Result<(), SignatureError> {
        reset_canary(self.client, self.config, "lifecycle", "lifecycle").await
    }

    async fn initialize(
        &mut self,
    ) -> Result<super::signature_webview::SignatureInitReport, SignatureError> {
        self.runtime
            .initialize_with_profile(self.profile.clone())
            .await
    }

    async fn sign(&mut self) -> Result<(), SignatureError> {
        use crate::music::contract::EncodedComponent;

        self.runtime
            .sign_text(&EncodedComponent::encode("yinmi-lifecycle-cycle"))
            .await
            .map(|_| ())
    }

    async fn host_snapshot(
        &mut self,
    ) -> Result<super::signature_host::RawHostProbeSnapshot, SignatureError> {
        self.runtime.probe_host_snapshot().await
    }

    async fn destroy(&mut self) -> Result<(), SignatureError> {
        self.runtime.destroy().await
    }

    async fn invariant_snapshot(&mut self) -> Result<LifecycleInvariantSnapshot, SignatureError> {
        let observation =
            canary_snapshot(self.client, self.config, "lifecycle", "lifecycle").await?;
        let policy_store =
            super::webview_resource_policy::policy_store_snapshot(self.runtime.app_handle())
                .await?;
        Ok(LifecycleInvariantSnapshot {
            browser_process_baseline: observation.browser_process_baseline,
            browser_process_current: observation.browser_process_current,
            visible_window_leak_observed: observation.visible_window_leak_observed,
            unexpected_activation_observed: observation.unexpected_activation_observed,
            sleep_wake_observed: observation.sleep_wake_observed,
            slot_active: super::signature_host::signature_slot_active(),
            host_windows_absent: self.runtime.app_handle().windows().keys().all(|label| {
                !label.starts_with(super::signature_webview::SIGNATURE_HOST_WINDOW_LABEL)
            }),
            policy_store,
        })
    }
}

async fn execute_lifecycle_stress<D: LifecycleStressDriver, C: LifecycleClock>(
    driver: &mut D,
    clock: &mut C,
    idle_duration: std::time::Duration,
) -> Result<(bool, bool, bool, bool, bool), SignatureError> {
    use std::collections::BTreeSet;

    driver.reset().await?;
    let baseline = driver.invariant_snapshot().await?;
    validate_lifecycle_invariant_sample(&baseline, &baseline, false)?;
    let mut labels = BTreeSet::new();
    for _ in 0..LIFECYCLE_CYCLE_COUNT {
        let initialized = driver.initialize().await?;
        if !labels.insert(initialized.host_label.clone()) {
            return Err(SignatureError::Webview(
                "lifecycle probe reused a native host label".into(),
            ));
        }
        driver.sign().await?;
        let snapshot = driver.host_snapshot().await?;
        if !snapshot.managed_webviews_empty {
            return Err(SignatureError::Webview(
                "lifecycle probe found a managed WebView".into(),
            ));
        }
        driver.destroy().await?;
        let round = driver.invariant_snapshot().await?;
        validate_lifecycle_invariant_sample(&baseline, &round, false)?;
    }
    driver.initialize().await?;
    clock.sleep(idle_duration).await;
    let idle_snapshot = driver.host_snapshot().await?;
    driver.destroy().await?;
    if !idle_snapshot.managed_webviews_empty {
        return Err(SignatureError::Webview(
            "lifecycle idle generation used a managed WebView".into(),
        ));
    }
    let final_sample = driver.invariant_snapshot().await?;
    validate_lifecycle_invariant_sample(&baseline, &final_sample, true)?;
    Ok((true, true, true, true, true))
}

async fn run_lifecycle_stress(
    runtime: &super::signature_webview::SignatureRuntime,
    client: &reqwest::Client,
    config: &ControlledCanaryConfig,
    profile: super::signature_host::RawHostProfile,
) -> Result<(bool, bool, bool, bool, bool), SignatureError> {
    let mut driver = LiveLifecycleStressDriver {
        runtime,
        client,
        config,
        profile,
    };
    let mut clock = RealLifecycleClock;
    let result = execute_lifecycle_stress(
        &mut driver,
        &mut clock,
        std::time::Duration::from_millis(config.idle_duration_ms),
    )
    .await;
    if result.is_err() {
        let _ = driver.destroy().await;
    }
    result
}

struct PlatformFacts {
    platform_id: String,
    host_platform: String,
    host_arch: String,
    os_version: String,
    binary_target_os: String,
    translated_process: Option<bool>,
}

#[cfg(windows)]
fn platform_facts(
    config: &ControlledCanaryConfig,
    runtime_version: &str,
) -> Result<PlatformFacts, SignatureError> {
    #[repr(C)]
    struct WindowsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        service_pack: [u16; 128],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(version: *mut WindowsVersionInfo) -> i32;
    }

    if std::env::consts::OS != "windows" || std::env::consts::ARCH != "x86_64" {
        return Err(SignatureError::Webview(
            "controlled canary requires a native Windows x86_64 child".into(),
        ));
    }
    let mut version: WindowsVersionInfo = unsafe { std::mem::zeroed() };
    version.size = std::mem::size_of::<WindowsVersionInfo>() as u32;
    if unsafe { RtlGetVersion(&mut version) } < 0 {
        return Err(SignatureError::Webview(
            "native Windows version query failed".into(),
        ));
    }
    let os_version = format!("{}.{}.{}", version.major, version.minor, version.build);
    match config.platform_id.as_str() {
        "windows-10-webview2-111-x64" => {
            let parts = runtime_version.split('.').collect::<Vec<_>>();
            let fixed_runtime = parts.len() == 4
                && parts[0] == "111"
                && parts[1] == "0"
                && parts[2] == "1661"
                && parts[3].bytes().all(|byte| byte.is_ascii_digit());
            if os_version != "10.0.19045" || !fixed_runtime {
                return Err(SignatureError::Webview(
                    "controlled Windows 10 host/runtime correlation failed".into(),
                ));
            }
        }
        "windows-11-x64" if version.build >= 22_000 && !runtime_version.is_empty() => {}
        _ => {
            return Err(SignatureError::Webview(
                "controlled Windows platform correlation failed".into(),
            ));
        }
    }
    Ok(PlatformFacts {
        platform_id: config.platform_id.clone(),
        host_platform: "win32".into(),
        host_arch: "x64".into(),
        os_version,
        binary_target_os: "windows".into(),
        translated_process: None,
    })
}

#[cfg(target_os = "macos")]
fn platform_facts(
    config: &ControlledCanaryConfig,
    runtime_version: &str,
) -> Result<PlatformFacts, SignatureError> {
    if std::env::consts::OS != "macos" || runtime_version.is_empty() {
        return Err(SignatureError::Webview(
            "controlled canary requires a native macOS child and WebKit version".into(),
        ));
    }
    let output = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .map_err(|_| SignatureError::Webview("native macOS version query failed".into()))?;
    if !output.status.success() {
        return Err(SignatureError::Webview(
            "native macOS version query failed".into(),
        ));
    }
    let os_version = std::str::from_utf8(&output.stdout)
        .map_err(|_| SignatureError::Webview("native macOS version was not UTF-8".into()))?
        .trim()
        .to_string();
    let components = os_version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SignatureError::Webview("native macOS version was malformed".into()))?;
    if !(2..=3).contains(&components.len()) {
        return Err(SignatureError::Webview(
            "native macOS version was malformed".into(),
        ));
    }
    let translated = translated_process()?;
    if translated != Some(false) {
        return Err(SignatureError::Webview(
            "controlled macOS child must not be translated".into(),
        ));
    }
    let host_arch = match (config.platform_id.as_str(), std::env::consts::ARCH) {
        ("macos-13-intel", "x86_64") if components[0] == 13 && components[1] == 3 => "x64",
        ("macos-current-arm64", "aarch64") if (components[0], components[1]) >= (13, 3) => "arm64",
        _ => {
            return Err(SignatureError::Webview(
                "controlled macOS platform correlation failed".into(),
            ));
        }
    };
    Ok(PlatformFacts {
        platform_id: config.platform_id.clone(),
        host_platform: "darwin".into(),
        host_arch: host_arch.into(),
        os_version,
        binary_target_os: "macos".into(),
        translated_process: translated,
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_facts(
    _config: &ControlledCanaryConfig,
    _runtime_version: &str,
) -> Result<PlatformFacts, SignatureError> {
    Err(SignatureError::Webview(
        "controlled signature probes are supported only on Windows and macOS".into(),
    ))
}

async fn run_controlled_isolation_probe(
    runtime: &super::signature_webview::SignatureRuntime,
    config: &ControlledCanaryConfig,
) -> Result<IsolationReport, SignatureError> {
    runtime.destroy().await?;
    let first = runtime.initialize().await?;
    let bridge_raw = runtime
        .evaluate_probe_script(bridge_start_script(&format!(
            "yinmi-raw-wry-{}-{}",
            first.generation, first.operation_id
        )))
        .await?;
    let bridge: BridgeObservation = parse_probe_evaluation(&bridge_raw)?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let response_raw = runtime
        .evaluate_probe_script(bridge_finish_script())
        .await?;
    let response: BridgeResponseObservation = parse_probe_evaluation(&response_raw)?;
    if bridge.status != "ok" || response.status != "ok" {
        return Err(SignatureError::Webview(
            "raw WRY bridge probe failed".into(),
        ));
    }
    let canary_after_hidden = ipc_canary_snapshot();
    let marker_key = format!("yinmi-signature-marker-{}", first.generation);
    let marker_value = format!("{}-{}", first.generation, first.operation_id);
    runtime
        .evaluate_probe_script(storage_write_start_script(&marker_key, &marker_value))
        .await?;
    let _ = wait_for_storage_state(runtime).await?;
    let first_host = runtime.probe_host_snapshot().await?;
    let first_url = first.current_url.clone();
    runtime.destroy().await?;

    let second = runtime.initialize().await?;
    runtime
        .evaluate_probe_script(storage_read_start_script(&marker_key, &marker_value))
        .await?;
    let recovered = wait_for_storage_state(runtime).await?;
    let second_host = runtime.probe_host_snapshot().await?;
    let final_url = second.current_url.clone();
    runtime.destroy().await?;

    let counterfactual_profile = super::signature_host::RawHostProfile::controlled(
        config.allowed_origin.as_str().to_string(),
        super::signature_host::RawResourcePolicyProfile::Counterfactual,
    )?;
    let protected_profile = super::signature_host::RawHostProfile::controlled(
        config.allowed_origin.as_str().to_string(),
        super::signature_host::RawResourcePolicyProfile::ProtectedCanary,
    )?;
    let canary_client = canary_client()?;
    let (protected, protected_host, resource_vector_results) = collect_controlled_resource_matrix(
        runtime,
        &canary_client,
        config,
        counterfactual_profile,
        protected_profile.clone(),
    )
    .await?;
    runtime.destroy().await?;
    let mut scenario_driver = RuntimeFixedScenarioDriver {
        runtime,
        retry_profile: protected_profile.clone(),
    };
    let (fixed_scenarios, scenario_checks) =
        execute_fixed_scenario_matrix(&mut scenario_driver).await?;
    let (
        lifecycle_no_monotonic_growth,
        no_orphan_host_windows,
        sleep_wake_observed,
        visible_window_leak_absent,
        unexpected_activation_absent,
    ) = run_lifecycle_stress(runtime, &canary_client, config, protected_profile).await?;
    if !lifecycle_no_monotonic_growth
        || !no_orphan_host_windows
        || !sleep_wake_observed
        || !visible_window_leak_absent
        || !unexpected_activation_absent
    {
        return Err(SignatureError::Webview(
            "controlled canary lifecycle observation failed".into(),
        ));
    }

    let host_labels_after_destroy = runtime
        .app_handle()
        .windows()
        .keys()
        .filter(|label| label.starts_with(super::signature_webview::SIGNATURE_HOST_WINDOW_LABEL))
        .cloned()
        .collect::<Vec<_>>();
    let any_recovered = recovered.local_recovered.unwrap_or(false)
        || recovered.session_recovered.unwrap_or(false)
        || recovered.cookie_recovered.unwrap_or(false)
        || recovered.cache_recovered.unwrap_or(false);
    if first.webview_runtime_version != second.webview_runtime_version
        || first.webview_runtime_version != protected.webview_runtime_version
    {
        return Err(SignatureError::Webview(
            "WebView runtime changed during the controlled probe".into(),
        ));
    }
    let PlatformFacts {
        platform_id,
        host_platform,
        host_arch,
        os_version,
        binary_target_os,
        translated_process,
    } = platform_facts(config, &protected.webview_runtime_version)?;
    let unique_host_generations = first_host.host_label == first.host_label
        && second_host.host_label == second.host_label
        && protected_host.host_label == protected.host_label
        && first_host.host_label != second_host.host_label
        && first_host.host_label != protected_host.host_label
        && second_host.host_label != protected_host.host_label;
    let hidden_delta_zero = canary_after_hidden == 0;
    let counters = protected_host.counters;
    let cross_origin_canary_server_hits = resource_vector_results
        .values()
        .map(|row| row.server_hits)
        .sum::<u64>();
    if cross_origin_canary_server_hits != 0
        || resource_vector_results.len() != RESOURCE_VECTORS.len()
        || resource_vector_results
            .values()
            .any(|row| !row.runtime_attempted)
    {
        return Err(SignatureError::Webview(
            "controlled canary resource observation was incomplete".into(),
        ));
    }

    Ok(IsolationReport {
        generation: protected_host.generation,
        operation_id: protected_host.operation_id,
        platform_id,
        host_platform,
        host_arch,
        os_version,
        binary_target_os,
        binary_target_arch: std::env::consts::ARCH.into(),
        translated_process,
        webview_runtime_version: protected.webview_runtime_version,
        runtime_mode: protected.runtime_mode,
        resource_policy_mode: protected.resource_policy_mode,
        strong_source_kinds_interface_available: {
            #[cfg(windows)]
            {
                Some(protected.strong_source_kinds_interface_available)
            }
            #[cfg(target_os = "macos")]
            {
                None
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                None
            }
        },
        current_url: first_url,
        final_url,
        counters,
        host_labels_after_destroy,
        fixed_scenarios,
        checks: PlatformSignatureChecks {
            raw_wry_host: unique_host_generations
                && first_host.managed_webviews_empty
                && second_host.managed_webviews_empty
                && protected_host.managed_webviews_empty,
            tauri_globals_absent: bridge.tauri_globals_absent,
            application_initialization_scripts_absent: bridge.tauri_globals_absent,
            application_ipc_handler_absent: hidden_delta_zero && !response.response_observed,
            inert_wry_shim_present: bridge.inert_wry_shim_present && bridge.ipc_post_accepted,
            hidden_ipc_canary_delta_zero: hidden_delta_zero,
            hidden_ipc_produced_no_response: !response.response_observed,
            app_state_unchanged: hidden_delta_zero,
            capability_match_absent: first_host.managed_webviews_empty,
            policy_installed_before_first_network_navigation: protected
                .policy_installed_before_first_network_navigation,
            official_finished_before_polling: first.official_finished_before_polling
                && second.official_finished_before_polling,
            official_only_origins: first.current_url == super::signature_webview::GD_PAGE_URL
                && second.current_url == super::signature_webview::GD_PAGE_URL,
            storage_non_persistent: !any_recovered,
            timeout_check: scenario_checks.timeout_check,
            retry_check: scenario_checks.retry_check,
            policy_fault_invalidates_instance: scenario_checks.policy_fault_invalidates_instance,
            late_callback_isolated: scenario_checks.late_callback_isolated,
            destroy_confirmed_before_retry: scenario_checks.destroy_confirmed_before_retry,
            resource_policy_cleanup_acknowledged: scenario_checks
                .resource_policy_cleanup_acknowledged,
            policy_tombstones_empty_before_exit: scenario_checks
                .policy_tombstones_empty_before_exit,
            lifecycle_no_monotonic_growth,
            no_orphan_host_windows,
            visible_window_leak_absent,
            unexpected_activation_absent,
            ordinary_exit_cleanup_acknowledged: false,
            uses_tauri_managed_web_view: false,
            new_instance_storage_recovered: any_recovered,
            restart_storage_recovered: false,
            cross_origin_canary_server_hits,
            #[cfg(windows)]
            blocked_canary_attempts: Some(counters.resource_canary_hits),
            #[cfg(target_os = "macos")]
            blocked_canary_attempts: None,
            #[cfg(not(any(windows, target_os = "macos")))]
            blocked_canary_attempts: None,
            resource_vector_results,
        },
    })
}

pub async fn run_isolation_probe(
    runtime: &super::signature_webview::SignatureRuntime,
) -> Result<IsolationReport, SignatureError> {
    let config = controlled_canary_config_from_trace().await?;
    let baseline_count = reset_ipc_canary();
    if baseline_count == 0 {
        return Err(SignatureError::Webview(
            "invoke feasibility_ipc_canary from the main window before isolation".into(),
        ));
    }
    let result = run_controlled_isolation_probe(runtime, &config).await;
    let cleanup = runtime.destroy().await;
    match (result, cleanup) {
        (Ok(report), Ok(())) => seal_isolation_report(report, || {
            super::webview_resource_policy::assert_policy_cleanup_callbacks_clean()
        }),
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        FIXED_SCENARIO_IDS, FixedScenarioReport, IsolationReport, PlatformSignatureChecks,
        RESOURCE_VECTORS, ResourceVectorResult, SERVICE_WORKER_API_ABSENT_EXPRESSION,
        ServiceWorkerObservation, seal_isolation_report, service_worker_availability,
    };
    use serde_json::json;

    fn canonical_teardown_scenario(
        id: &'static str,
        generation: u64,
        operation_id: u64,
        observed_event: &'static str,
        post_teardown_events: &[&str],
    ) -> FixedScenarioReport {
        let ordered_actor_events = [
            "scenario-started",
            observed_event,
            "generation-invalidated",
            "native-destroyed",
            "manager-host-absent",
            "policy-cleanup-acknowledged",
            "policy-tombstones-empty",
            "teardown-complete",
        ]
        .into_iter()
        .chain(post_teardown_events.iter().copied())
        .map(str::to_string)
        .collect();

        FixedScenarioReport {
            id,
            generation,
            operation_id,
            ordered_actor_events,
            terminal_state: "destroy-confirmed".into(),
        }
    }

    fn canonical_windows_resource_result(vector: &str) -> ResourceVectorResult {
        let (barrier, evidence_mode) = if RESOURCE_VECTORS[..14].contains(&vector) {
            ("webview2-web-resource-requested", "native-callback")
        } else {
            match vector {
                "popup" => ("new-window-handler", "handler-callback"),
                "download" => ("download-handler", "handler-callback"),
                _ => ("navigation-handler", "handler-callback"),
            }
        };

        ResourceVectorResult {
            runtime_attempted: true,
            availability_outcome: "available".into(),
            deterministic_barrier_seam_covered: true,
            expected_barrier: barrier.into(),
            enforced_barrier: barrier.into(),
            barrier_evidence_mode: evidence_mode.into(),
            counterfactual_server_hits: None,
            allowed_redirect_hop_hits: u64::from(vector == "redirect") * 2,
            server_hits: 0,
        }
    }

    fn canonical_windows_10_isolation_report() -> IsolationReport {
        let resource_vector_results = RESOURCE_VECTORS
            .into_iter()
            .map(|vector| {
                (
                    vector.to_string(),
                    canonical_windows_resource_result(vector),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let fixed_scenarios = vec![
            canonical_teardown_scenario(
                "policy-registration-fault",
                10,
                20,
                "policy-registration-fault-observed",
                &["retry-ready", "retry-destroyed"],
            ),
            canonical_teardown_scenario(
                "initialization-finished-delay-past-20s",
                11,
                21,
                "initialization-timeout-observed",
                &["retry-ready", "retry-destroyed"],
            ),
            canonical_teardown_scenario(
                "sign-callback-delay-past-5s",
                12,
                22,
                "sign-timeout-observed",
                &["retry-ready", "retry-destroyed"],
            ),
            canonical_teardown_scenario(
                "destroy-during-pending-policy",
                13,
                23,
                "pending-policy-observed",
                &["retry-ready", "retry-destroyed"],
            ),
            canonical_teardown_scenario(
                "late-callback-after-new-generation",
                14,
                24,
                "sign-timeout-observed",
                &[
                    "retry-ready",
                    "new-generation-ready",
                    "late-callback-isolated",
                    "new-generation-sign-succeeded",
                    "retry-destroyed",
                ],
            ),
            FixedScenarioReport {
                id: "main-close-state-machine-seam",
                generation: 15,
                operation_id: 25,
                ordered_actor_events: [
                    "scenario-started",
                    "would-exit-blocked",
                    "policy-tombstones-empty",
                    "teardown-complete",
                    "would-exit-released",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                terminal_state: "destroy-confirmed".into(),
            },
        ];

        IsolationReport {
            generation: 1,
            operation_id: 2,
            platform_id: "windows-10-webview2-111-x64".into(),
            host_platform: "win32".into(),
            host_arch: "x64".into(),
            os_version: "10.0.19045".into(),
            binary_target_os: "windows".into(),
            binary_target_arch: "x86_64".into(),
            translated_process: None,
            webview_runtime_version: "111.0.1661.54".into(),
            runtime_mode: "native-host-raw-wry-0.55.1".into(),
            resource_policy_mode: "webview2-22-all-source-kinds".into(),
            strong_source_kinds_interface_available: Some(true),
            current_url: "https://music.gdstudio.xyz/".into(),
            final_url: "https://music.gdstudio.xyz/".into(),
            counters: crate::feasibility::webview_resource_policy::IsolationCounterSnapshot {
                blocked_navigations: 4,
                blocked_new_windows: 1,
                blocked_downloads: 1,
                blocked_resource_requests: 14,
                resource_canary_hits: 14,
                policy_faults: 0,
            },
            host_labels_after_destroy: Vec::new(),
            fixed_scenarios,
            checks: PlatformSignatureChecks {
                raw_wry_host: true,
                tauri_globals_absent: true,
                application_initialization_scripts_absent: true,
                application_ipc_handler_absent: true,
                inert_wry_shim_present: true,
                hidden_ipc_canary_delta_zero: true,
                hidden_ipc_produced_no_response: true,
                app_state_unchanged: true,
                capability_match_absent: true,
                policy_installed_before_first_network_navigation: true,
                official_finished_before_polling: true,
                official_only_origins: true,
                storage_non_persistent: true,
                timeout_check: true,
                retry_check: true,
                policy_fault_invalidates_instance: true,
                late_callback_isolated: true,
                destroy_confirmed_before_retry: true,
                resource_policy_cleanup_acknowledged: true,
                policy_tombstones_empty_before_exit: true,
                lifecycle_no_monotonic_growth: true,
                no_orphan_host_windows: true,
                visible_window_leak_absent: true,
                unexpected_activation_absent: true,
                ordinary_exit_cleanup_acknowledged: false,
                uses_tauri_managed_web_view: false,
                new_instance_storage_recovered: false,
                restart_storage_recovered: false,
                cross_origin_canary_server_hits: 0,
                blocked_canary_attempts: Some(14),
                resource_vector_results,
            },
        }
    }

    #[test]
    fn signature_webview_windows_10_wire_report_matches_the_canonical_fixture_exactly() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/signature/windows-10-webview2-111-x64-isolation-report.json"
        ))
        .unwrap();

        assert_eq!(
            serde_json::to_value(canonical_windows_10_isolation_report()).unwrap(),
            fixture
        );
    }

    #[test]
    fn signature_webview_report_seal_rechecks_sticky_cleanup_faults() {
        assert_eq!(seal_isolation_report(7_u8, || Ok(())).unwrap(), 7);
        let error = seal_isolation_report(7_u8, || {
            Err(super::SignatureError::Webview(
                "injected late callback fault".into(),
            ))
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected late callback fault"));
    }

    #[test]
    fn signature_webview_probe_resource_vector_rows_have_the_exact_nine_key_schema() {
        assert_eq!(RESOURCE_VECTORS.len(), 20);
        assert_eq!(
            RESOURCE_VECTORS
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            20
        );
        let row = ResourceVectorResult {
            runtime_attempted: true,
            availability_outcome: "available".into(),
            deterministic_barrier_seam_covered: true,
            expected_barrier: "native-resource-policy".into(),
            enforced_barrier: "native-resource-policy".into(),
            barrier_evidence_mode: "counterfactual-and-server-observation".into(),
            counterfactual_server_hits: Some(1),
            allowed_redirect_hop_hits: 0,
            server_hits: 0,
        };
        assert_eq!(
            serde_json::to_value(row).unwrap(),
            json!({
                "runtimeAttempted": true,
                "availabilityOutcome": "available",
                "deterministicBarrierSeamCovered": true,
                "expectedBarrier": "native-resource-policy",
                "enforcedBarrier": "native-resource-policy",
                "barrierEvidenceMode": "counterfactual-and-server-observation",
                "counterfactualServerHits": 1,
                "allowedRedirectHopHits": 0,
                "serverHits": 0
            })
        );
    }

    #[test]
    fn signature_webview_probe_never_accepts_placeholder_resource_rows_as_observation() {
        use super::{BrowserTriggerState, CanarySnapshot, derive_resource_row};
        use crate::feasibility::webview_resource_policy::IsolationCounterSnapshot;

        let mut rows = std::collections::BTreeMap::new();
        for vector in RESOURCE_VECTORS {
            let before = IsolationCounterSnapshot::default();
            let mut after = before;
            if RESOURCE_VECTORS[..14].contains(&vector) {
                after.resource_canary_hits = 1;
                after.blocked_resource_requests = 1;
            } else if vector == "popup" {
                after.blocked_new_windows = 1;
            } else if vector == "download" {
                after.blocked_downloads = 1;
            } else {
                after.blocked_navigations = 1;
            }
            let counterfactual = CanarySnapshot {
                run_id: "0123456789abcdef0123456789abcdef".into(),
                mode: "counterfactual".into(),
                vector: vector.into(),
                direct_hits: 1,
                allowed_redirect_hop_hits: u64::from(vector == "redirect") * 2,
                browser_preflight_hits: 0,
                websocket_handshakes: u64::from(vector == "websocket"),
                sleep_wake_observed: false,
                browser_process_baseline: 0,
                browser_process_current: 0,
                visible_window_leak_observed: false,
                unexpected_activation_observed: false,
            };
            let protected = CanarySnapshot {
                run_id: "0123456789abcdef0123456789abcdef".into(),
                mode: "protected".into(),
                vector: vector.into(),
                direct_hits: 0,
                allowed_redirect_hop_hits: u64::from(vector == "redirect") * 2,
                browser_preflight_hits: 0,
                websocket_handshakes: 0,
                sleep_wake_observed: false,
                browser_process_baseline: 0,
                browser_process_current: 0,
                visible_window_leak_observed: false,
                unexpected_activation_observed: false,
            };
            rows.insert(
                vector,
                derive_resource_row(
                    vector,
                    &BrowserTriggerState {
                        status: "ok".into(),
                        done: true,
                        attempted: true,
                        availability_outcome: "available".into(),
                        error: None,
                    },
                    RESOURCE_VECTORS[..14]
                        .contains(&vector)
                        .then_some(&counterfactual),
                    &protected,
                    before,
                    after,
                )
                .unwrap(),
            );
        }
        assert_eq!(rows.len(), RESOURCE_VECTORS.len());
        for (vector, row) in rows {
            assert!(row.runtime_attempted, "{vector} was not attempted");
            assert_ne!(
                row.availability_outcome, "controlled-canary-required",
                "{vector} returned a placeholder availability outcome"
            );
            assert_ne!(
                row.barrier_evidence_mode, "deterministic-seam-only",
                "{vector} returned seam-only evidence as an observation"
            );
        }
    }

    #[test]
    fn signature_webview_builds_all_twenty_fixed_runtime_triggers() {
        use super::{AutorunPhase, browser_trigger_start_script, parse_controlled_canary_config};

        let config = parse_controlled_canary_config(
            &serde_json::json!({
                "runId": "0123456789abcdef0123456789abcdef",
                "phase": "write-marker-and-close-main",
                "platformId": "windows-11-x64",
                "controlOrigin": "http://127.0.0.1:50000/",
                "allowedOrigin": "https://127.0.0.1:50001/",
                "blockedHttpOrigin": "http://127.0.0.1:50000/",
                "blockedHttpsOrigin": "https://127.0.0.1:50002/",
                "blockedWsOrigin": "ws://127.0.0.1:50000/",
                "blockedWssOrigin": "wss://127.0.0.1:50002/",
                "idleDurationMs": 600000
            })
            .to_string(),
            "0123456789abcdef0123456789abcdef",
            AutorunPhase::WriteMarkerAndCloseMain,
        )
        .unwrap();
        let scripts = RESOURCE_VECTORS
            .into_iter()
            .map(|vector| {
                (
                    vector,
                    browser_trigger_start_script(&config, "protected", vector).unwrap(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(scripts.len(), 20);
        assert!(scripts["document"].contains("document.createElement(\"object\")"));
        assert!(scripts["worker"].contains("importScripts"));
        assert!(scripts["service_worker"].contains(r#"!("serviceWorker" in navigator)"#));
        assert!(scripts["service_worker"].contains("worker.state"));
        assert!(scripts["service_worker"].contains("service-worker-terminal-timeout"));
        assert!(scripts["websocket"].contains("new WebSocket"));
        assert!(scripts["sse"].contains("new EventSource"));
        assert!(scripts["beacon"].contains("new Uint8Array([1])"));
        assert!(scripts["beacon"].contains("new Promise((resolve) => setTimeout(resolve, 250))"));
        assert!(!scripts["beacon"].contains("settle(undefined, 250)"));
        assert!(scripts["redirect"].contains("/redirect/one"));
        assert!(scripts["popup"].contains("window.open"));
        assert!(scripts["download"].contains("link.download"));
        assert!(scripts["top_level_data"].contains("data:text/html,yinmi-probe"));
        assert!(scripts["top_level_blob"].contains("URL.createObjectURL"));
        assert!(scripts["top_level_file"].contains("file:///yinmi-feasibility-denied"));
        assert!(scripts["top_level_custom_protocol"].contains("yinmi-feasibility-denied://probe"));
    }

    #[test]
    fn signature_webview_probe_service_worker_absence_is_only_allowed_for_fixed_api_absence() {
        assert_eq!(
            SERVICE_WORKER_API_ABSENT_EXPRESSION,
            r#"!("serviceWorker" in navigator)"#
        );
        assert_eq!(
            service_worker_availability(ServiceWorkerObservation::ApiPresence(false)).unwrap(),
            "service-worker-api-absent"
        );
        assert_eq!(
            service_worker_availability(ServiceWorkerObservation::ApiPresence(true)).unwrap(),
            "available"
        );
        for observation in [
            ServiceWorkerObservation::Rejected,
            ServiceWorkerObservation::TimedOut,
            ServiceWorkerObservation::CspBlocked,
            ServiceWorkerObservation::ScriptFailed,
        ] {
            assert!(service_worker_availability(observation).is_err());
        }
    }

    #[test]
    fn signature_webview_probe_fixed_negative_scenarios_are_closed_and_generation_ordered() {
        assert_eq!(
            FIXED_SCENARIO_IDS,
            [
                "policy-registration-fault",
                "initialization-finished-delay-past-20s",
                "sign-callback-delay-past-5s",
                "destroy-during-pending-policy",
                "late-callback-after-new-generation",
                "main-close-state-machine-seam",
            ]
        );
    }

    #[derive(Default)]
    struct FakeFixedScenarioDriver {
        calls: Vec<&'static str>,
    }

    impl super::FixedScenarioDriver for FakeFixedScenarioDriver {
        async fn run_scenario(
            &mut self,
            id: &'static str,
        ) -> Result<super::FixedScenarioReport, super::SignatureError> {
            self.calls.push(id);
            let index = super::FIXED_SCENARIO_IDS
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap();
            let specific = match id {
                "policy-registration-fault" => "policy-registration-fault-observed",
                "initialization-finished-delay-past-20s" => "initialization-timeout-observed",
                "sign-callback-delay-past-5s" => "sign-timeout-observed",
                "destroy-during-pending-policy" => "pending-policy-observed",
                "late-callback-after-new-generation" => "sign-timeout-observed",
                "main-close-state-machine-seam" => "would-exit-blocked",
                _ => unreachable!(),
            };
            let mut events = vec!["scenario-started".into(), specific.into()];
            if id == "main-close-state-machine-seam" {
                events.extend([
                    "policy-tombstones-empty".into(),
                    "teardown-complete".into(),
                    "would-exit-released".into(),
                ]);
            } else {
                events.extend([
                    "generation-invalidated".into(),
                    "native-destroyed".into(),
                    "manager-host-absent".into(),
                    "policy-cleanup-acknowledged".into(),
                    "policy-tombstones-empty".into(),
                    "teardown-complete".into(),
                    "retry-ready".into(),
                ]);
                if id == "late-callback-after-new-generation" {
                    events.extend([
                        "new-generation-ready".into(),
                        "late-callback-isolated".into(),
                        "new-generation-sign-succeeded".into(),
                    ]);
                }
                events.push("retry-destroyed".into());
            }
            Ok(super::FixedScenarioReport {
                id,
                generation: 100 + index as u64,
                operation_id: 200 + index as u64,
                ordered_actor_events: events,
                terminal_state: "destroy-confirmed".into(),
            })
        }
    }

    #[tokio::test]
    async fn signature_webview_fixed_scenario_executor_derives_checks_from_driver_traces() {
        let mut driver = FakeFixedScenarioDriver::default();
        let (reports, checks) = super::execute_fixed_scenario_matrix(&mut driver)
            .await
            .unwrap();
        assert_eq!(driver.calls, super::FIXED_SCENARIO_IDS);
        assert_eq!(reports.len(), 6);
        assert!(checks.timeout_check);
        assert!(checks.retry_check);
        assert!(checks.policy_fault_invalidates_instance);
        assert!(checks.late_callback_isolated);
        assert!(checks.destroy_confirmed_before_retry);
        assert!(checks.resource_policy_cleanup_acknowledged);
        assert!(checks.policy_tombstones_empty_before_exit);
    }

    #[test]
    fn signature_webview_fixed_scenario_appends_an_already_consumed_runtime_audit_once() {
        let ticket = crate::feasibility::signature_host::CreationTicket::new(31, 47);
        ticket.mark_policy_cleanup();
        ticket.mark_tombstones_empty();
        ticket.mark_native_destroyed();
        ticket.mark_manager_absent();
        let audit = ticket.teardown_audit();
        let mut events = vec!["scenario-started".into()];
        super::append_verified_teardown_audit(&audit, &mut events).unwrap();
        assert_eq!(
            events,
            [
                "scenario-started",
                "generation-invalidated",
                "policy-cleanup-acknowledged",
                "policy-tombstones-empty",
                "native-destroyed",
                "manager-host-absent",
                "teardown-complete",
            ]
        );
    }

    #[test]
    fn signature_webview_autorun_process_ack_is_exact_correlated_and_typed() {
        use super::{AutorunKind, AutorunPhase, parse_process_info_ack};

        let run_id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_process_info_ack(
                br#"{"accepted":"process-info","runId":"0123456789abcdef0123456789abcdef","kind":"isolation","phase":"write-marker-and-close-main"}"#,
                run_id,
                AutorunPhase::WriteMarkerAndCloseMain,
            )
            .unwrap(),
            AutorunKind::Isolation,
        );
        assert_eq!(
            parse_process_info_ack(
                br#"{"accepted":"process-info","runId":"0123456789abcdef0123456789abcdef","kind":"lifecycle","phase":"verify-marker-absent"}"#,
                run_id,
                AutorunPhase::VerifyMarkerAbsent,
            )
            .unwrap(),
            AutorunKind::Lifecycle,
        );
        for invalid in [
            br#"{"accepted":"event","runId":"0123456789abcdef0123456789abcdef","kind":"isolation","phase":"write-marker-and-close-main"}"#.as_slice(),
            br#"{"accepted":"process-info","runId":"fedcba9876543210fedcba9876543210","kind":"isolation","phase":"write-marker-and-close-main"}"#.as_slice(),
            br#"{"accepted":"process-info","runId":"0123456789abcdef0123456789abcdef","kind":"isolation","phase":"verify-marker-absent"}"#.as_slice(),
            br#"{"accepted":"process-info","runId":"0123456789abcdef0123456789abcdef","kind":"isolation","phase":"write-marker-and-close-main","extra":true}"#.as_slice(),
        ] {
            assert!(
                parse_process_info_ack(
                    invalid,
                    run_id,
                    AutorunPhase::WriteMarkerAndCloseMain,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn signature_webview_probe_autorun_environment_is_exact_and_fail_closed() {
        use super::{
            AUTORUN_ENV, AutorunPhase, RUN_ID_ENV, TRACE_ENDPOINT_ENV,
            parse_autorun_environment_values,
        };

        assert_eq!(AUTORUN_ENV, "YINMI_FEASIBILITY_SIGNATURE_AUTORUN");
        assert_eq!(
            TRACE_ENDPOINT_ENV,
            "YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT"
        );
        assert_eq!(RUN_ID_ENV, "YINMI_FEASIBILITY_SIGNATURE_RUN_ID");
        assert_eq!(
            parse_autorun_environment_values(
                Some("write-marker-and-close-main"),
                Some("http://127.0.0.1:49152/"),
                Some("0123456789abcdef0123456789abcdef"),
            )
            .unwrap()
            .unwrap()
            .phase,
            AutorunPhase::WriteMarkerAndCloseMain
        );
        assert_eq!(
            parse_autorun_environment_values(
                Some("verify-marker-absent"),
                Some("http://127.0.0.1:49152/"),
                Some("fedcba9876543210fedcba9876543210"),
            )
            .unwrap()
            .unwrap()
            .phase,
            AutorunPhase::VerifyMarkerAbsent
        );
        assert!(
            parse_autorun_environment_values(None, None, None)
                .unwrap()
                .is_none()
        );
        for invalid in [
            (
                Some("unknown"),
                Some("http://127.0.0.1:1/"),
                Some("0123456789abcdef0123456789abcdef"),
            ),
            (
                Some("write-marker-and-close-main"),
                Some("https://127.0.0.1:1/"),
                Some("0123456789abcdef0123456789abcdef"),
            ),
            (
                Some("write-marker-and-close-main"),
                Some("http://localhost:1/"),
                Some("0123456789abcdef0123456789abcdef"),
            ),
            (
                Some("write-marker-and-close-main"),
                Some("http://127.0.0.1:1/path"),
                Some("0123456789abcdef0123456789abcdef"),
            ),
            (
                Some("write-marker-and-close-main"),
                Some("http://127.0.0.1:1/"),
                Some("ABCDEF0123456789ABCDEF0123456789"),
            ),
            (
                Some("write-marker-and-close-main"),
                Some("http://127.0.0.1:1/"),
                Some("short"),
            ),
            (
                Some("write-marker-and-close-main"),
                None,
                Some("0123456789abcdef0123456789abcdef"),
            ),
        ] {
            assert!(parse_autorun_environment_values(invalid.0, invalid.1, invalid.2).is_err());
        }
    }

    #[test]
    fn signature_webview_controlled_canary_configuration_is_exact_and_loopback_only() {
        use super::{AutorunPhase, parse_controlled_canary_config};

        let valid = serde_json::json!({
            "runId": "0123456789abcdef0123456789abcdef",
            "phase": "write-marker-and-close-main",
            "platformId": "windows-11-x64",
            "controlOrigin": "http://127.0.0.1:50000/",
            "allowedOrigin": "https://127.0.0.1:50001/",
            "blockedHttpOrigin": "http://127.0.0.1:50000/",
            "blockedHttpsOrigin": "https://127.0.0.1:50002/",
            "blockedWsOrigin": "ws://127.0.0.1:50000/",
            "blockedWssOrigin": "wss://127.0.0.1:50002/",
            "idleDurationMs": 600000
        });
        let parsed = parse_controlled_canary_config(
            &valid.to_string(),
            "0123456789abcdef0123456789abcdef",
            AutorunPhase::WriteMarkerAndCloseMain,
        )
        .unwrap();
        assert_eq!(parsed.allowed_origin.as_str(), "https://127.0.0.1:50001/");
        assert_eq!(parsed.idle_duration_ms, 600_000);

        for invalid in [
            {
                let mut value = valid.clone();
                value["allowedOrigin"] = serde_json::json!("https://music.gdstudio.xyz/");
                value
            },
            {
                let mut value = valid.clone();
                value["controlOrigin"] = serde_json::json!("http://localhost:50000/");
                value
            },
            {
                let mut value = valid.clone();
                value["idleDurationMs"] = serde_json::json!(1);
                value
            },
            {
                let mut value = valid.clone();
                value["extra"] = serde_json::json!(true);
                value
            },
        ] {
            assert!(
                parse_controlled_canary_config(
                    &invalid.to_string(),
                    "0123456789abcdef0123456789abcdef",
                    AutorunPhase::WriteMarkerAndCloseMain,
                )
                .is_err()
            );
        }
        assert!(
            parse_controlled_canary_config(
                &valid.to_string(),
                "fedcba9876543210fedcba9876543210",
                AutorunPhase::WriteMarkerAndCloseMain,
            )
            .is_err()
        );
    }

    #[test]
    fn signature_webview_probe_freezes_every_browser_trigger() {
        use super::RESOURCE_VECTOR_TRIGGERS;

        assert_eq!(RESOURCE_VECTOR_TRIGGERS.len(), RESOURCE_VECTORS.len());
        assert!(RESOURCE_VECTOR_TRIGGERS.iter().any(|(id, trigger)| {
            *id == "document" && trigger.contains("<object") && trigger.contains("text/html")
        }));
        assert!(RESOURCE_VECTOR_TRIGGERS.iter().any(|(id, trigger)| {
            *id == "service_worker"
                && trigger.contains("/sw.js")
                && trigger.contains("BLOCKED_HTTPS_URL")
        }));
        assert!(RESOURCE_VECTOR_TRIGGERS.iter().any(|(id, trigger)| {
            *id == "redirect"
                && trigger.contains("/redirect/one")
                && trigger.contains("ALLOWED_HTTPS_URL")
        }));
        assert!(RESOURCE_VECTOR_TRIGGERS.iter().any(|(id, trigger)| {
            *id == "top_level_custom_protocol"
                && trigger.contains("yinmi-feasibility-denied://probe")
        }));
    }

    #[derive(Default)]
    struct FakeResourceMatrixDriver {
        active_profile: Option<crate::feasibility::signature_host::RawResourcePolicyProfile>,
        counters: crate::feasibility::webview_resource_policy::IsolationCounterSnapshot,
        events: Vec<String>,
        generation: u64,
        service_worker_absent: bool,
        service_worker_error: bool,
        late_protected_hit: bool,
    }

    impl super::ResourceMatrixDriver for FakeResourceMatrixDriver {
        async fn destroy(&mut self) -> Result<(), super::SignatureError> {
            let name = match self.active_profile.take() {
                Some(
                    crate::feasibility::signature_host::RawResourcePolicyProfile::Counterfactual,
                ) => "counterfactual",
                Some(
                    crate::feasibility::signature_host::RawResourcePolicyProfile::ProtectedCanary,
                ) => "protected",
                Some(crate::feasibility::signature_host::RawResourcePolicyProfile::Live) => "live",
                None => "none",
            };
            self.events.push(format!("destroy:{name}"));
            self.counters = Default::default();
            Ok(())
        }

        async fn initialize(
            &mut self,
            profile: crate::feasibility::signature_host::RawHostProfile,
        ) -> Result<crate::feasibility::signature_webview::SignatureInitReport, super::SignatureError>
        {
            let profile_name = match profile.resource_policy {
                crate::feasibility::signature_host::RawResourcePolicyProfile::Counterfactual => {
                    "counterfactual"
                }
                crate::feasibility::signature_host::RawResourcePolicyProfile::ProtectedCanary => {
                    "protected"
                }
                crate::feasibility::signature_host::RawResourcePolicyProfile::Live => "live",
            };
            self.events.push(format!("initialize:{profile_name}"));
            self.active_profile = Some(profile.resource_policy);
            self.generation += 1;
            Ok(crate::feasibility::signature_webview::SignatureInitReport {
                generation: self.generation,
                operation_id: self.generation,
                host_label: format!("yinmi-signature-host-{}", self.generation),
                webview_id: "yinmi-signature-raw".into(),
                current_url: profile.navigation_url,
                runtime_mode: "native-host-raw-wry-0.55.1".into(),
                webview_runtime_version: "test-runtime".into(),
                resource_policy_mode: match profile.resource_policy {
                    crate::feasibility::signature_host::RawResourcePolicyProfile::Counterfactual => {
                        "counterfactual-no-resource-rule".into()
                    }
                    #[cfg(windows)]
                    _ => "webview2-22-all-source-kinds".into(),
                    #[cfg(target_os = "macos")]
                    _ => "wk-content-rule-list-exact-origin".into(),
                    #[cfg(not(any(windows, target_os = "macos")))]
                    _ => "unsupported-platform-resource-policy".into(),
                },
                strong_source_kinds_interface_available: cfg!(windows),
                official_finished_before_polling: true,
                policy_installed_before_first_network_navigation: profile.resource_policy
                    != crate::feasibility::signature_host::RawResourcePolicyProfile::Counterfactual,
            })
        }

        async fn preflight(&mut self) -> Result<(), super::SignatureError> {
            self.events.push("browser-preflight".into());
            Ok(())
        }

        async fn reset(&mut self, mode: &str, vector: &str) -> Result<(), super::SignatureError> {
            self.events.push(format!("reset:{mode}:{vector}"));
            Ok(())
        }

        async fn trigger(
            &mut self,
            mode: &str,
            vector: &str,
        ) -> Result<super::BrowserTriggerState, super::SignatureError> {
            self.events.push(format!("evaluate:{mode}:{vector}"));
            let unavailable = vector == "service_worker" && self.service_worker_absent;
            if mode == "protected" && !unavailable {
                if super::RESOURCE_VECTORS[..14].contains(&vector) {
                    self.counters.blocked_resource_requests += 1;
                    self.counters.resource_canary_hits += 1;
                } else if vector == "popup" {
                    self.counters.blocked_new_windows += 1;
                } else if vector == "download" {
                    self.counters.blocked_downloads += 1;
                } else {
                    self.counters.blocked_navigations += 1;
                }
            }
            Ok(super::BrowserTriggerState {
                status: "ok".into(),
                done: true,
                attempted: true,
                availability_outcome: if unavailable {
                    "service-worker-api-absent".into()
                } else {
                    "available".into()
                },
                error: (vector == "service_worker" && self.service_worker_error)
                    .then(|| "service-worker-registration-rejected".into()),
            })
        }

        async fn complete(
            &mut self,
            mode: &str,
            vector: &str,
        ) -> Result<(), super::SignatureError> {
            self.events.push(format!("complete:{mode}:{vector}"));
            Ok(())
        }

        async fn observation(
            &mut self,
            mode: &str,
            vector: &str,
            wait_for_direct_hit: bool,
        ) -> Result<super::CanarySnapshot, super::SignatureError> {
            self.events.push(format!("recorder:{mode}:{vector}"));
            let unavailable = vector == "service_worker" && self.service_worker_absent;
            let direct_hits = u64::from(mode == "counterfactual" && !unavailable);
            if wait_for_direct_hit != (direct_hits > 0) {
                return Err(super::SignatureError::Webview(
                    "fake recorder wait contract did not match the matrix".into(),
                ));
            }
            Ok(super::CanarySnapshot {
                run_id: "0123456789abcdef0123456789abcdef".into(),
                mode: mode.into(),
                vector: vector.into(),
                direct_hits,
                allowed_redirect_hop_hits: u64::from(vector == "redirect") * 2,
                browser_preflight_hits: 0,
                websocket_handshakes: u64::from(vector == "websocket" && direct_hits > 0),
                sleep_wake_observed: false,
                browser_process_baseline: 4,
                browser_process_current: 4,
                visible_window_leak_observed: false,
                unexpected_activation_observed: false,
            })
        }

        async fn host_snapshot(
            &mut self,
        ) -> Result<crate::feasibility::signature_host::RawHostProbeSnapshot, super::SignatureError>
        {
            self.events.push("native-counter-snapshot".into());
            Ok(crate::feasibility::signature_host::RawHostProbeSnapshot {
                generation: self.generation,
                operation_id: self.generation,
                host_label: format!("yinmi-signature-host-{}", self.generation),
                managed_webviews_empty: true,
                counters: self.counters,
            })
        }

        async fn seal_protected(&mut self) -> Result<(), super::SignatureError> {
            self.events.push("seal:protected".into());
            for vector in super::RESOURCE_VECTORS {
                if !self
                    .events
                    .iter()
                    .any(|event| event == &format!("complete:protected:{vector}"))
                    || !self
                        .events
                        .iter()
                        .any(|event| event == &format!("recorder:protected:{vector}"))
                {
                    return Err(super::SignatureError::Webview(
                        "fake protected completion barrier was incomplete".into(),
                    ));
                }
            }
            Ok(())
        }

        async fn verify_protected_seal(&mut self) -> Result<(), super::SignatureError> {
            self.events.push("verify-seal:protected".into());
            if self.late_protected_hit {
                Err(super::SignatureError::Webview(
                    "fake late protected hit invalidated the seal".into(),
                ))
            } else {
                Ok(())
            }
        }
    }

    fn controlled_test_profiles() -> (
        crate::feasibility::signature_host::RawHostProfile,
        crate::feasibility::signature_host::RawHostProfile,
    ) {
        use crate::feasibility::signature_host::{RawHostProfile, RawResourcePolicyProfile};

        (
            RawHostProfile::controlled(
                "https://127.0.0.1:54321/".into(),
                RawResourcePolicyProfile::Counterfactual,
            )
            .unwrap(),
            RawHostProfile::controlled(
                "https://127.0.0.1:54321/".into(),
                RawResourcePolicyProfile::ProtectedCanary,
            )
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn signature_webview_matrix_executor_evaluates_and_records_all_frozen_vectors() {
        let mut driver = FakeResourceMatrixDriver::default();
        let (counterfactual, protected) = controlled_test_profiles();
        let (_, _, rows) = super::collect_resource_matrix(&mut driver, counterfactual, protected)
            .await
            .unwrap();

        assert_eq!(rows.len(), 20);
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| event.starts_with("evaluate:counterfactual:"))
                .count(),
            14
        );
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| event.starts_with("recorder:counterfactual:"))
                .count(),
            14
        );
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| event.starts_with("evaluate:protected:"))
                .count(),
            20
        );
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| event.starts_with("recorder:protected:"))
                .count(),
            20
        );
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| event.starts_with("complete:counterfactual:"))
                .count(),
            14
        );
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| event.starts_with("complete:protected:"))
                .count(),
            20
        );
        let counterfactual_destroy = driver
            .events
            .iter()
            .position(|event| event == "destroy:counterfactual")
            .unwrap();
        let protected_initialize = driver
            .events
            .iter()
            .position(|event| event == "initialize:protected")
            .unwrap();
        assert!(counterfactual_destroy < protected_initialize);
        let protected_seal = driver
            .events
            .iter()
            .position(|event| event == "seal:protected")
            .unwrap();
        let protected_destroy = driver
            .events
            .iter()
            .position(|event| event == "destroy:protected")
            .unwrap();
        let protected_verify = driver
            .events
            .iter()
            .position(|event| event == "verify-seal:protected")
            .unwrap();
        assert!(protected_seal < protected_destroy);
        assert!(protected_destroy < protected_verify);

        for vector in super::RESOURCE_VECTORS {
            let row = &rows[vector];
            assert!(row.runtime_attempted, "{vector}");
            assert_eq!(row.availability_outcome, "available", "{vector}");
            assert_eq!(row.server_hits, 0, "{vector}");
            assert_eq!(
                row.allowed_redirect_hop_hits,
                u64::from(vector == "redirect") * 2,
                "{vector}"
            );
            if super::RESOURCE_VECTORS[..14].contains(&vector) {
                #[cfg(windows)]
                assert_eq!(row.barrier_evidence_mode, "native-callback", "{vector}");
                #[cfg(target_os = "macos")]
                {
                    assert_eq!(
                        row.barrier_evidence_mode, "paired-counterfactual",
                        "{vector}"
                    );
                    assert_eq!(row.counterfactual_server_hits, Some(1), "{vector}");
                }
            } else {
                assert_eq!(row.barrier_evidence_mode, "handler-callback", "{vector}");
            }
        }
    }

    #[tokio::test]
    async fn signature_webview_matrix_executor_allows_only_the_exact_service_worker_absence() {
        let mut absent_driver = FakeResourceMatrixDriver {
            service_worker_absent: true,
            ..Default::default()
        };
        let (counterfactual, protected) = controlled_test_profiles();
        let (_, _, rows) =
            super::collect_resource_matrix(&mut absent_driver, counterfactual, protected)
                .await
                .unwrap();
        let service_worker = &rows["service_worker"];
        assert_eq!(
            service_worker.availability_outcome,
            "service-worker-api-absent"
        );
        assert_eq!(
            service_worker.barrier_evidence_mode,
            "deterministic-seam-only"
        );
        assert_eq!(service_worker.counterfactual_server_hits, None);
        assert!(rows.iter().all(|(vector, row)| {
            vector == "service_worker" || row.availability_outcome == "available"
        }));

        let mut rejected_driver = FakeResourceMatrixDriver {
            service_worker_error: true,
            ..Default::default()
        };
        let (counterfactual, protected) = controlled_test_profiles();
        assert!(
            super::collect_resource_matrix(&mut rejected_driver, counterfactual, protected)
                .await
                .is_err()
        );
        assert_eq!(
            rejected_driver.events.last().map(String::as_str),
            Some("destroy:counterfactual")
        );
    }

    #[tokio::test]
    async fn signature_webview_matrix_executor_fails_if_a_hit_arrives_after_the_protected_seal() {
        let mut driver = FakeResourceMatrixDriver {
            late_protected_hit: true,
            ..Default::default()
        };
        let (counterfactual, protected) = controlled_test_profiles();
        assert!(
            super::collect_resource_matrix(&mut driver, counterfactual, protected)
                .await
                .is_err()
        );
        assert!(driver.events.iter().any(|event| event == "seal:protected"));
        assert!(
            driver
                .events
                .iter()
                .any(|event| event == "verify-seal:protected")
        );
    }

    #[derive(Default)]
    struct FakeLifecycleDriver {
        active: bool,
        destroy_count: usize,
        initialize_count: usize,
        invariant_count: usize,
        reset_count: usize,
        sign_count: usize,
        browser_growth_sample: Option<usize>,
        store_residue_sample: Option<usize>,
        tombstone_residue_sample: Option<usize>,
    }

    impl super::LifecycleStressDriver for FakeLifecycleDriver {
        async fn reset(&mut self) -> Result<(), super::SignatureError> {
            self.reset_count += 1;
            Ok(())
        }

        async fn initialize(
            &mut self,
        ) -> Result<crate::feasibility::signature_webview::SignatureInitReport, super::SignatureError>
        {
            self.initialize_count += 1;
            self.active = true;
            Ok(crate::feasibility::signature_webview::SignatureInitReport {
                generation: self.initialize_count as u64,
                operation_id: self.initialize_count as u64,
                host_label: format!("yinmi-signature-host-{}", self.initialize_count),
                webview_id: "yinmi-signature-raw".into(),
                current_url: "https://127.0.0.1:54321/".into(),
                runtime_mode: "native-host-raw-wry-0.55.1".into(),
                webview_runtime_version: "test-runtime".into(),
                resource_policy_mode: "test-policy".into(),
                strong_source_kinds_interface_available: cfg!(windows),
                official_finished_before_polling: true,
                policy_installed_before_first_network_navigation: true,
            })
        }

        async fn sign(&mut self) -> Result<(), super::SignatureError> {
            assert!(self.active);
            self.sign_count += 1;
            Ok(())
        }

        async fn host_snapshot(
            &mut self,
        ) -> Result<crate::feasibility::signature_host::RawHostProbeSnapshot, super::SignatureError>
        {
            assert!(self.active);
            Ok(crate::feasibility::signature_host::RawHostProbeSnapshot {
                generation: self.initialize_count as u64,
                operation_id: self.initialize_count as u64,
                host_label: format!("yinmi-signature-host-{}", self.initialize_count),
                managed_webviews_empty: true,
                counters: Default::default(),
            })
        }

        async fn destroy(&mut self) -> Result<(), super::SignatureError> {
            self.active = false;
            self.destroy_count += 1;
            Ok(())
        }

        async fn invariant_snapshot(
            &mut self,
        ) -> Result<super::LifecycleInvariantSnapshot, super::SignatureError> {
            self.invariant_count += 1;
            let mut identifiers = Vec::new();
            if self.store_residue_sample == Some(self.invariant_count) {
                identifiers.push("yinmi-gd-signature-leaked".into());
            }
            let tombstones = if self.tombstone_residue_sample == Some(self.invariant_count) {
                vec!["yinmi-gd-signature-tombstoned".into()]
            } else {
                Vec::new()
            };
            Ok(super::LifecycleInvariantSnapshot {
                browser_process_baseline: 4,
                browser_process_current: if self.browser_growth_sample == Some(self.invariant_count)
                {
                    5
                } else {
                    4
                },
                visible_window_leak_observed: false,
                unexpected_activation_observed: false,
                sleep_wake_observed: self.invariant_count == super::LIFECYCLE_CYCLE_COUNT + 2,
                slot_active: self.active,
                host_windows_absent: !self.active,
                policy_store: crate::feasibility::webview_resource_policy::PolicyStoreSnapshot {
                    backend: "fake-wk-content-rule-list-store",
                    identifiers,
                    tombstones,
                },
            })
        }
    }

    #[derive(Default)]
    struct FakeLifecycleClock {
        sleeps: Vec<std::time::Duration>,
    }

    impl super::LifecycleClock for FakeLifecycleClock {
        async fn sleep(&mut self, duration: std::time::Duration) {
            self.sleeps.push(duration);
        }
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_executor_runs_twenty_cycles_and_exact_idle_duration() {
        let mut driver = FakeLifecycleDriver::default();
        let mut clock = FakeLifecycleClock::default();
        let result = super::execute_lifecycle_stress(
            &mut driver,
            &mut clock,
            std::time::Duration::from_millis(super::CANARY_IDLE_DURATION_MS),
        )
        .await
        .unwrap();
        assert_eq!(result, (true, true, true, true, true));
        assert_eq!(driver.reset_count, 1);
        assert_eq!(driver.initialize_count, super::LIFECYCLE_CYCLE_COUNT + 1);
        assert_eq!(driver.sign_count, super::LIFECYCLE_CYCLE_COUNT);
        assert_eq!(driver.destroy_count, super::LIFECYCLE_CYCLE_COUNT + 1);
        assert_eq!(driver.invariant_count, super::LIFECYCLE_CYCLE_COUNT + 2);
        assert_eq!(clock.sleeps, [std::time::Duration::from_millis(600_000)]);
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_fails_on_intermediate_growth_that_later_recovers() {
        let mut driver = FakeLifecycleDriver {
            browser_growth_sample: Some(2),
            ..Default::default()
        };
        let mut clock = FakeLifecycleClock::default();
        assert!(
            super::execute_lifecycle_stress(
                &mut driver,
                &mut clock,
                std::time::Duration::from_millis(super::CANARY_IDLE_DURATION_MS),
            )
            .await
            .is_err()
        );
        assert_eq!(driver.invariant_count, 2);
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_fails_on_macos_store_prefix_residue() {
        let mut driver = FakeLifecycleDriver {
            store_residue_sample: Some(5),
            ..Default::default()
        };
        let mut clock = FakeLifecycleClock::default();
        assert!(
            super::execute_lifecycle_stress(
                &mut driver,
                &mut clock,
                std::time::Duration::from_millis(super::CANARY_IDLE_DURATION_MS),
            )
            .await
            .is_err()
        );
        assert_eq!(driver.invariant_count, 5);
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_fails_on_baseline_store_prefix_residue() {
        let mut driver = FakeLifecycleDriver {
            store_residue_sample: Some(1),
            ..Default::default()
        };
        let mut clock = FakeLifecycleClock::default();
        assert!(
            super::execute_lifecycle_stress(
                &mut driver,
                &mut clock,
                std::time::Duration::from_millis(super::CANARY_IDLE_DURATION_MS),
            )
            .await
            .is_err()
        );
        assert_eq!(driver.invariant_count, 1);
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_fails_on_final_store_prefix_residue() {
        let final_sample = super::LIFECYCLE_CYCLE_COUNT + 2;
        let mut driver = FakeLifecycleDriver {
            store_residue_sample: Some(final_sample),
            ..Default::default()
        };
        let mut clock = FakeLifecycleClock::default();
        assert!(
            super::execute_lifecycle_stress(
                &mut driver,
                &mut clock,
                std::time::Duration::from_millis(super::CANARY_IDLE_DURATION_MS),
            )
            .await
            .is_err()
        );
        assert_eq!(driver.invariant_count, final_sample);
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_fails_on_tombstone_only_residue() {
        let mut driver = FakeLifecycleDriver {
            tombstone_residue_sample: Some(4),
            ..Default::default()
        };
        let mut clock = FakeLifecycleClock::default();
        assert!(
            super::execute_lifecycle_stress(
                &mut driver,
                &mut clock,
                std::time::Duration::from_millis(super::CANARY_IDLE_DURATION_MS),
            )
            .await
            .is_err()
        );
        assert_eq!(driver.invariant_count, 4);
    }
}
