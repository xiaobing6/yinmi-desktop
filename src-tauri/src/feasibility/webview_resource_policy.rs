use std::sync::atomic::{AtomicU64, Ordering};

use super::signature_webview::is_allowed_gd_navigation;
use url::Url;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub(crate) use macos::MacosResourcePolicyGuard as ResourcePolicyGuard;
#[cfg(windows)]
pub(crate) use windows::WindowsResourcePolicyGuard as ResourcePolicyGuard;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePolicyMetadata {
    pub runtime_version: String,
    pub mode: String,
    pub strong_source_kinds_interface_available: bool,
}

pub(crate) const MACOS_POLICY_IDENTIFIER_PREFIX: &str = "yinmi-gd-signature-";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PolicyStoreSnapshot {
    pub(crate) backend: &'static str,
    pub(crate) identifiers: Vec<String>,
    pub(crate) tombstones: Vec<String>,
}

impl PolicyStoreSnapshot {
    pub(crate) fn has_signature_residue(&self) -> bool {
        self.identifiers
            .iter()
            .chain(&self.tombstones)
            .any(|identifier| identifier.starts_with(MACOS_POLICY_IDENTIFIER_PREFIX))
    }
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn signature_policy_identifiers(
    identifiers: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let mut identifiers = identifiers
        .into_iter()
        .filter(|identifier| identifier.starts_with(MACOS_POLICY_IDENTIFIER_PREFIX))
        .collect::<Vec<_>>();
    identifiers.sort();
    identifiers.dedup();
    identifiers
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(crate) struct StickyCallbackGate {
    claimed: std::sync::atomic::AtomicBool,
    duplicate_fault: std::sync::atomic::AtomicBool,
}

#[cfg(any(target_os = "macos", test))]
impl StickyCallbackGate {
    pub(crate) fn claim(&self) -> bool {
        if self.claimed.swap(true, Ordering::AcqRel) {
            self.duplicate_fault.store(true, Ordering::Release);
            false
        } else {
            true
        }
    }

    pub(crate) fn duplicate_faulted(&self) -> bool {
        self.duplicate_fault.load(Ordering::Acquire)
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MacPolicyIdentity {
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    pub(crate) identifier: String,
}

#[cfg(any(target_os = "macos", test))]
impl MacPolicyIdentity {
    pub(crate) fn new(generation: u64, operation_id: u64) -> Self {
        Self {
            generation,
            operation_id,
            identifier: format!("{MACOS_POLICY_IDENTIFIER_PREFIX}{generation}-{operation_id}"),
        }
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MacCompileState {
    #[default]
    NotStarted,
    InFlight,
    Succeeded,
    Failed,
    UnknownAffinity,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CleanupCompletion {
    #[default]
    Pending,
    VerifiedAbsent,
    Failed,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MacCleanupState {
    compile: MacCompileState,
    cleanup_requested: bool,
    removal_started: bool,
    verification_started: bool,
    verification_attempts: u8,
    completion: CleanupCompletion,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(crate) struct MacCleanupLatch(std::sync::Mutex<MacCleanupState>);

#[cfg(any(target_os = "macos", test))]
const MAX_MACOS_ABSENCE_ATTEMPTS: u8 = 3;

#[cfg(any(target_os = "macos", test))]
impl MacCleanupLatch {
    fn with_state<T>(&self, operation: impl FnOnce(&mut MacCleanupState) -> T) -> T {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operation(&mut state)
    }

    pub(crate) fn mark_compile_started(&self) -> bool {
        self.with_state(|state| {
            if state.compile != MacCompileState::NotStarted {
                return false;
            }
            state.compile = MacCompileState::InFlight;
            true
        })
    }

    pub(crate) fn complete_compile(&self, completion: MacCompileState) -> bool {
        if !matches!(
            completion,
            MacCompileState::Succeeded | MacCompileState::Failed | MacCompileState::UnknownAffinity
        ) {
            return false;
        }
        self.with_state(|state| {
            if state.compile != MacCompileState::InFlight {
                return false;
            }
            state.compile = completion;
            true
        })
    }

    pub(crate) fn cancel_before_compile(&self) -> bool {
        self.with_state(|state| {
            if state.compile != MacCompileState::NotStarted {
                return false;
            }
            state.compile = MacCompileState::Failed;
            true
        })
    }

    pub(crate) fn compile_state(&self) -> MacCompileState {
        self.with_state(|state| state.compile)
    }

    pub(crate) fn request_cleanup(&self) {
        self.with_state(|state| state.cleanup_requested = true);
    }

    pub(crate) fn claim_removal_start(&self) -> bool {
        self.with_state(|state| {
            if !state.cleanup_requested
                || state.removal_started
                || state.completion != CleanupCompletion::Pending
                || !matches!(
                    state.compile,
                    MacCompileState::Succeeded | MacCompileState::UnknownAffinity
                )
            {
                return false;
            }
            state.removal_started = true;
            true
        })
    }

    pub(crate) fn claim_verification_start(&self) -> bool {
        self.with_state(|state| {
            if !state.cleanup_requested
                || state.verification_started
                || state.completion != CleanupCompletion::Pending
                || !(state.removal_started || state.compile == MacCompileState::Failed)
            {
                return false;
            }
            state.verification_started = true;
            state.verification_attempts = state.verification_attempts.saturating_add(1);
            true
        })
    }

    pub(crate) fn retry_verification_after_callback(&self) -> bool {
        self.with_state(|state| {
            if !state.verification_started
                || state.completion != CleanupCompletion::Pending
                || state.verification_attempts >= MAX_MACOS_ABSENCE_ATTEMPTS
            {
                return false;
            }
            state.verification_started = false;
            true
        })
    }

    pub(crate) fn complete_cleanup(&self, completion: CleanupCompletion) -> bool {
        if completion == CleanupCompletion::Pending {
            return false;
        }
        self.with_state(|state| {
            if state.completion != CleanupCompletion::Pending {
                return false;
            }
            state.completion = completion;
            true
        })
    }

    pub(crate) fn cleanup_completion(&self) -> CleanupCompletion {
        self.with_state(|state| state.completion)
    }
}

#[cfg(any(target_os = "macos", test))]
pub(crate) struct LateMacPolicyOwner {
    pub(crate) identity: MacPolicyIdentity,
    pub(crate) latch: std::sync::Arc<MacCleanupLatch>,
    acknowledged: bool,
}

#[cfg(any(target_os = "macos", test))]
impl LateMacPolicyOwner {
    pub(crate) fn new(identity: MacPolicyIdentity, latch: std::sync::Arc<MacCleanupLatch>) -> Self {
        Self {
            identity,
            latch,
            acknowledged: false,
        }
    }

    pub(crate) fn acknowledge_verified_absence(
        &mut self,
        identity: &MacPolicyIdentity,
        latch: &MacCleanupLatch,
    ) -> bool {
        if self.acknowledged
            || &self.identity != identity
            || !std::ptr::eq(self.latch.as_ref(), latch)
            || self.latch.cleanup_completion() != CleanupCompletion::VerifiedAbsent
        {
            return false;
        }
        self.acknowledged = true;
        true
    }

    pub(crate) fn acknowledged(&self) -> bool {
        self.acknowledged
    }
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn release_native_before_late_owner_return<Native, Owner>(
    native: Native,
    owner: Owner,
) -> Owner {
    drop(native);
    owner
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PolicyStoreEnumerationFailure {
    Dispatch,
    CallbackStopped,
    Timeout,
    OffMainCallback,
    MissingIdentifiers,
}

#[cfg(any(target_os = "macos", test))]
impl PolicyStoreEnumerationFailure {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Dispatch => "macOS policy-store dispatch failed",
            Self::CallbackStopped => "macOS policy-store callback stopped",
            Self::Timeout => "macOS policy-store enumeration timed out",
            Self::OffMainCallback => {
                "macOS policy-store enumeration callback arrived off the main thread"
            }
            Self::MissingIdentifiers => "macOS policy-store enumeration returned no identifiers",
        }
    }
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn validate_policy_store_callback(
    on_main_thread: bool,
    identifiers_present: bool,
) -> Result<(), PolicyStoreEnumerationFailure> {
    if !on_main_thread {
        Err(PolicyStoreEnumerationFailure::OffMainCallback)
    } else if !identifiers_present {
        Err(PolicyStoreEnumerationFailure::MissingIdentifiers)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) async fn policy_store_snapshot(
    _app: &tauri::AppHandle<tauri::Wry>,
) -> Result<PolicyStoreSnapshot, crate::feasibility::signature_webview::SignatureError> {
    Ok(PolicyStoreSnapshot {
        backend: "not-applicable-webview2",
        identifiers: Vec::new(),
        tombstones: Vec::new(),
    })
}

#[cfg(target_os = "macos")]
pub(crate) async fn policy_store_snapshot(
    app: &tauri::AppHandle<tauri::Wry>,
) -> Result<PolicyStoreSnapshot, crate::feasibility::signature_webview::SignatureError> {
    macos::policy_store_snapshot(app).await
}

#[cfg(windows)]
pub(crate) fn assert_policy_cleanup_callbacks_clean()
-> Result<(), crate::feasibility::signature_webview::SignatureError> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn assert_policy_cleanup_callbacks_clean()
-> Result<(), crate::feasibility::signature_webview::SignatureError> {
    macos::assert_policy_cleanup_callbacks_clean()
}

pub const MACOS_CONTENT_RULES: &str = r#"[
  {
    "trigger": { "url-filter": ".*" },
    "action": { "type": "block" }
  },
  {
    "trigger": {
      "url-filter": "^https://music\\.gdstudio\\.xyz(:443)?/",
      "if-domain": ["music.gdstudio.xyz"]
    },
    "action": { "type": "ignore-previous-rules" }
  }
]"#;

#[cfg(any(target_os = "macos", test))]
pub(crate) fn macos_content_rules_for(allowed_origin: &Url) -> Result<String, &'static str> {
    if is_allowed_gd_navigation(allowed_origin) {
        return Ok(MACOS_CONTENT_RULES.into());
    }
    if allowed_origin.scheme() != "https"
        || allowed_origin.host_str() != Some("127.0.0.1")
        || allowed_origin.port().is_none()
        || allowed_origin.path() != "/"
        || allowed_origin.query().is_some()
        || allowed_origin.fragment().is_some()
        || !allowed_origin.username().is_empty()
        || allowed_origin.password().is_some()
    {
        return Err("macOS controlled rule requires an exact HTTPS IPv4 loopback origin");
    }
    let port = allowed_origin
        .port()
        .ok_or("macOS controlled rule requires an assigned port")?;
    Ok(format!(
        r#"[
  {{
    "trigger": {{ "url-filter": ".*" }},
    "action": {{ "type": "block" }}
  }},
  {{
    "trigger": {{ "url-filter": "^https://127\\.0\\.0\\.1:{port}/" }},
    "action": {{ "type": "ignore-previous-rules" }}
  }}
]"#
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceRequestDecision {
    Allow,
    Block { canary: bool },
}

#[derive(Default)]
pub struct IsolationCounters {
    blocked_navigations: AtomicU64,
    blocked_new_windows: AtomicU64,
    blocked_downloads: AtomicU64,
    blocked_resource_requests: AtomicU64,
    resource_canary_hits: AtomicU64,
    policy_faults: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolationCounterSnapshot {
    pub blocked_navigations: u64,
    pub blocked_new_windows: u64,
    pub blocked_downloads: u64,
    pub blocked_resource_requests: u64,
    pub resource_canary_hits: u64,
    pub policy_faults: u64,
}

impl IsolationCounters {
    pub fn blocked_navigation(&self) {
        self.blocked_navigations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn blocked_new_window(&self) {
        self.blocked_new_windows.fetch_add(1, Ordering::Relaxed);
    }

    pub fn blocked_download(&self) {
        self.blocked_downloads.fetch_add(1, Ordering::Relaxed);
    }

    pub fn blocked_resource_request(&self, canary: bool) {
        self.blocked_resource_requests
            .fetch_add(1, Ordering::Relaxed);
        if canary {
            self.resource_canary_hits.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn policy_fault(&self) {
        self.policy_faults.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> IsolationCounterSnapshot {
        IsolationCounterSnapshot {
            blocked_navigations: self.blocked_navigations.load(Ordering::Relaxed),
            blocked_new_windows: self.blocked_new_windows.load(Ordering::Relaxed),
            blocked_downloads: self.blocked_downloads.load(Ordering::Relaxed),
            blocked_resource_requests: self.blocked_resource_requests.load(Ordering::Relaxed),
            resource_canary_hits: self.resource_canary_hits.load(Ordering::Relaxed),
            policy_faults: self.policy_faults.load(Ordering::Relaxed),
        }
    }
}

pub fn is_allowed_network_request(url: &Url) -> bool {
    is_allowed_gd_navigation(url)
}

pub(crate) fn is_allowed_network_request_for(allowed_origin: &Url, url: &Url) -> bool {
    let authority_has_userinfo = url
        .as_str()
        .split_once("://")
        .and_then(|(_, remainder)| remainder.split(['/', '?', '#']).next())
        .is_some_and(|authority| authority.contains('@'));
    url.scheme() == allowed_origin.scheme()
        && url.host_str() == allowed_origin.host_str()
        && url.port_or_known_default() == allowed_origin.port_or_known_default()
        && !authority_has_userinfo
        && url.username().is_empty()
        && url.password().is_none()
}

pub fn classify_resource_request(raw: &str) -> ResourceRequestDecision {
    let Ok(url) = Url::parse(raw) else {
        return ResourceRequestDecision::Block { canary: false };
    };
    if is_allowed_network_request(&url) {
        return ResourceRequestDecision::Allow;
    }

    let canary = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    ResourceRequestDecision::Block { canary }
}

pub(crate) fn classify_resource_request_for(
    allowed_origin: &Url,
    raw: &str,
) -> ResourceRequestDecision {
    let Ok(url) = Url::parse(raw) else {
        return ResourceRequestDecision::Block { canary: false };
    };
    if is_allowed_network_request_for(allowed_origin, &url) {
        return ResourceRequestDecision::Allow;
    }
    let canary = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    ResourceRequestDecision::Block { canary }
}

#[cfg(test)]
pub(crate) fn macos_rule_identifier(generation: u64, operation_id: u64) -> String {
    format!("yinmi-gd-signature-{generation}-{operation_id}")
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MacosPolicyPhase {
    Compiling,
    Attached,
    AwaitingLateCompilation,
    RemovingStoreRule,
    Cleaned,
    Faulted,
}

#[cfg(test)]
pub(crate) struct MacosPolicyModel {
    phase: MacosPolicyPhase,
    tombstone: bool,
    cleanup_acknowledged: bool,
    policy_faulted: bool,
}

#[cfg(test)]
impl MacosPolicyModel {
    pub(crate) fn new(generation: u64, operation_id: u64) -> Self {
        let _ = macos_rule_identifier(generation, operation_id);
        Self {
            phase: MacosPolicyPhase::Compiling,
            tombstone: false,
            cleanup_acknowledged: false,
            policy_faulted: false,
        }
    }

    pub(crate) fn phase(&self) -> MacosPolicyPhase {
        self.phase
    }

    pub(crate) fn has_tombstone(&self) -> bool {
        self.tombstone
    }

    pub(crate) fn cleanup_acknowledged(&self) -> bool {
        self.cleanup_acknowledged
    }

    pub(crate) fn policy_faulted(&self) -> bool {
        self.policy_faulted
    }

    pub(crate) fn compilation_succeeded(
        &mut self,
        on_main_thread: bool,
    ) -> Result<(), &'static str> {
        if !on_main_thread || self.phase != MacosPolicyPhase::Compiling {
            return self.fault("content-rule completion was not current on the main thread");
        }
        self.phase = MacosPolicyPhase::Attached;
        Ok(())
    }

    pub(crate) fn compilation_failed(&mut self, on_main_thread: bool) -> Result<(), &'static str> {
        if !on_main_thread {
            return self.fault("content-rule failure was delivered off the main thread");
        }
        self.phase = MacosPolicyPhase::Faulted;
        self.policy_faulted = true;
        self.tombstone = false;
        self.cleanup_acknowledged = true;
        Err("content-rule compilation failed")
    }

    pub(crate) fn begin_destroy(&mut self) {
        match self.phase {
            MacosPolicyPhase::Compiling => {
                self.phase = MacosPolicyPhase::AwaitingLateCompilation;
                self.tombstone = true;
            }
            MacosPolicyPhase::Attached => {
                self.phase = MacosPolicyPhase::RemovingStoreRule;
                self.tombstone = true;
            }
            _ => {}
        }
    }

    pub(crate) fn guard_dropped(&mut self, on_main_thread: bool) -> Result<(), &'static str> {
        if !on_main_thread {
            return self.fault("content-rule guard dropped off the main thread");
        }
        self.begin_destroy();
        Ok(())
    }

    pub(crate) fn late_compilation_completed(
        &mut self,
        returned_rule: bool,
        on_main_thread: bool,
    ) -> Result<(), &'static str> {
        if !on_main_thread || self.phase != MacosPolicyPhase::AwaitingLateCompilation {
            return self.fault("late content-rule completion was unsafe or stale");
        }
        if returned_rule {
            self.phase = MacosPolicyPhase::RemovingStoreRule;
        } else {
            self.phase = MacosPolicyPhase::Cleaned;
            self.tombstone = false;
            self.cleanup_acknowledged = true;
        }
        Ok(())
    }

    pub(crate) fn store_removal_completed(
        &mut self,
        succeeded: bool,
        on_main_thread: bool,
    ) -> Result<(), &'static str> {
        if !on_main_thread || !succeeded || self.phase != MacosPolicyPhase::RemovingStoreRule {
            return self.fault("content-rule store removal failed or was unsafe");
        }
        self.phase = MacosPolicyPhase::Cleaned;
        self.tombstone = false;
        self.cleanup_acknowledged = true;
        Ok(())
    }

    fn fault(&mut self, message: &'static str) -> Result<(), &'static str> {
        self.phase = MacosPolicyPhase::Faulted;
        self.policy_faulted = true;
        Err(message)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        CleanupCompletion, LateMacPolicyOwner, MACOS_CONTENT_RULES, MacCleanupLatch,
        MacCompileState, MacPolicyIdentity, MacosPolicyModel, MacosPolicyPhase,
        PolicyStoreEnumerationFailure, ResourceRequestDecision, StickyCallbackGate,
        classify_resource_request, is_allowed_network_request, macos_content_rules_for,
        macos_rule_identifier, release_native_before_late_owner_return,
        signature_policy_identifiers, validate_policy_store_callback,
    };
    use serde_json::json;
    use url::Url;

    #[test]
    fn signature_webview_network_policy_allows_only_official_https_requests() {
        for allowed in [
            "https://music.gdstudio.xyz/",
            "https://music.gdstudio.xyz/api.php",
            "https://music.gdstudio.xyz:443/js/player.js?v=20260616",
        ] {
            assert!(
                is_allowed_network_request(&Url::parse(allowed).unwrap()),
                "expected allow: {allowed}"
            );
        }

        for denied in [
            "http://music.gdstudio.xyz/",
            "https://music.gdstudio.xyz:444/",
            "https://user:pass@music.gdstudio.xyz/",
            "https://music.gdstudio.xyz.evil.example/asset.js",
            "https://evil.example/frame.html",
            "http://ipc.localhost/",
            "http://127.0.0.1:31337/canary",
            "http://[::1]:31337/canary",
            "https://localhost/canary",
            "file:///tmp/canary",
            "data:text/plain,canary",
        ] {
            assert!(
                !is_allowed_network_request(&Url::parse(denied).unwrap()),
                "expected deny: {denied}"
            );
        }
    }

    #[test]
    fn signature_webview_resource_handler_fails_closed_and_marks_canaries() {
        assert_eq!(
            classify_resource_request("https://music.gdstudio.xyz/api.php"),
            ResourceRequestDecision::Allow
        );
        for raw in ["not a url", "http://ipc.localhost/", "file:///tmp/probe"] {
            assert_eq!(
                classify_resource_request(raw),
                ResourceRequestDecision::Block { canary: false }
            );
        }
        for raw in [
            "http://127.0.0.1:31337/canary",
            "http://[::1]:31337/canary",
            "https://localhost/canary",
        ] {
            assert_eq!(
                classify_resource_request(raw),
                ResourceRequestDecision::Block { canary: true }
            );
        }
    }

    #[test]
    fn signature_webview_macos_rule_is_an_exact_domain_exception_without_wildcards() {
        let rules: serde_json::Value = serde_json::from_str(MACOS_CONTENT_RULES).unwrap();
        assert_eq!(
            rules,
            json!([
                {
                    "trigger": { "url-filter": ".*" },
                    "action": { "type": "block" }
                },
                {
                    "trigger": {
                        "url-filter": "^https://music\\.gdstudio\\.xyz(:443)?/",
                        "if-domain": ["music.gdstudio.xyz"]
                    },
                    "action": { "type": "ignore-previous-rules" }
                }
            ])
        );
    }

    #[test]
    fn signature_webview_macos_controlled_rule_allows_only_the_assigned_tls_origin() {
        let allowed = Url::parse("https://127.0.0.1:54321/").unwrap();
        let rules: serde_json::Value =
            serde_json::from_str(&macos_content_rules_for(&allowed).unwrap()).unwrap();
        assert_eq!(
            rules,
            json!([
                {
                    "trigger": { "url-filter": ".*" },
                    "action": { "type": "block" }
                },
                {
                    "trigger": { "url-filter": "^https://127\\.0\\.0\\.1:54321/" },
                    "action": { "type": "ignore-previous-rules" }
                }
            ])
        );
        for rejected in [
            "http://127.0.0.1:54321/",
            "https://localhost:54321/",
            "https://127.0.0.1/",
            "https://127.0.0.1:54321/path",
        ] {
            assert!(macos_content_rules_for(&Url::parse(rejected).unwrap()).is_err());
        }
    }

    #[test]
    fn signature_webview_macos_rule_ids_and_tombstones_are_generation_specific() {
        assert_eq!(macos_rule_identifier(7, 11), "yinmi-gd-signature-7-11");
        let mut pending = MacosPolicyModel::new(7, 11);
        assert_eq!(pending.phase(), MacosPolicyPhase::Compiling);
        pending.begin_destroy();
        assert_eq!(pending.phase(), MacosPolicyPhase::AwaitingLateCompilation);
        assert!(pending.has_tombstone());
        assert!(!pending.cleanup_acknowledged());

        assert!(pending.late_compilation_completed(true, true).is_ok());
        assert_eq!(pending.phase(), MacosPolicyPhase::RemovingStoreRule);
        assert!(pending.has_tombstone());
        pending.store_removal_completed(true, true).unwrap();
        assert_eq!(pending.phase(), MacosPolicyPhase::Cleaned);
        assert!(!pending.has_tombstone());
        assert!(pending.cleanup_acknowledged());
    }

    #[test]
    fn signature_webview_macos_pending_and_ready_cleanup_fail_closed() {
        let mut compile_failure = MacosPolicyModel::new(1, 2);
        assert!(compile_failure.compilation_failed(true).is_err());
        assert_eq!(compile_failure.phase(), MacosPolicyPhase::Faulted);
        assert!(compile_failure.policy_faulted());
        assert!(compile_failure.cleanup_acknowledged());
        assert!(!compile_failure.has_tombstone());

        let mut ready = MacosPolicyModel::new(3, 4);
        ready.compilation_succeeded(true).unwrap();
        assert_eq!(ready.phase(), MacosPolicyPhase::Attached);
        ready.begin_destroy();
        assert_eq!(ready.phase(), MacosPolicyPhase::RemovingStoreRule);
        assert!(ready.has_tombstone());
        assert!(ready.store_removal_completed(false, true).is_err());
        assert_eq!(ready.phase(), MacosPolicyPhase::Faulted);
        assert!(ready.has_tombstone());
        assert!(!ready.cleanup_acknowledged());
    }

    #[test]
    fn signature_webview_macos_guard_drop_is_main_thread_idempotent_and_not_silent() {
        let mut guard = MacosPolicyModel::new(5, 6);
        guard.compilation_succeeded(true).unwrap();
        guard.guard_dropped(true).unwrap();
        assert_eq!(guard.phase(), MacosPolicyPhase::RemovingStoreRule);
        assert!(guard.has_tombstone());
        guard.guard_dropped(true).unwrap();
        assert_eq!(guard.phase(), MacosPolicyPhase::RemovingStoreRule);

        let mut unsafe_guard = MacosPolicyModel::new(7, 8);
        unsafe_guard.compilation_succeeded(true).unwrap();
        assert!(unsafe_guard.guard_dropped(false).is_err());
        assert!(unsafe_guard.policy_faulted());

        let macos_source = include_str!("webview_resource_policy/macos.rs");
        let host_source = include_str!("signature_host.rs");
        assert!(macos_source.contains("impl Drop for MacosResourcePolicyGuard"));
        assert!(macos_source.contains("into_late_owner_on_ui"));
        assert!(macos_source.contains("release_native_before_late_owner_return"));
        assert!(macos_source.contains("MainThreadMarker::new()"));
        assert!(macos_source.contains("getAvailableContentRuleListIdentifiers"));
        assert!(macos_source.contains("late_policy_tombstone_identifiers"));
        assert!(
            include_str!("webview_resource_policy.rs")
                .contains("policy-store enumeration timed out")
        );
        assert!(macos_source.contains("schedule_macos_absence_retry"));
        assert!(macos_source.contains("retry_verification_after_callback"));
        assert!(!macos_source.contains("signature policy callback UI dispatch failed"));

        let publish = host_source
            .find("pending.policy_build.native = Some(policy);")
            .expect("pending native owner must be published before compilation");
        let compile = host_source[publish..]
            .find(".compile_invocation()")
            .map(|offset| publish + offset)
            .expect("compile invocation must be prepared from the published owner");
        let invoke = host_source[compile..]
            .find("invocation.invoke()")
            .map(|offset| compile + offset)
            .expect("compile must start only after the slot borrow is released");
        assert!(publish < compile && compile < invoke);

        let removal = macos_source
            .split("pub(crate) fn begin_macos_store_removal_on_ui")
            .nth(1)
            .expect("removal implementation must exist");
        assert!(
            removal
                .find("WKContentRuleListStore::defaultStore")
                .unwrap()
                < removal.find("claim_removal_start").unwrap(),
            "store preflight must succeed before consuming the removal claim"
        );
        let verification = macos_source
            .split("fn begin_macos_absence_verification_on_ui")
            .nth(1)
            .expect("absence verification implementation must exist");
        assert!(
            verification
                .find("WKContentRuleListStore::defaultStore")
                .unwrap()
                < verification.find("claim_verification_start").unwrap(),
            "store preflight must succeed before consuming the verification claim"
        );
    }

    #[test]
    fn signature_webview_macos_store_callback_duplicate_is_sticky() {
        let gate = StickyCallbackGate::default();
        assert!(gate.claim());
        assert!(!gate.duplicate_faulted());
        assert!(!gate.claim());
        assert!(gate.duplicate_faulted());
        assert!(!gate.claim());
        assert!(gate.duplicate_faulted());
    }

    #[test]
    fn cleanup_verified_before_owner_publish_is_latched_then_acknowledged_once() {
        let latch = Arc::new(MacCleanupLatch::default());
        latch.request_cleanup();
        assert_eq!(latch.compile_state(), MacCompileState::NotStarted);
        assert!(latch.cancel_before_compile());
        assert_eq!(latch.compile_state(), MacCompileState::Failed);
        assert!(latch.complete_cleanup(CleanupCompletion::VerifiedAbsent));
        assert_eq!(
            latch.cleanup_completion(),
            CleanupCompletion::VerifiedAbsent
        );

        let identity = MacPolicyIdentity::new(17, 29);
        let mut owner = LateMacPolicyOwner::new(identity.clone(), Arc::clone(&latch));
        assert!(owner.acknowledge_verified_absence(&identity, &latch));
        assert!(!owner.acknowledge_verified_absence(&identity, &latch));
        assert!(owner.acknowledged());
        assert_eq!(
            latch.cleanup_completion(),
            CleanupCompletion::VerifiedAbsent
        );
    }

    #[test]
    fn duplicate_compile_callback_cannot_steal_cleanup_or_start_a_second_removal() {
        let latch = MacCleanupLatch::default();
        assert!(latch.mark_compile_started());
        assert!(latch.complete_compile(MacCompileState::Succeeded));
        assert!(!latch.complete_compile(MacCompileState::Succeeded));
        latch.request_cleanup();
        assert!(latch.claim_removal_start());
        assert!(!latch.claim_removal_start());
        assert!(latch.complete_cleanup(CleanupCompletion::VerifiedAbsent));
        assert!(!latch.complete_cleanup(CleanupCompletion::VerifiedAbsent));
        assert_eq!(
            latch.cleanup_completion(),
            CleanupCompletion::VerifiedAbsent
        );
    }

    #[test]
    fn macos_pending_native_values_drop_before_late_owner_can_be_published() {
        struct NativeDropSpy(Arc<std::sync::Mutex<Vec<&'static str>>>);

        impl Drop for NativeDropSpy {
            fn drop(&mut self) {
                self.0.lock().unwrap().push("native-dropped");
            }
        }

        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let owner = release_native_before_late_owner_return(
            NativeDropSpy(Arc::clone(&events)),
            "late-owner",
        );
        events.lock().unwrap().push("owner-published");

        assert_eq!(owner, "late-owner");
        assert_eq!(
            *events.lock().unwrap(),
            ["native-dropped", "owner-published"]
        );
    }

    #[test]
    fn macos_identifier_presence_retries_are_bounded_before_cleanup_faults() {
        let latch = MacCleanupLatch::default();
        assert!(latch.mark_compile_started());
        assert!(latch.complete_compile(MacCompileState::Succeeded));
        latch.request_cleanup();
        assert!(latch.claim_removal_start());

        for attempt in 1..=3 {
            assert!(latch.claim_verification_start());
            if attempt < 3 {
                assert!(latch.retry_verification_after_callback());
            } else {
                assert!(!latch.retry_verification_after_callback());
            }
        }
        assert_eq!(latch.cleanup_completion(), CleanupCompletion::Pending);
        assert!(latch.complete_cleanup(CleanupCompletion::Failed));
        assert_eq!(latch.cleanup_completion(), CleanupCompletion::Failed);
    }

    #[test]
    fn macos_off_main_compile_affinity_fault_can_still_own_exact_cleanup() {
        let latch = MacCleanupLatch::default();
        assert!(latch.mark_compile_started());
        assert!(latch.complete_compile(MacCompileState::UnknownAffinity));
        latch.request_cleanup();
        assert!(latch.claim_removal_start());
        assert!(!latch.claim_removal_start());
        assert_eq!(latch.cleanup_completion(), CleanupCompletion::Pending);
    }

    #[test]
    fn signature_webview_macos_store_filter_ignores_unrelated_identifiers() {
        assert_eq!(
            signature_policy_identifiers([
                "shared-unrelated-rule".to_string(),
                "yinmi-gd-signature-9-2".to_string(),
                "yinmi-gd-signature-9-1".to_string(),
                "yinmi-gd-signature-9-2".to_string(),
            ]),
            [
                "yinmi-gd-signature-9-1".to_string(),
                "yinmi-gd-signature-9-2".to_string(),
            ]
        );
    }

    #[test]
    fn signature_webview_macos_store_enumeration_failures_are_closed_and_typed() {
        assert_eq!(
            validate_policy_store_callback(false, true),
            Err(PolicyStoreEnumerationFailure::OffMainCallback)
        );
        assert_eq!(
            validate_policy_store_callback(true, false),
            Err(PolicyStoreEnumerationFailure::MissingIdentifiers)
        );
        assert_eq!(validate_policy_store_callback(true, true), Ok(()));
        for failure in [
            PolicyStoreEnumerationFailure::Dispatch,
            PolicyStoreEnumerationFailure::CallbackStopped,
            PolicyStoreEnumerationFailure::Timeout,
        ] {
            assert!(failure.message().starts_with("macOS policy-store"));
        }
    }

    #[test]
    fn signature_webview_macos_late_compile_error_needs_no_store_removal() {
        let mut pending = MacosPolicyModel::new(13, 14);
        pending.begin_destroy();
        pending.late_compilation_completed(false, true).unwrap();
        assert_eq!(pending.phase(), MacosPolicyPhase::Cleaned);
        assert!(pending.cleanup_acknowledged());
        assert!(!pending.has_tombstone());
    }

    #[test]
    fn signature_webview_macos_callbacks_require_main_thread_marshalling() {
        let mut pending = MacosPolicyModel::new(9, 10);
        assert!(pending.compilation_succeeded(false).is_err());
        assert_eq!(pending.phase(), MacosPolicyPhase::Faulted);
        assert!(pending.policy_faulted());

        let mut late = MacosPolicyModel::new(11, 12);
        late.begin_destroy();
        assert!(late.late_compilation_completed(false, false).is_err());
        assert_eq!(late.phase(), MacosPolicyPhase::Faulted);
        assert!(late.has_tombstone());
    }
}
