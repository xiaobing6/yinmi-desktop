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

#[cfg(any(target_os = "macos", test))]
pub(crate) const MACOS_POLICY_IDENTIFIER_PREFIX: &str = "yinmi-gd-signature-";

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

#[cfg(windows)]
pub(crate) fn assert_policy_cleanup_callbacks_clean()
-> Result<(), crate::signature::signature_webview::SignatureError> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn assert_policy_cleanup_callbacks_clean()
-> Result<(), crate::signature::signature_webview::SignatureError> {
    macos::assert_policy_cleanup_callbacks_clean()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn assert_policy_cleanup_callbacks_clean()
-> Result<(), crate::signature::signature_webview::SignatureError> {
    Ok(())
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
#[cfg(windows)]
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
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn mac_cleanup_latch_enforces_compile_cleanup_and_retry_order() {
        let latch = MacCleanupLatch::default();
        assert_eq!(latch.compile_state(), MacCompileState::NotStarted);
        assert!(!latch.complete_compile(MacCompileState::Succeeded));
        assert!(latch.mark_compile_started());
        assert!(!latch.mark_compile_started());
        assert!(!latch.complete_compile(MacCompileState::InFlight));
        assert!(latch.complete_compile(MacCompileState::Succeeded));
        assert!(!latch.cancel_before_compile());

        latch.request_cleanup();
        assert!(latch.claim_removal_start());
        assert!(!latch.claim_removal_start());
        for attempt in 0..MAX_MACOS_ABSENCE_ATTEMPTS {
            assert!(latch.claim_verification_start());
            if attempt + 1 < MAX_MACOS_ABSENCE_ATTEMPTS {
                assert!(latch.retry_verification_after_callback());
            }
        }
        assert!(!latch.retry_verification_after_callback());
        assert!(latch.complete_cleanup(CleanupCompletion::VerifiedAbsent));
        assert!(!latch.complete_cleanup(CleanupCompletion::Failed));
        assert_eq!(
            latch.cleanup_completion(),
            CleanupCompletion::VerifiedAbsent
        );

        let failed = MacCleanupLatch::default();
        assert!(failed.cancel_before_compile());
        failed.request_cleanup();
        assert!(!failed.claim_removal_start());
        assert!(failed.claim_verification_start());
        assert!(failed.complete_cleanup(CleanupCompletion::Failed));

        let unknown = MacCleanupLatch::default();
        assert!(unknown.mark_compile_started());
        assert!(unknown.complete_compile(MacCompileState::UnknownAffinity));
    }

    #[test]
    fn mac_policy_identity_owner_and_callback_gate_are_strict() {
        let gate = StickyCallbackGate::default();
        assert!(gate.claim());
        assert!(!gate.claim());
        assert!(gate.duplicate_faulted());

        let identity = MacPolicyIdentity::new(7, 11);
        assert_eq!(identity.identifier, "yinmi-gd-signature-7-11");
        let latch = Arc::new(MacCleanupLatch::default());
        assert!(latch.cancel_before_compile());
        latch.request_cleanup();
        assert!(latch.claim_verification_start());
        assert!(latch.complete_cleanup(CleanupCompletion::VerifiedAbsent));

        let mut owner = LateMacPolicyOwner::new(identity.clone(), Arc::clone(&latch));
        assert!(!owner.acknowledge_verified_absence(
            &MacPolicyIdentity::new(8, 11),
            latch.as_ref(),
        ));
        assert!(owner.acknowledge_verified_absence(&identity, latch.as_ref()));
        assert!(owner.acknowledged());
        assert!(!owner.acknowledge_verified_absence(&identity, latch.as_ref()));

        let returned = release_native_before_late_owner_return(String::from("native"), 42_u8);
        assert_eq!(returned, 42);
    }

    #[test]
    fn mac_content_rules_only_accept_controlled_origins() {
        let gd = Url::parse("https://music.gdstudio.xyz/").expect("fixed URL is valid");
        assert_eq!(macos_content_rules_for(&gd), Ok(MACOS_CONTENT_RULES.into()));

        let loopback = Url::parse("https://127.0.0.1:43117/").expect("fixed URL is valid");
        let rules = macos_content_rules_for(&loopback).expect("loopback origin is controlled");
        assert!(rules.contains("127\\\\.0\\\\.0\\\\.1:43117"));

        let invalid = Url::parse("http://127.0.0.1:43117/").expect("fixed URL is valid");
        assert!(macos_content_rules_for(&invalid).is_err());
    }
}
