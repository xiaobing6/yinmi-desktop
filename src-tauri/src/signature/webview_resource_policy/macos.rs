use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use block2::RcBlock;
use objc2::{MainThreadMarker, rc::Retained};
use objc2_foundation::{NSArray, NSError, NSString};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKUserContentController, WKWebViewConfiguration,
    WKWebsiteDataStore,
};
use url::Url;
use wry::WebViewBuilderExtMacos;

use super::{
    CleanupCompletion, IsolationCounters, LateMacPolicyOwner, MacCleanupLatch, MacCompileState,
    MacPolicyIdentity, ResourcePolicyMetadata, StickyCallbackGate, macos_content_rules_for,
};
use crate::signature::{
    signature_host::{
        complete_macos_compile_on_ui, fail_macos_compile_affinity_on_ui,
        reconcile_macos_cleanup_on_ui,
    },
    signature_webview::{SignatureError, add_late_policy_tombstone},
};

pub(crate) type MacosRuleResult = Result<Retained<WKContentRuleList>, String>;
type CompileBlock = RcBlock<dyn Fn(*mut WKContentRuleList, *mut NSError)>;

static STORE_ENUMERATION_DUPLICATE_FAULT: AtomicBool = AtomicBool::new(false);
static PENDING_POLICY_CLEANUP_FAULT: AtomicBool = AtomicBool::new(false);

pub(crate) fn mark_policy_cleanup_fault() {
    PENDING_POLICY_CLEANUP_FAULT.store(true, Ordering::Release);
}

pub(crate) struct PendingMacosResourcePolicy {
    identity: MacPolicyIdentity,
    app: tauri::AppHandle<tauri::Wry>,
    configuration: Option<Retained<WKWebViewConfiguration>>,
    controller: Retained<WKUserContentController>,
    store: Retained<WKContentRuleListStore>,
    identifier_ns: Retained<NSString>,
    rules: Retained<NSString>,
    runtime_version: String,
    latch: Arc<MacCleanupLatch>,
}

pub(crate) struct MacosCompileInvocation {
    store: Retained<WKContentRuleListStore>,
    identifier: Retained<NSString>,
    rules: Retained<NSString>,
    completion: CompileBlock,
}

impl MacosCompileInvocation {
    pub(crate) fn invoke(self) {
        unsafe {
            self.store
                .compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                    Some(&self.identifier),
                    Some(&self.rules),
                    Some(&self.completion),
                );
        }
    }
}

impl PendingMacosResourcePolicy {
    pub(crate) fn prepare(
        app: tauri::AppHandle<tauri::Wry>,
        generation: u64,
        operation_id: u64,
        allowed_origin: &Url,
    ) -> Result<Self, SignatureError> {
        let marker = MainThreadMarker::new().ok_or_else(|| {
            SignatureError::Webview("macOS signature policy must start on the main thread".into())
        })?;
        let configuration = nonpersistent_configuration(marker);
        let controller = unsafe { configuration.userContentController() };
        let store = unsafe { WKContentRuleListStore::defaultStore(marker) }.ok_or_else(|| {
            SignatureError::Webview("macOS content-rule store is unavailable".into())
        })?;
        let identity = MacPolicyIdentity::new(generation, operation_id);
        let identifier_ns = NSString::from_str(&identity.identifier);
        let rule_source = macos_content_rules_for(allowed_origin)
            .map_err(|message| SignatureError::Webview(message.into()))?;
        let rules = NSString::from_str(&rule_source);
        let runtime_version = wry::webview_version()
            .map_err(|_| SignatureError::Webview("WebKit version query failed".into()))?;
        Ok(Self {
            identity,
            app,
            configuration: Some(configuration),
            controller,
            store,
            identifier_ns,
            rules,
            runtime_version,
            latch: Arc::new(MacCleanupLatch::default()),
        })
    }

    pub(crate) fn compile_invocation(&mut self) -> Result<MacosCompileInvocation, SignatureError> {
        if !self.latch.mark_compile_started() {
            return Err(SignatureError::Webview(
                "macOS content-rule compilation was already started".into(),
            ));
        }
        let callback_app = self.app.clone();
        let callback_identity = self.identity.clone();
        let callback_latch = Arc::clone(&self.latch);
        let callback_gate = Arc::new(StickyCallbackGate::default());
        let completion: CompileBlock =
            RcBlock::new(move |rule: *mut WKContentRuleList, error: *mut NSError| {
                if !callback_gate.claim() {
                    mark_policy_cleanup_fault();
                    eprintln!("macOS content-rule callback arrived more than once");
                    return;
                }
                if MainThreadMarker::new().is_none() {
                    callback_latch.request_cleanup();
                    if !callback_latch.complete_compile(MacCompileState::UnknownAffinity) {
                        mark_policy_cleanup_fault();
                        return;
                    }
                    let dispatch_app = callback_app.clone();
                    let dispatch_identity = callback_identity.clone();
                    let dispatch_latch = Arc::clone(&callback_latch);
                    if callback_app
                        .run_on_main_thread(move || {
                            fail_macos_compile_affinity_on_ui(
                                dispatch_identity,
                                dispatch_latch,
                                &dispatch_app,
                            );
                        })
                        .is_err()
                    {
                        callback_latch.complete_cleanup(CleanupCompletion::Failed);
                        mark_policy_cleanup_fault();
                        eprintln!("macOS off-main policy callback dispatch failed");
                    }
                    return;
                }
                let outcome: MacosRuleResult = if rule.is_null() || !error.is_null() {
                    Err("macOS content-rule compilation failed".into())
                } else {
                    unsafe { Retained::retain(rule) }
                        .ok_or_else(|| "macOS content-rule callback returned no rule".into())
                };
                let compile_state = if outcome.is_ok() {
                    MacCompileState::Succeeded
                } else {
                    MacCompileState::Failed
                };
                if !callback_latch.complete_compile(compile_state) {
                    mark_policy_cleanup_fault();
                    return;
                }
                complete_macos_compile_on_ui(
                    callback_identity.clone(),
                    Arc::clone(&callback_latch),
                    outcome,
                    &callback_app,
                );
            });
        Ok(MacosCompileInvocation {
            store: self.store.clone(),
            identifier: self.identifier_ns.clone(),
            rules: self.rules.clone(),
            completion,
        })
    }

    pub(crate) fn matches(&self, identity: &MacPolicyIdentity, latch: &MacCleanupLatch) -> bool {
        &self.identity == identity && std::ptr::eq(self.latch.as_ref(), latch)
    }

    pub(crate) fn take_configuration(
        &mut self,
    ) -> Result<Retained<WKWebViewConfiguration>, SignatureError> {
        self.configuration.take().ok_or_else(|| {
            SignatureError::Webview("macOS signature configuration was already consumed".into())
        })
    }

    pub(crate) fn attach(&self, rule: &WKContentRuleList) {
        unsafe { self.controller.addContentRuleList(rule) };
    }

    pub(crate) fn detach(&self, rule: &WKContentRuleList) {
        unsafe { self.controller.removeContentRuleList(rule) };
    }

    pub(crate) fn into_guard(
        self,
        rule: Retained<WKContentRuleList>,
        counters: Arc<IsolationCounters>,
    ) -> MacosResourcePolicyGuard {
        MacosResourcePolicyGuard {
            identity: self.identity,
            app: self.app,
            controller: self.controller,
            rule: Some(rule),
            metadata: ResourcePolicyMetadata {
                runtime_version: self.runtime_version,
                mode: "wk-content-rule-list-exact-origin".into(),
                strong_source_kinds_interface_available: false,
            },
            _counters: counters,
            latch: self.latch,
            consumed: false,
        }
    }

    pub(crate) fn into_late_owner_on_ui(mut self) -> Result<LateMacPolicyOwner, SignatureError> {
        MainThreadMarker::new().ok_or_else(|| {
            SignatureError::Webview("macOS pending policy must be released on the UI thread".into())
        })?;
        drop(self.configuration.take());
        self.latch.request_cleanup();
        if self.latch.compile_state() == MacCompileState::NotStarted {
            if !self.latch.cancel_before_compile()
                || !self
                    .latch
                    .complete_cleanup(CleanupCompletion::VerifiedAbsent)
            {
                return Err(SignatureError::Webview(
                    "macOS pending policy cancellation latch was inconsistent".into(),
                ));
            }
        }
        let owner = LateMacPolicyOwner::new(self.identity.clone(), Arc::clone(&self.latch));
        Ok(super::release_native_before_late_owner_return(self, owner))
    }
}

fn nonpersistent_configuration(marker: MainThreadMarker) -> Retained<WKWebViewConfiguration> {
    let configuration = unsafe { WKWebViewConfiguration::new(marker) };
    let data_store = unsafe { WKWebsiteDataStore::nonPersistentDataStore(marker) };
    unsafe { configuration.setWebsiteDataStore(&data_store) };
    configuration
}


pub(crate) fn assert_policy_cleanup_callbacks_clean() -> Result<(), SignatureError> {
    if STORE_ENUMERATION_DUPLICATE_FAULT.load(Ordering::Acquire)
        || PENDING_POLICY_CLEANUP_FAULT.load(Ordering::Acquire)
    {
        Err(SignatureError::Webview(
            "macOS policy cleanup has a sticky callback or dispatch fault".into(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) struct MacosResourcePolicyGuard {
    identity: MacPolicyIdentity,
    app: tauri::AppHandle<tauri::Wry>,
    controller: Retained<WKUserContentController>,
    rule: Option<Retained<WKContentRuleList>>,
    metadata: ResourcePolicyMetadata,
    _counters: Arc<IsolationCounters>,
    latch: Arc<MacCleanupLatch>,
    consumed: bool,
}

impl MacosResourcePolicyGuard {
    pub(crate) fn metadata(&self) -> &ResourcePolicyMetadata {
        &self.metadata
    }

    fn take_late_owner_on_ui(&mut self) -> Result<LateMacPolicyOwner, SignatureError> {
        MainThreadMarker::new().ok_or_else(|| {
            SignatureError::Webview("macOS policy guard must be released on the UI thread".into())
        })?;
        if self.consumed {
            return Err(SignatureError::Webview(
                "macOS policy guard cleanup owner was already consumed".into(),
            ));
        }
        self.consumed = true;
        if let Some(rule) = self.rule.take() {
            unsafe { self.controller.removeContentRuleList(&rule) };
        }
        self.latch.request_cleanup();
        Ok(LateMacPolicyOwner::new(
            self.identity.clone(),
            Arc::clone(&self.latch),
        ))
    }

    pub(crate) fn into_late_owner_on_ui(mut self) -> Result<LateMacPolicyOwner, SignatureError> {
        self.take_late_owner_on_ui()
    }
}

impl Drop for MacosResourcePolicyGuard {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        add_late_policy_tombstone(
            self.identity.generation,
            self.identity.operation_id,
            self.identity.identifier.clone(),
        );
        mark_policy_cleanup_fault();
        if MainThreadMarker::new().is_none() {
            eprintln!(
                "macOS signature content-rule guard dropped off the UI thread; cleanup remains tombstoned"
            );
            return;
        }
        match self.take_late_owner_on_ui() {
            Ok(owner) => {
                if let Err(error) =
                    begin_macos_store_removal_on_ui(&self.app, &owner.identity, &owner.latch)
                {
                    eprintln!("macOS signature content-rule Drop cleanup failed: {error}");
                }
            }
            Err(error) => {
                eprintln!("macOS signature content-rule Drop cleanup failed: {error}")
            }
        }
    }
}

pub(crate) fn apply_configuration<'a>(
    builder: wry::WebViewBuilder<'a>,
    configuration: Retained<WKWebViewConfiguration>,
) -> wry::WebViewBuilder<'a> {
    builder.with_webview_configuration(configuration)
}

fn fail_cleanup_latch(latch: &MacCleanupLatch, message: &str) {
    let _ = latch.complete_cleanup(CleanupCompletion::Failed);
    mark_policy_cleanup_fault();
    eprintln!("{message}");
}

pub(crate) fn begin_macos_store_removal_on_ui(
    app: &tauri::AppHandle<tauri::Wry>,
    identity: &MacPolicyIdentity,
    latch: &Arc<MacCleanupLatch>,
) -> Result<bool, SignatureError> {
    let marker = MainThreadMarker::new().ok_or_else(|| {
        SignatureError::Webview("macOS rule removal must start on the main thread".into())
    })?;
    let store = unsafe { WKContentRuleListStore::defaultStore(marker) }.ok_or_else(|| {
        SignatureError::Webview("macOS content-rule store is unavailable for removal".into())
    })?;
    let identifier_ns = NSString::from_str(&identity.identifier);
    if !latch.claim_removal_start() {
        return Ok(false);
    }
    let callback_app = app.clone();
    let callback_identity = identity.clone();
    let callback_latch = Arc::clone(latch);
    let completion_gate = Arc::new(StickyCallbackGate::default());
    let completion: RcBlock<dyn Fn(*mut NSError)> = RcBlock::new(move |error: *mut NSError| {
        if !completion_gate.claim() {
            fail_cleanup_latch(
                &callback_latch,
                "macOS rule-removal callback arrived more than once",
            );
            return;
        }
        if MainThreadMarker::new().is_none() {
            let dispatch_app = callback_app.clone();
            let dispatch_identity = callback_identity.clone();
            let dispatch_latch = Arc::clone(&callback_latch);
            if callback_app
                .run_on_main_thread(move || {
                    if let Err(error) = begin_macos_absence_verification_on_ui(
                        &dispatch_app,
                        &dispatch_identity,
                        &dispatch_latch,
                    ) {
                        fail_cleanup_latch(
                            &dispatch_latch,
                            &format!("macOS rule absence verification failed: {error}"),
                        );
                    }
                })
                .is_err()
            {
                fail_cleanup_latch(
                    &callback_latch,
                    "macOS off-main removal callback dispatch failed",
                );
            }
            return;
        }
        if !error.is_null() {
            eprintln!("macOS content-rule removal returned an error; verifying store absence");
        }
        if let Err(error) = begin_macos_absence_verification_on_ui(
            &callback_app,
            &callback_identity,
            &callback_latch,
        ) {
            fail_cleanup_latch(
                &callback_latch,
                &format!("macOS rule absence verification failed: {error}"),
            );
        }
    });
    unsafe {
        store.removeContentRuleListForIdentifier_completionHandler(
            Some(&identifier_ns),
            Some(&completion),
        );
    }
    Ok(true)
}

fn begin_macos_absence_verification_on_ui(
    app: &tauri::AppHandle<tauri::Wry>,
    identity: &MacPolicyIdentity,
    latch: &Arc<MacCleanupLatch>,
) -> Result<bool, SignatureError> {
    let marker = MainThreadMarker::new().ok_or_else(|| {
        SignatureError::Webview(
            "macOS rule absence verification must start on the main thread".into(),
        )
    })?;
    let store = unsafe { WKContentRuleListStore::defaultStore(marker) }.ok_or_else(|| {
        SignatureError::Webview(
            "macOS content-rule store is unavailable for absence verification".into(),
        )
    })?;
    if !latch.claim_verification_start() {
        return Ok(false);
    }
    let callback_app = app.clone();
    let callback_identity = identity.clone();
    let callback_latch = Arc::clone(latch);
    let completion_gate = Arc::new(StickyCallbackGate::default());
    let completion: RcBlock<dyn Fn(*mut NSArray<NSString>)> =
        RcBlock::new(move |identifiers: *mut NSArray<NSString>| {
            if !completion_gate.claim() {
                fail_cleanup_latch(
                    &callback_latch,
                    "macOS rule absence callback arrived more than once",
                );
                return;
            }
            if MainThreadMarker::new().is_none() {
                schedule_macos_absence_retry(&callback_app, &callback_identity, &callback_latch);
                return;
            }
            if identifiers.is_null() {
                fail_cleanup_latch(
                    &callback_latch,
                    "macOS rule absence verification returned no identifiers",
                );
                return;
            }
            let identifiers = unsafe { &*identifiers };
            let still_present = (0..identifiers.count()).any(|index| {
                identifiers.objectAtIndex(index).to_string() == callback_identity.identifier
            });
            if still_present {
                schedule_macos_absence_retry(&callback_app, &callback_identity, &callback_latch);
                return;
            }
            if !callback_latch.complete_cleanup(CleanupCompletion::VerifiedAbsent) {
                fail_cleanup_latch(
                    &callback_latch,
                    "macOS rule absence completion was duplicated or stale",
                );
                return;
            }
            reconcile_macos_cleanup_on_ui(
                callback_identity.clone(),
                Arc::clone(&callback_latch),
                &callback_app,
            );
        });
    unsafe { store.getAvailableContentRuleListIdentifiers(Some(&completion)) };
    Ok(true)
}

fn schedule_macos_absence_retry(
    app: &tauri::AppHandle<tauri::Wry>,
    identity: &MacPolicyIdentity,
    latch: &Arc<MacCleanupLatch>,
) {
    if !latch.retry_verification_after_callback() {
        fail_cleanup_latch(
            latch,
            "macOS rule absence verification exhausted its bounded retries",
        );
        return;
    }
    let retry_app = app.clone();
    let retry_identity = identity.clone();
    let retry_latch = Arc::clone(latch);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        let dispatch_app = retry_app.clone();
        let dispatch_identity = retry_identity.clone();
        let dispatch_latch = Arc::clone(&retry_latch);
        if retry_app
            .run_on_main_thread(move || {
                if let Err(error) = begin_macos_absence_verification_on_ui(
                    &dispatch_app,
                    &dispatch_identity,
                    &dispatch_latch,
                ) {
                    fail_cleanup_latch(
                        &dispatch_latch,
                        &format!("macOS rule absence retry failed: {error}"),
                    );
                }
            })
            .is_err()
        {
            fail_cleanup_latch(&retry_latch, "macOS rule absence retry dispatch failed");
        }
    });
}
