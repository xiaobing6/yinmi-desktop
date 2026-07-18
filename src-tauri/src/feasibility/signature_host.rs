#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::{cell::RefCell, mem};

use super::{
    signature_webview::{
        GD_PAGE_URL, SCENARIO_SIGN_CALLBACK_DELAY, SIGNATURE_HOST_WINDOW_LABEL, SignatureError,
    },
    webview_resource_policy::{IsolationCounters, ResourcePolicyMetadata},
};
use tauri::Manager;
use url::Url;

#[cfg(any(windows, target_os = "macos"))]
use super::signature_webview::{
    NavigationGate, SCENARIO_INIT_CALLBACK_DELAY, SIGNATURE_WEBVIEW_ID,
};
#[cfg(any(windows, target_os = "macos"))]
use super::webview_resource_policy::ResourcePolicyGuard;

#[cfg(target_os = "macos")]
use super::webview_resource_policy::{
    CleanupCompletion, LateMacPolicyOwner, MacCleanupLatch, MacCompileState, MacPolicyIdentity,
};

static RAW_SIGNATURE_SLOT_ACTIVE: AtomicBool = AtomicBool::new(false);
static LAST_ISOLATED_DELAYED_CALLBACK_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(crate) const SCENARIO_STAGE_PENDING_POLICY: u8 = 1;

pub(crate) fn last_isolated_delayed_callback_generation() -> u64 {
    LAST_ISOLATED_DELAYED_CALLBACK_GENERATION.load(Ordering::Acquire)
}

fn mark_isolated_delayed_callback(generation: u64) {
    LAST_ISOLATED_DELAYED_CALLBACK_GENERATION.store(generation, Ordering::Release);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostArrivalDecision {
    BuildRawChild,
    DestroyWithoutBuilding,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum TeardownAuditEvent {
    NativeDestroyed,
    ManagerHostAbsent,
    PolicyCleanupAcknowledged,
    PolicyTombstonesEmpty,
    TeardownComplete,
}

impl TeardownAuditEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::NativeDestroyed => "native-destroyed",
            Self::ManagerHostAbsent => "manager-host-absent",
            Self::PolicyCleanupAcknowledged => "policy-cleanup-acknowledged",
            Self::PolicyTombstonesEmpty => "policy-tombstones-empty",
            Self::TeardownComplete => "teardown-complete",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GenerationTeardownAudit {
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    ordered_events: Vec<TeardownAuditEvent>,
    composite_ack_count: u64,
}

impl GenerationTeardownAudit {
    pub(crate) fn ordered_event_names(&self) -> Vec<&'static str> {
        self.ordered_events
            .iter()
            .copied()
            .map(TeardownAuditEvent::as_str)
            .collect()
    }

    pub(crate) fn is_complete_and_unique(&self) -> bool {
        let unique = self
            .ordered_events
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        self.composite_ack_count == 1
            && self.ordered_events.len() == 5
            && unique.len() == 5
            && self.ordered_events.last() == Some(&TeardownAuditEvent::TeardownComplete)
    }
}

pub(crate) struct CreationTicket {
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    host_label: String,
    cancelled: AtomicBool,
    cancelled_notify: tokio::sync::Notify,
    scenario_stage: AtomicU8,
    scenario_stage_notify: tokio::sync::Notify,
    native_destroyed: AtomicBool,
    native_destroyed_notify: tokio::sync::Notify,
    manager_absent: AtomicBool,
    policy_cleanup: AtomicBool,
    tombstones_empty: AtomicBool,
    teardown_complete: AtomicBool,
    teardown_notify: tokio::sync::Notify,
    slot_empty: AtomicBool,
    slot_empty_notify: tokio::sync::Notify,
    composite_ack_count: AtomicU64,
    teardown_audit_events: Mutex<Vec<TeardownAuditEvent>>,
}

impl CreationTicket {
    pub(crate) fn new(generation: u64, operation_id: u64) -> Arc<Self> {
        Arc::new(Self {
            generation,
            operation_id,
            host_label: format!("{SIGNATURE_HOST_WINDOW_LABEL}-{generation}-{operation_id}"),
            cancelled: AtomicBool::new(false),
            cancelled_notify: tokio::sync::Notify::new(),
            scenario_stage: AtomicU8::new(0),
            scenario_stage_notify: tokio::sync::Notify::new(),
            native_destroyed: AtomicBool::new(false),
            native_destroyed_notify: tokio::sync::Notify::new(),
            manager_absent: AtomicBool::new(false),
            policy_cleanup: AtomicBool::new(false),
            tombstones_empty: AtomicBool::new(false),
            teardown_complete: AtomicBool::new(false),
            teardown_notify: tokio::sync::Notify::new(),
            slot_empty: AtomicBool::new(false),
            slot_empty_notify: tokio::sync::Notify::new(),
            composite_ack_count: AtomicU64::new(0),
            teardown_audit_events: Mutex::new(Vec::with_capacity(5)),
        })
    }

    pub(crate) fn host_label(&self) -> &str {
        &self.host_label
    }

    pub(crate) fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            self.cancelled_notify.notify_waiters();
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn mark_scenario_stage(&self, stage: u8) {
        self.scenario_stage.store(stage, Ordering::Release);
        self.scenario_stage_notify.notify_waiters();
    }

    pub(crate) async fn wait_scenario_stage(&self, stage: u8) {
        loop {
            let notified = self.scenario_stage_notify.notified();
            if self.scenario_stage.load(Ordering::Acquire) >= stage {
                return;
            }
            notified.await;
        }
    }

    pub(crate) async fn wait_cancelled(&self) {
        loop {
            let notified = self.cancelled_notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }

    pub(crate) fn host_arrival_decision(&self) -> HostArrivalDecision {
        if self.is_cancelled() {
            HostArrivalDecision::DestroyWithoutBuilding
        } else {
            HostArrivalDecision::BuildRawChild
        }
    }

    pub(crate) fn mark_native_destroyed(&self) {
        if self.mark_teardown_event(&self.native_destroyed, TeardownAuditEvent::NativeDestroyed) {
            self.native_destroyed_notify.notify_waiters();
        }
        self.maybe_complete_teardown();
    }

    pub(crate) fn mark_manager_absent(&self) {
        self.mark_teardown_event(&self.manager_absent, TeardownAuditEvent::ManagerHostAbsent);
        self.maybe_complete_teardown();
    }

    pub(crate) fn mark_policy_cleanup(&self) {
        self.mark_teardown_event(
            &self.policy_cleanup,
            TeardownAuditEvent::PolicyCleanupAcknowledged,
        );
        self.maybe_complete_teardown();
    }

    pub(crate) fn mark_tombstones_empty(&self) {
        self.mark_teardown_event(
            &self.tombstones_empty,
            TeardownAuditEvent::PolicyTombstonesEmpty,
        );
        self.maybe_complete_teardown();
    }

    pub(crate) fn native_destroyed(&self) -> bool {
        self.native_destroyed.load(Ordering::Acquire)
    }

    pub(crate) fn teardown_complete(&self) -> bool {
        self.teardown_complete.load(Ordering::Acquire)
    }

    pub(crate) fn composite_ack_count(&self) -> u64 {
        self.composite_ack_count.load(Ordering::Acquire)
    }

    pub(crate) fn teardown_audit(&self) -> GenerationTeardownAudit {
        let ordered_events = self
            .teardown_audit_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        GenerationTeardownAudit {
            generation: self.generation,
            operation_id: self.operation_id,
            ordered_events,
            composite_ack_count: self.composite_ack_count(),
        }
    }

    pub(crate) async fn wait_native_destroyed(&self) {
        loop {
            let notified = self.native_destroyed_notify.notified();
            if self.native_destroyed() {
                return;
            }
            notified.await;
        }
    }

    pub(crate) async fn wait_teardown_complete(&self) {
        loop {
            let notified = self.teardown_notify.notified();
            if self.teardown_complete() {
                return;
            }
            notified.await;
        }
    }

    pub(crate) async fn wait_slot_empty(&self) {
        loop {
            let notified = self.slot_empty_notify.notified();
            if self.slot_empty.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    fn mark_slot_empty(&self) {
        if !self.slot_empty.swap(true, Ordering::AcqRel) {
            self.slot_empty_notify.notify_waiters();
        }
    }

    fn maybe_complete_teardown(&self) {
        if self.native_destroyed.load(Ordering::Acquire)
            && self.manager_absent.load(Ordering::Acquire)
            && self.policy_cleanup.load(Ordering::Acquire)
            && self.tombstones_empty.load(Ordering::Acquire)
            && self
                .teardown_complete
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            self.composite_ack_count.fetch_add(1, Ordering::AcqRel);
            self.teardown_audit_events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(TeardownAuditEvent::TeardownComplete);
            self.teardown_notify.notify_waiters();
            if !RAW_SIGNATURE_SLOT_ACTIVE.load(Ordering::Acquire) {
                self.mark_slot_empty();
            }
        }
    }

    fn mark_teardown_event(&self, flag: &AtomicBool, event: TeardownAuditEvent) -> bool {
        let mut ordered_events = self
            .teardown_audit_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if flag.swap(true, Ordering::AcqRel) {
            return false;
        }
        ordered_events.push(event);
        true
    }
}

pub(crate) struct OperationTicket {
    generation: u64,
    operation_id: u64,
    cancelled: AtomicBool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvaluationPurpose {
    Probe,
    Signature,
}

impl OperationTicket {
    pub(crate) fn new(generation: u64, operation_id: u64) -> Arc<Self> {
        Arc::new(Self {
            generation,
            operation_id,
            cancelled: AtomicBool::new(false),
        })
    }

    pub(crate) fn accepts(&self, generation: u64, operation_id: u64) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && self.generation == generation
            && self.operation_id == operation_id
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RawResourcePolicyProfile {
    Live,
    Counterfactual,
    ProtectedCanary,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ScenarioFault {
    #[default]
    None,
    PolicyRegistrationFailure,
    InitializationFinishedDelay,
    SignCallbackDelay,
    HoldBeforePolicyRegistration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RawHostProfile {
    pub(crate) navigation_url: String,
    pub(crate) resource_policy: RawResourcePolicyProfile,
    pub(crate) scenario_fault: ScenarioFault,
}

impl RawHostProfile {
    pub(crate) fn live() -> Self {
        Self {
            navigation_url: GD_PAGE_URL.into(),
            resource_policy: RawResourcePolicyProfile::Live,
            scenario_fault: ScenarioFault::None,
        }
    }

    pub(crate) fn controlled(
        navigation_url: String,
        resource_policy: RawResourcePolicyProfile,
    ) -> Result<Self, SignatureError> {
        if resource_policy == RawResourcePolicyProfile::Live {
            return Err(SignatureError::Webview(
                "controlled actor cannot select the live policy profile".into(),
            ));
        }
        let url = Url::parse(&navigation_url)
            .map_err(|_| SignatureError::Webview("invalid controlled actor URL".into()))?;
        if url.scheme() != "https"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(SignatureError::Webview(
                "controlled actor URL must be an exact HTTPS IPv4 loopback origin".into(),
            ));
        }
        Ok(Self {
            navigation_url,
            resource_policy,
            scenario_fault: ScenarioFault::None,
        })
    }

    pub(crate) fn with_scenario_fault(mut self, scenario_fault: ScenarioFault) -> Self {
        self.scenario_fault = scenario_fault;
        self
    }

    pub(crate) fn allows_navigation(&self, url: &Url) -> bool {
        match self.resource_policy {
            RawResourcePolicyProfile::Live => {
                super::signature_webview::is_allowed_gd_navigation(url)
            }
            RawResourcePolicyProfile::Counterfactual
            | RawResourcePolicyProfile::ProtectedCanary => Url::parse(&self.navigation_url)
                .is_ok_and(|origin| {
                    super::webview_resource_policy::is_allowed_network_request_for(&origin, url)
                }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RawWebViewBuilderSpec {
    pub(crate) id: &'static str,
    pub(crate) initial_url: &'static str,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) devtools: bool,
    pub(crate) incognito: bool,
    pub(crate) clipboard: bool,
    pub(crate) autofill: bool,
    pub(crate) deny_new_windows: bool,
    pub(crate) deny_downloads: bool,
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    pub(crate) initialization_script_count: usize,
    pub(crate) application_ipc_handler: bool,
    pub(crate) custom_protocol: bool,
    pub(crate) profile: RawHostProfile,
}

impl RawWebViewBuilderSpec {
    #[cfg(test)]
    pub(crate) fn new(generation: u64, operation_id: u64) -> Self {
        Self::for_profile(generation, operation_id, RawHostProfile::live())
    }

    pub(crate) fn for_profile(generation: u64, operation_id: u64, profile: RawHostProfile) -> Self {
        Self {
            id: super::signature_webview::SIGNATURE_WEBVIEW_ID,
            initial_url: "about:blank",
            visible: false,
            focused: false,
            devtools: false,
            incognito: true,
            clipboard: false,
            autofill: false,
            deny_new_windows: true,
            deny_downloads: true,
            generation,
            operation_id,
            initialization_script_count: 0,
            application_ipc_handler: false,
            custom_protocol: false,
            profile,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationStep {
    PendingInserted,
    RawChildBuilt,
    NativeInterfacesFound,
    PolicyInstalled,
    ReadyTransition,
    NetworkNavigation,
    DestroyRequested,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UiCreationTrace {
    steps: Vec<CreationStep>,
    cancelled: bool,
}

impl UiCreationTrace {
    #[cfg(test)]
    const SUCCESS_STEPS: [CreationStep; 6] = [
        CreationStep::PendingInserted,
        CreationStep::RawChildBuilt,
        CreationStep::NativeInterfacesFound,
        CreationStep::PolicyInstalled,
        CreationStep::ReadyTransition,
        CreationStep::NetworkNavigation,
    ];

    #[cfg(test)]
    pub(crate) fn run_with_cancellation(cancelled_after: CreationStep) -> Self {
        let mut steps = Vec::new();
        for step in Self::SUCCESS_STEPS {
            steps.push(step);
            if step == cancelled_after {
                steps.push(CreationStep::DestroyRequested);
                return Self {
                    steps,
                    cancelled: true,
                };
            }
        }
        Self {
            steps,
            cancelled: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn run_successfully() -> Self {
        Self {
            steps: Self::SUCCESS_STEPS.to_vec(),
            cancelled: false,
        }
    }

    pub(crate) fn new() -> Self {
        Self {
            steps: Vec::new(),
            cancelled: false,
        }
    }

    pub(crate) fn record(&mut self, step: CreationStep) {
        self.steps.push(step);
    }

    pub(crate) fn record_cancelled(&mut self) {
        self.cancelled = true;
        self.record(CreationStep::DestroyRequested);
    }

    #[cfg(test)]
    pub(crate) fn steps(&self) -> &[CreationStep] {
        &self.steps
    }

    #[cfg(test)]
    pub(crate) fn position(&self, step: CreationStep) -> usize {
        self.steps
            .iter()
            .position(|candidate| *candidate == step)
            .unwrap_or(usize::MAX)
    }

    #[cfg(test)]
    pub(crate) fn was_cancelled(&self) -> bool {
        self.cancelled
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ActorPhase {
    #[default]
    Empty,
    Pending,
    Ready,
    Destroying,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum InitGate {
    #[default]
    WaitingForPolicy,
    WaitingForOfficialFinished,
    #[cfg(test)]
    OfficialFinishedAfterPolicy,
}

impl InitGate {
    #[cfg(test)]
    pub(crate) fn may_poll(self) -> bool {
        self == Self::OfficialFinishedAfterPolicy
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HostCounts {
    pub(crate) registry_slots: u64,
    pub(crate) native_hosts: u64,
    pub(crate) browser_processes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActorTicket {
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    pub(crate) host_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TeardownEvent {
    NativeDestroyed,
    PolicyCleanup,
}

#[derive(Debug, Default)]
pub(crate) struct ActorModel {
    phase: ActorPhase,
    init_gate: InitGate,
    generation: u64,
    operation_id: u64,
    cancelled: bool,
    native_destroyed: bool,
    policy_cleanup: bool,
    counts: HostCounts,
    composite_teardown_count: u64,
}

impl ActorModel {
    pub(crate) fn begin(
        &mut self,
        generation: u64,
        operation_id: u64,
    ) -> Result<ActorTicket, &'static str> {
        if self.phase != ActorPhase::Empty {
            return Err("signature slot is not empty");
        }
        self.phase = ActorPhase::Pending;
        self.init_gate = InitGate::WaitingForPolicy;
        self.generation = generation;
        self.operation_id = operation_id;
        self.cancelled = false;
        self.native_destroyed = false;
        self.policy_cleanup = false;
        self.counts = HostCounts {
            registry_slots: 1,
            native_hosts: 1,
            browser_processes: 1,
        };
        Ok(ActorTicket {
            generation,
            operation_id,
            host_label: format!("{SIGNATURE_HOST_WINDOW_LABEL}-{generation}-{operation_id}"),
        })
    }

    #[cfg(test)]
    pub(crate) fn phase(&self) -> ActorPhase {
        self.phase
    }

    #[cfg(test)]
    pub(crate) fn init_gate(&self) -> InitGate {
        self.init_gate
    }

    pub(crate) fn policy_ready(
        &mut self,
        generation: u64,
        operation_id: u64,
    ) -> Result<bool, &'static str> {
        self.require_current(generation, operation_id)?;
        if self.cancelled || self.phase == ActorPhase::Destroying {
            return Ok(false);
        }
        if self.phase != ActorPhase::Pending {
            return Err("policy acknowledgement requires pending slot");
        }
        self.init_gate = InitGate::WaitingForOfficialFinished;
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn record_page_finished(&mut self, url: &str) -> bool {
        if self.phase != ActorPhase::Pending
            || self.cancelled
            || self.init_gate != InitGate::WaitingForOfficialFinished
            || url != GD_PAGE_URL
        {
            return false;
        }
        self.init_gate = InitGate::OfficialFinishedAfterPolicy;
        true
    }

    pub(crate) fn mark_ready(
        &mut self,
        generation: u64,
        operation_id: u64,
    ) -> Result<(), &'static str> {
        self.require_current(generation, operation_id)?;
        if self.cancelled || self.phase != ActorPhase::Pending {
            return Err("ready transition requires an active pending slot");
        }
        self.phase = ActorPhase::Ready;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn cancel(
        &mut self,
        generation: u64,
        operation_id: u64,
    ) -> Result<(), &'static str> {
        self.require_current(generation, operation_id)?;
        self.cancelled = true;
        self.phase = ActorPhase::Destroying;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn accepts_callback(&self, generation: u64, operation_id: u64) -> bool {
        self.generation == generation
            && self.operation_id == operation_id
            && !self.cancelled
            && matches!(self.phase, ActorPhase::Pending | ActorPhase::Ready)
    }

    pub(crate) fn request_destroy(
        &mut self,
        generation: u64,
        operation_id: u64,
    ) -> Result<bool, &'static str> {
        self.require_current(generation, operation_id)?;
        if self.phase == ActorPhase::Destroying {
            return Ok(false);
        }
        if !matches!(self.phase, ActorPhase::Pending | ActorPhase::Ready) {
            return Err("destroy requires an active slot");
        }
        self.cancelled = true;
        self.phase = ActorPhase::Destroying;
        Ok(true)
    }

    pub(crate) fn acknowledge(
        &mut self,
        generation: u64,
        operation_id: u64,
        event: TeardownEvent,
    ) -> Result<(), &'static str> {
        self.require_current(generation, operation_id)?;
        if self.phase == ActorPhase::Empty {
            return Ok(());
        }
        if self.phase != ActorPhase::Destroying {
            return Err("teardown acknowledgement requires destroying slot");
        }
        match event {
            TeardownEvent::NativeDestroyed => self.native_destroyed = true,
            TeardownEvent::PolicyCleanup => self.policy_cleanup = true,
        }
        if self.native_destroyed && self.policy_cleanup {
            self.phase = ActorPhase::Empty;
            self.counts = HostCounts::default();
            self.composite_teardown_count += 1;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn host_counts(&self) -> HostCounts {
        self.counts
    }

    #[cfg(test)]
    pub(crate) fn composite_teardown_count(&self) -> u64 {
        self.composite_teardown_count
    }

    fn require_current(&self, generation: u64, operation_id: u64) -> Result<(), &'static str> {
        if self.generation == generation && self.operation_id == operation_id {
            Ok(())
        } else {
            Err("stale signature actor generation")
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum HostEvent {
    PageFinished {
        generation: u64,
        operation_id: u64,
        url: String,
    },
    PolicyFault {
        generation: u64,
        operation_id: u64,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct HostInitialization {
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    pub(crate) host_label: String,
    pub(crate) current_url: String,
    pub(crate) policy: ResourcePolicyMetadata,
}

#[derive(Clone, Debug)]
pub(crate) struct RawHostProbeSnapshot {
    pub(crate) generation: u64,
    pub(crate) operation_id: u64,
    pub(crate) host_label: String,
    pub(crate) managed_webviews_empty: bool,
    pub(crate) counters: super::webview_resource_policy::IsolationCounterSnapshot,
}

thread_local! {
    static RAW_SIGNATURE_SLOT: RefCell<MainThreadSignatureSlot> =
        const { RefCell::new(MainThreadSignatureSlot::Empty) };
    static UI_ACTOR_MODEL: RefCell<ActorModel> = RefCell::new(ActorModel::default());
}

#[cfg_attr(
    not(any(windows, target_os = "macos")),
    allow(clippy::large_enum_variant)
)]
enum MainThreadSignatureSlot {
    Empty,
    Pending(PendingMainThreadSignatureInstance),
    Ready(MainThreadSignatureInstance),
    Destroying(DestroyingMainThreadSignatureInstance),
}

struct PendingMainThreadSignatureInstance {
    generation: u64,
    operation_id: u64,
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    builder_spec: RawWebViewBuilderSpec,
    policy_build: PendingResourcePolicy,
    events: tokio::sync::mpsc::UnboundedSender<HostEvent>,
    trace: UiCreationTrace,
    #[cfg(target_os = "macos")]
    initialization_result:
        Option<tokio::sync::oneshot::Sender<Result<HostInitialization, SignatureError>>>,
}

struct MainThreadSignatureInstance {
    generation: u64,
    operation_id: u64,
    host: tauri::Window<tauri::Wry>,
    webview: wry::WebView,
    policy: ActiveResourcePolicy,
    counters: Arc<IsolationCounters>,
    ticket: Arc<CreationTicket>,
    trace: UiCreationTrace,
    scenario_fault: ScenarioFault,
}

enum ActiveResourcePolicy {
    #[cfg(any(windows, target_os = "macos"))]
    Protected(ResourcePolicyGuard),
    Counterfactual(ResourcePolicyMetadata),
}

impl ActiveResourcePolicy {
    fn metadata(&self) -> &ResourcePolicyMetadata {
        match self {
            #[cfg(any(windows, target_os = "macos"))]
            Self::Protected(policy) => policy.metadata(),
            Self::Counterfactual(metadata) => metadata,
        }
    }

    #[cfg(windows)]
    fn uninstall(&mut self) -> Result<bool, SignatureError> {
        match self {
            Self::Protected(policy) => {
                policy.uninstall()?;
                Ok(true)
            }
            Self::Counterfactual(_) => Ok(true),
        }
    }

    #[cfg(target_os = "macos")]
    fn into_late_owner_on_ui(self) -> Result<Option<LateMacPolicyOwner>, SignatureError> {
        match self {
            Self::Protected(policy) => policy.into_late_owner_on_ui().map(Some),
            Self::Counterfactual(_) => Ok(None),
        }
    }
}

struct DestroyingMainThreadSignatureInstance {
    generation: u64,
    operation_id: u64,
    host_label: String,
    ticket: Arc<CreationTicket>,
    #[cfg(target_os = "macos")]
    late_policy: Option<LateMacPolicyOwner>,
}

#[cfg(windows)]
#[derive(Default)]
struct PendingResourcePolicy {
    synchronous_registration_started: bool,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct PendingResourcePolicy {
    native: Option<super::webview_resource_policy::macos::PendingMacosResourcePolicy>,
}

#[cfg(not(any(windows, target_os = "macos")))]
#[derive(Default)]
struct PendingResourcePolicy;

pub(crate) async fn create_raw_signature_host(
    app: tauri::AppHandle<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    profile: RawHostProfile,
) -> Result<
    (
        HostInitialization,
        tokio::sync::mpsc::UnboundedReceiver<HostEvent>,
    ),
    SignatureError,
> {
    let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
    let task_app = app.clone();
    let task_ticket = Arc::clone(&ticket);
    let builder_spec =
        RawWebViewBuilderSpec::for_profile(ticket.generation, ticket.operation_id, profile);

    tauri::async_runtime::spawn(async move {
        let build_app = task_app.clone();
        let build_ticket = Arc::clone(&task_ticket);
        let host_result = tokio::task::spawn_blocking(move || {
            let builder = tauri::window::WindowBuilder::new(
                &build_app,
                build_ticket.host_label().to_string(),
            )
            .inner_size(1.0, 1.0)
            .visible(false)
            .focused(false)
            .focusable(false)
            .decorations(false)
            .shadow(false)
            .resizable(false);
            #[cfg(windows)]
            let builder = builder.skip_taskbar(true);
            builder.build().map_err(|error| {
                SignatureError::Webview(format!("native signature host creation failed: {error}"))
            })
        })
        .await
        .map_err(|error| SignatureError::Webview(format!("host task failed: {error}")))
        .and_then(|result| result);

        let result = match host_result {
            Ok(host) => {
                if matches!(
                    task_ticket.host_arrival_decision(),
                    HostArrivalDecision::DestroyWithoutBuilding
                ) {
                    task_ticket.cancel();
                }
                register_destroyed_handler(&task_app, &host, Arc::clone(&task_ticket));
                handoff_host_to_ui(task_app, host, task_ticket, builder_spec, event_sender).await
            }
            Err(error) => {
                task_ticket.mark_native_destroyed();
                task_ticket.mark_manager_absent();
                task_ticket.mark_policy_cleanup();
                task_ticket.mark_tombstones_empty();
                Err(error)
            }
        };
        let _ = result_sender.send(result);
    });

    let initialization = result_receiver
        .await
        .map_err(|_| SignatureError::Webview("host creation coordinator stopped".into()))??;
    Ok((initialization, event_receiver))
}

fn register_destroyed_handler(
    app: &tauri::AppHandle<tauri::Wry>,
    host: &tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
) {
    let event_app = app.clone();
    let host_label = ticket.host_label().to_string();
    host.on_window_event(move |event| {
        if !matches!(event, tauri::WindowEvent::Destroyed) {
            return;
        }
        let check_app = event_app.clone();
        let check_label = host_label.clone();
        let check_ticket = Arc::clone(&ticket);
        let dispatch_app = check_app.clone();
        let _ = dispatch_app.run_on_main_thread(move || {
            if check_app.get_window(&check_label).is_none() {
                check_ticket.mark_native_destroyed();
                check_ticket.mark_manager_absent();
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor.borrow_mut().acknowledge(
                        check_ticket.generation,
                        check_ticket.operation_id,
                        TeardownEvent::NativeDestroyed,
                    );
                });
                finalize_destroyed_slot(&check_ticket);
            }
        });
    });
}

#[cfg(windows)]
async fn handoff_host_to_ui(
    app: tauri::AppHandle<tauri::Wry>,
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    builder_spec: RawWebViewBuilderSpec,
    events: tokio::sync::mpsc::UnboundedSender<HostEvent>,
) -> Result<HostInitialization, SignatureError> {
    if builder_spec.profile.scenario_fault == ScenarioFault::HoldBeforePolicyRegistration {
        ticket.mark_scenario_stage(SCENARIO_STAGE_PENDING_POLICY);
        ticket.wait_cancelled().await;
        ticket.mark_policy_cleanup();
        ticket.mark_tombstones_empty();
        let _ = host.destroy();
        return Err(SignatureError::Cancelled);
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let fallback_host = host.clone();
    let dispatch_result = app.run_on_main_thread(move || {
        let result = install_raw_child_on_ui(host, ticket, builder_spec, events);
        let _ = sender.send(result);
    });
    if dispatch_result.is_err() {
        let _ = fallback_host.destroy();
        return Err(SignatureError::Webview(
            "raw signature host UI handoff failed".into(),
        ));
    }
    receiver
        .await
        .map_err(|_| SignatureError::Webview("raw signature host UI callback stopped".into()))?
}

#[cfg(not(any(windows, target_os = "macos")))]
async fn handoff_host_to_ui(
    _app: tauri::AppHandle<tauri::Wry>,
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    _builder_spec: RawWebViewBuilderSpec,
    _events: tokio::sync::mpsc::UnboundedSender<HostEvent>,
) -> Result<HostInitialization, SignatureError> {
    ticket.cancel();
    ticket.mark_policy_cleanup();
    ticket.mark_tombstones_empty();
    host.destroy().map_err(|error| {
        SignatureError::Webview(format!("native signature host destroy failed: {error}"))
    })?;
    Err(SignatureError::Webview(
        "raw signature host is supported only on Windows and macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
async fn handoff_host_to_ui(
    app: tauri::AppHandle<tauri::Wry>,
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    builder_spec: RawWebViewBuilderSpec,
    events: tokio::sync::mpsc::UnboundedSender<HostEvent>,
) -> Result<HostInitialization, SignatureError> {
    if builder_spec.profile.scenario_fault == ScenarioFault::HoldBeforePolicyRegistration {
        ticket.mark_scenario_stage(SCENARIO_STAGE_PENDING_POLICY);
        ticket.wait_cancelled().await;
        ticket.mark_policy_cleanup();
        ticket.mark_tombstones_empty();
        let _ = host.destroy();
        return Err(SignatureError::Cancelled);
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let fallback_host = host.clone();
    let ui_app = app.clone();
    let dispatch_result = app.run_on_main_thread(move || {
        install_raw_child_on_ui(host, ticket, builder_spec, events, ui_app, sender);
    });
    if dispatch_result.is_err() {
        let _ = fallback_host.destroy();
        return Err(SignatureError::Webview(
            "raw signature host UI handoff failed".into(),
        ));
    }
    receiver
        .await
        .map_err(|_| SignatureError::Webview("raw signature host UI callback stopped".into()))?
}

#[cfg(windows)]
fn install_raw_child_on_ui(
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    builder_spec: RawWebViewBuilderSpec,
    events: tokio::sync::mpsc::UnboundedSender<HostEvent>,
) -> Result<HostInitialization, SignatureError> {
    let mut trace = UiCreationTrace::new();
    UI_ACTOR_MODEL.with(|actor| {
        actor
            .borrow_mut()
            .begin(ticket.generation, ticket.operation_id)
            .map(|_| ())
            .map_err(|message| SignatureError::Webview(message.into()))
    })?;
    RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !matches!(*slot, MainThreadSignatureSlot::Empty) {
            return Err(SignatureError::Webview(
                "raw signature TLS slot is occupied".into(),
            ));
        }
        trace.record(CreationStep::PendingInserted);
        *slot = MainThreadSignatureSlot::Pending(PendingMainThreadSignatureInstance {
            generation: ticket.generation,
            operation_id: ticket.operation_id,
            host,
            ticket: Arc::clone(&ticket),
            builder_spec,
            policy_build: PendingResourcePolicy::default(),
            events,
            trace,
            #[cfg(target_os = "macos")]
            initialization_result: None,
        });
        RAW_SIGNATURE_SLOT_ACTIVE.store(true, Ordering::Release);
        Ok(())
    })?;

    if ticket.is_cancelled() {
        destroy_generation_on_ui(&ticket)?;
        return Err(SignatureError::Cancelled);
    }

    let pending = RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        match mem::replace(&mut *slot, MainThreadSignatureSlot::Empty) {
            MainThreadSignatureSlot::Pending(pending)
                if pending.generation == ticket.generation
                    && pending.operation_id == ticket.operation_id =>
            {
                Ok(pending)
            }
            other => {
                *slot = other;
                Err(SignatureError::Webview(
                    "raw signature pending slot changed".into(),
                ))
            }
        }
    })?;

    let PendingMainThreadSignatureInstance {
        generation,
        operation_id,
        host,
        ticket,
        builder_spec,
        mut policy_build,
        events,
        mut trace,
    } = pending;
    let counters = Arc::new(IsolationCounters::default());
    let navigation_origin = Url::parse(&builder_spec.profile.navigation_url)
        .map_err(|_| SignatureError::OriginRejected)?;
    let navigation_gate = Arc::new(NavigationGate::for_origin(navigation_origin));
    let navigation_counters = Arc::clone(&counters);
    let navigation_gate_callback = Arc::clone(&navigation_gate);
    let new_window_counters = Arc::clone(&counters);
    let download_counters = Arc::clone(&counters);
    let page_events = events.clone();
    let page_ticket = Arc::clone(&ticket);
    let delay_page_finished =
        builder_spec.profile.scenario_fault == ScenarioFault::InitializationFinishedDelay;

    use wry::WebViewBuilderExtWindows;
    let builder = wry::WebViewBuilder::new()
        .with_id(SIGNATURE_WEBVIEW_ID)
        .with_url(builder_spec.initial_url)
        .with_visible(builder_spec.visible)
        .with_focused(builder_spec.focused)
        .with_devtools(builder_spec.devtools)
        .with_incognito(builder_spec.incognito)
        .with_clipboard(builder_spec.clipboard)
        .with_general_autofill_enabled(builder_spec.autofill)
        .with_navigation_handler(move |raw_url| {
            let allowed =
                Url::parse(&raw_url).is_ok_and(|url| navigation_gate_callback.allows(&url));
            if !allowed {
                navigation_counters.blocked_navigation();
            }
            allowed
        })
        .with_new_window_req_handler(move |_, _| {
            new_window_counters.blocked_new_window();
            wry::NewWindowResponse::Deny
        })
        .with_download_started_handler(move |_, _| {
            download_counters.blocked_download();
            false
        })
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, wry::PageLoadEvent::Finished) {
                let callback_ticket = Arc::clone(&page_ticket);
                let callback_events = page_events.clone();
                if delay_page_finished {
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(SCENARIO_INIT_CALLBACK_DELAY).await;
                        if !callback_ticket.is_cancelled() {
                            let _ = callback_events.send(HostEvent::PageFinished {
                                generation,
                                operation_id,
                                url,
                            });
                        }
                    });
                } else if !callback_ticket.is_cancelled() {
                    let _ = callback_events.send(HostEvent::PageFinished {
                        generation,
                        operation_id,
                        url,
                    });
                }
            }
        })
        .with_browser_accelerator_keys(false)
        .with_default_context_menus(false);

    let webview = match builder.build_as_child(&host) {
        Ok(webview) => webview,
        Err(error) => {
            begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
            return Err(SignatureError::Webview(format!(
                "raw WRY child creation failed: {error}"
            )));
        }
    };
    trace.record(CreationStep::RawChildBuilt);
    trace.record(CreationStep::NativeInterfacesFound);
    if ticket.is_cancelled() {
        drop(webview);
        begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
        return Err(SignatureError::Cancelled);
    }
    if builder_spec.profile.scenario_fault == ScenarioFault::PolicyRegistrationFailure {
        drop(webview);
        begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
        return Err(SignatureError::Webview(
            "injected policy registration failure".into(),
        ));
    }

    let policy = if builder_spec.profile.resource_policy == RawResourcePolicyProfile::Counterfactual
    {
        match super::webview_resource_policy::windows::counterfactual_metadata(&webview) {
            Ok(metadata) => ActiveResourcePolicy::Counterfactual(metadata),
            Err(error) => {
                drop(webview);
                begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
                return Err(error);
            }
        }
    } else {
        policy_build.synchronous_registration_started = true;
        debug_assert!(policy_build.synchronous_registration_started);
        let fault_events = events.clone();
        let fault_ticket = Arc::clone(&ticket);
        let fault_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = fault_events.send(HostEvent::PolicyFault {
                generation: fault_ticket.generation,
                operation_id: fault_ticket.operation_id,
            });
        });
        let allowed_origin = Url::parse(&builder_spec.profile.navigation_url)
            .map_err(|_| SignatureError::OriginRejected)?;
        match ResourcePolicyGuard::install(
            &webview,
            allowed_origin,
            Arc::clone(&counters),
            fault_callback,
        ) {
            Ok(policy) => ActiveResourcePolicy::Protected(policy),
            Err(error) => {
                drop(webview);
                begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
                return Err(error);
            }
        }
    };
    trace.record(CreationStep::PolicyInstalled);
    if ticket.is_cancelled() {
        drop(policy);
        drop(webview);
        begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
        return Err(SignatureError::Cancelled);
    }
    let policy_accepted = UI_ACTOR_MODEL.with(|actor| {
        actor
            .borrow_mut()
            .policy_ready(generation, operation_id)
            .map_err(|message| SignatureError::Webview(message.into()))
    })?;
    if !policy_accepted {
        drop(policy);
        drop(webview);
        begin_destroy_without_webview(host, Arc::clone(&ticket), &mut trace);
        return Err(SignatureError::Cancelled);
    }

    let initialization = HostInitialization {
        generation,
        operation_id,
        host_label: ticket.host_label().to_string(),
        current_url: builder_spec.initial_url.to_string(),
        policy: policy.metadata().clone(),
    };
    trace.record(CreationStep::ReadyTransition);
    RAW_SIGNATURE_SLOT.with(|slot| {
        *slot.borrow_mut() = MainThreadSignatureSlot::Ready(MainThreadSignatureInstance {
            generation,
            operation_id,
            host,
            webview,
            policy,
            counters,
            ticket: Arc::clone(&ticket),
            trace,
            scenario_fault: builder_spec.profile.scenario_fault,
        });
    });
    UI_ACTOR_MODEL.with(|actor| {
        actor
            .borrow_mut()
            .mark_ready(generation, operation_id)
            .map_err(|message| SignatureError::Webview(message.into()))
    })?;

    if ticket.is_cancelled() {
        destroy_generation_on_ui(&ticket)?;
        return Err(SignatureError::Cancelled);
    }
    RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let MainThreadSignatureSlot::Ready(instance) = &mut *slot else {
            return Err(SignatureError::Webview(
                "raw signature ready slot disappeared".into(),
            ));
        };
        instance
            .webview
            .load_url(&builder_spec.profile.navigation_url)
            .map_err(|error| {
                SignatureError::Webview(format!("signature navigation failed: {error}"))
            })?;
        instance.trace.record(CreationStep::NetworkNavigation);
        Ok(())
    })?;
    Ok(initialization)
}

#[cfg(target_os = "macos")]
fn install_raw_child_on_ui(
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    builder_spec: RawWebViewBuilderSpec,
    events: tokio::sync::mpsc::UnboundedSender<HostEvent>,
    app: tauri::AppHandle<tauri::Wry>,
    result: tokio::sync::oneshot::Sender<Result<HostInitialization, SignatureError>>,
) {
    let mut trace = UiCreationTrace::new();
    let actor_result = UI_ACTOR_MODEL.with(|actor| {
        actor
            .borrow_mut()
            .begin(ticket.generation, ticket.operation_id)
            .map(|_| ())
            .map_err(|message| SignatureError::Webview(message.into()))
    });
    if let Err(error) = actor_result {
        let _ = result.send(Err(error));
        let _ = host.destroy();
        return;
    }
    let insert_result = RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !matches!(*slot, MainThreadSignatureSlot::Empty) {
            return Err(SignatureError::Webview(
                "raw signature TLS slot is occupied".into(),
            ));
        }
        trace.record(CreationStep::PendingInserted);
        *slot = MainThreadSignatureSlot::Pending(PendingMainThreadSignatureInstance {
            generation: ticket.generation,
            operation_id: ticket.operation_id,
            host,
            ticket: Arc::clone(&ticket),
            builder_spec,
            policy_build: PendingResourcePolicy::default(),
            events,
            trace,
            initialization_result: Some(result),
        });
        RAW_SIGNATURE_SLOT_ACTIVE.store(true, Ordering::Release);
        Ok(())
    });
    if insert_result.is_err() {
        return;
    }
    if ticket.is_cancelled() {
        let _ = destroy_generation_on_ui(&ticket);
        return;
    }

    let inject_policy_failure = RAW_SIGNATURE_SLOT.with(|slot| {
        matches!(
            &*slot.borrow(),
            MainThreadSignatureSlot::Pending(pending)
                if pending.generation == ticket.generation
                    && pending.operation_id == ticket.operation_id
                    && pending.builder_spec.profile.scenario_fault
                        == ScenarioFault::PolicyRegistrationFailure
        )
    });
    if inject_policy_failure {
        fail_macos_pending_on_ui(
            ticket.generation,
            ticket.operation_id,
            SignatureError::Webview("injected policy registration failure".into()),
        );
        return;
    }

    if RAW_SIGNATURE_SLOT.with(|slot| {
        matches!(
            &*slot.borrow(),
            MainThreadSignatureSlot::Pending(pending)
                if pending.generation == ticket.generation
                    && pending.operation_id == ticket.operation_id
                    && pending.builder_spec.profile.resource_policy
                        == RawResourcePolicyProfile::Counterfactual
        )
    }) {
        let active = RAW_SIGNATURE_SLOT
            .with(|slot| mem::replace(&mut *slot.borrow_mut(), MainThreadSignatureSlot::Empty));
        match active {
            MainThreadSignatureSlot::Pending(pending)
                if pending.generation == ticket.generation
                    && pending.operation_id == ticket.operation_id =>
            {
                finish_macos_counterfactual_on_ui(pending);
            }
            other => RAW_SIGNATURE_SLOT.with(|slot| *slot.borrow_mut() = other),
        }
        return;
    }

    let allowed_origin = match RAW_SIGNATURE_SLOT.with(|slot| {
        let slot = slot.borrow();
        let MainThreadSignatureSlot::Pending(pending) = &*slot else {
            return Err(SignatureError::NotReady);
        };
        Url::parse(&pending.builder_spec.profile.navigation_url)
            .map_err(|_| SignatureError::OriginRejected)
    }) {
        Ok(origin) => origin,
        Err(error) => {
            fail_macos_pending_on_ui(ticket.generation, ticket.operation_id, error);
            return;
        }
    };

    let policy = super::webview_resource_policy::macos::PendingMacosResourcePolicy::prepare(
        app.clone(),
        ticket.generation,
        ticket.operation_id,
        &allowed_origin,
    );
    match policy {
        Ok(policy) => {
            let accepted = RAW_SIGNATURE_SLOT.with(|slot| {
                let mut slot = slot.borrow_mut();
                let MainThreadSignatureSlot::Pending(pending) = &mut *slot else {
                    return false;
                };
                if pending.generation != ticket.generation
                    || pending.operation_id != ticket.operation_id
                {
                    return false;
                }
                pending.policy_build.native = Some(policy);
                true
            });
            if !accepted || ticket.is_cancelled() {
                let _ = destroy_generation_on_ui(&ticket);
            } else {
                let invocation = RAW_SIGNATURE_SLOT.with(|slot| {
                    let mut slot = slot.borrow_mut();
                    let MainThreadSignatureSlot::Pending(pending) = &mut *slot else {
                        return Err(SignatureError::StaleCallback);
                    };
                    if pending.generation != ticket.generation
                        || pending.operation_id != ticket.operation_id
                    {
                        return Err(SignatureError::StaleCallback);
                    }
                    pending
                        .policy_build
                        .native
                        .as_mut()
                        .ok_or(SignatureError::StaleCallback)?
                        .compile_invocation()
                });
                match invocation {
                    Ok(invocation) => invocation.invoke(),
                    Err(error) => {
                        fail_macos_pending_on_ui(ticket.generation, ticket.operation_id, error)
                    }
                }
            }
        }
        Err(error) => fail_macos_pending_on_ui(ticket.generation, ticket.operation_id, error),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn complete_macos_compile_on_ui(
    identity: MacPolicyIdentity,
    latch: Arc<MacCleanupLatch>,
    outcome: super::webview_resource_policy::macos::MacosRuleResult,
    app: &tauri::AppHandle<tauri::Wry>,
) {
    let active = RAW_SIGNATURE_SLOT
        .with(|slot| mem::replace(&mut *slot.borrow_mut(), MainThreadSignatureSlot::Empty));
    match active {
        MainThreadSignatureSlot::Pending(mut pending)
            if pending.generation == identity.generation
                && pending.operation_id == identity.operation_id =>
        {
            let Some(mut native) = pending.policy_build.native.take() else {
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Pending(pending);
                });
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                return;
            };
            if !native.matches(&identity, &latch) {
                pending.policy_build.native = Some(native);
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Pending(pending);
                });
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                return;
            }
            match outcome {
                Ok(rule) if !pending.ticket.is_cancelled() => {
                    finish_macos_raw_child_on_ui(pending, native, rule);
                }
                Ok(rule) => {
                    drop(rule);
                    pending.policy_build.native = Some(native);
                    let _ = transition_macos_pending_to_destroying(
                        pending,
                        SignatureError::Cancelled,
                        false,
                    );
                }
                Err(message) => {
                    latch.request_cleanup();
                    if latch.cleanup_completion() == CleanupCompletion::Pending
                        && !latch.complete_cleanup(CleanupCompletion::VerifiedAbsent)
                    {
                        super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                    }
                    pending.policy_build.native = Some(native);
                    let _ = transition_macos_pending_to_destroying(
                        pending,
                        SignatureError::Webview(message),
                        false,
                    );
                }
            }
        }
        MainThreadSignatureSlot::Destroying(destroying)
            if destroying.generation == identity.generation
                && destroying.operation_id == identity.operation_id
                && destroying.late_policy.as_ref().is_some_and(|owner| {
                    owner.identity == identity && Arc::ptr_eq(&owner.latch, &latch)
                }) =>
        {
            let compile_failed = outcome.is_err();
            drop(outcome);
            latch.request_cleanup();
            if compile_failed
                && latch.cleanup_completion() == CleanupCompletion::Pending
                && !latch.complete_cleanup(CleanupCompletion::VerifiedAbsent)
            {
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
            }
            RAW_SIGNATURE_SLOT.with(|slot| {
                *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(destroying);
            });
            reconcile_macos_cleanup_on_ui(identity, latch, app);
        }
        other => {
            RAW_SIGNATURE_SLOT.with(|slot| *slot.borrow_mut() = other);
            super::webview_resource_policy::macos::mark_policy_cleanup_fault();
            eprintln!("macOS compile completion had no exact UI owner");
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn fail_macos_compile_affinity_on_ui(
    identity: MacPolicyIdentity,
    latch: Arc<MacCleanupLatch>,
    app: &tauri::AppHandle<tauri::Wry>,
) {
    let active = RAW_SIGNATURE_SLOT
        .with(|slot| mem::replace(&mut *slot.borrow_mut(), MainThreadSignatureSlot::Empty));
    match active {
        MainThreadSignatureSlot::Pending(mut pending)
            if pending.generation == identity.generation
                && pending.operation_id == identity.operation_id
                && pending
                    .policy_build
                    .native
                    .as_ref()
                    .is_some_and(|native| native.matches(&identity, &latch)) =>
        {
            let _ = transition_macos_pending_to_destroying(
                pending,
                SignatureError::Webview(
                    "macOS policy callback violated main-thread affinity".into(),
                ),
                false,
            );
        }
        MainThreadSignatureSlot::Destroying(destroying)
            if destroying.generation == identity.generation
                && destroying.operation_id == identity.operation_id
                && destroying.late_policy.as_ref().is_some_and(|owner| {
                    owner.identity == identity && Arc::ptr_eq(&owner.latch, &latch)
                }) =>
        {
            RAW_SIGNATURE_SLOT.with(|slot| {
                *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(destroying);
            });
            reconcile_macos_cleanup_on_ui(identity, latch, app);
        }
        other => {
            RAW_SIGNATURE_SLOT.with(|slot| *slot.borrow_mut() = other);
            super::webview_resource_policy::macos::mark_policy_cleanup_fault();
            eprintln!("macOS off-main compile callback had no exact UI owner");
        }
    }
}

#[cfg(target_os = "macos")]
fn finish_macos_raw_child_on_ui(
    mut pending: PendingMainThreadSignatureInstance,
    mut native: super::webview_resource_policy::macos::PendingMacosResourcePolicy,
    rule: objc2::rc::Retained<objc2_web_kit::WKContentRuleList>,
) {
    let configuration = match native.take_configuration() {
        Ok(configuration) => configuration,
        Err(error) => {
            pending.policy_build.native = Some(native);
            let _ = transition_macos_pending_to_destroying(pending, error, false);
            return;
        }
    };
    native.attach(&rule);
    pending.trace.record(CreationStep::PolicyInstalled);
    if pending.ticket.is_cancelled() {
        native.detach(&rule);
        pending.policy_build.native = Some(native);
        let _ = transition_macos_pending_to_destroying(pending, SignatureError::Cancelled, false);
        return;
    }
    let generation = pending.generation;
    let operation_id = pending.operation_id;
    let (webview, counters, builder_spec) =
        match build_macos_raw_child_on_ui(&mut pending, configuration) {
            Ok(built) => built,
            Err(error) => {
                native.detach(&rule);
                pending.policy_build.native = Some(native);
                let _ = transition_macos_pending_to_destroying(pending, error, false);
                return;
            }
        };
    let policy = ActiveResourcePolicy::Protected(native.into_guard(rule, Arc::clone(&counters)));
    activate_macos_raw_child_on_ui(pending, webview, policy, counters, builder_spec);
}

#[cfg(target_os = "macos")]
fn finish_macos_counterfactual_on_ui(mut pending: PendingMainThreadSignatureInstance) {
    let generation = pending.generation;
    let operation_id = pending.operation_id;
    let (configuration, metadata) =
        match super::webview_resource_policy::macos::counterfactual_configuration() {
            Ok(configuration) => configuration,
            Err(error) => {
                let _ = transition_macos_pending_to_destroying(pending, error, true);
                return;
            }
        };
    pending.trace.record(CreationStep::PolicyInstalled);
    let (webview, counters, builder_spec) =
        match build_macos_raw_child_on_ui(&mut pending, configuration) {
            Ok(built) => built,
            Err(error) => {
                let _ = transition_macos_pending_to_destroying(pending, error, true);
                return;
            }
        };
    debug_assert_eq!(pending.generation, generation);
    debug_assert_eq!(pending.operation_id, operation_id);
    activate_macos_raw_child_on_ui(
        pending,
        webview,
        ActiveResourcePolicy::Counterfactual(metadata),
        counters,
        builder_spec,
    );
}

#[cfg(target_os = "macos")]
fn build_macos_raw_child_on_ui(
    pending: &mut PendingMainThreadSignatureInstance,
    configuration: objc2::rc::Retained<objc2_web_kit::WKWebViewConfiguration>,
) -> Result<(wry::WebView, Arc<IsolationCounters>, RawWebViewBuilderSpec), SignatureError> {
    let generation = pending.generation;
    let operation_id = pending.operation_id;
    let builder_spec = pending.builder_spec.clone();
    let navigation_origin = Url::parse(&builder_spec.profile.navigation_url)
        .map_err(|_| SignatureError::OriginRejected)?;
    let counters = Arc::new(IsolationCounters::default());
    let navigation_gate = Arc::new(NavigationGate::for_origin(navigation_origin));
    let navigation_counters = Arc::clone(&counters);
    let navigation_gate_callback = Arc::clone(&navigation_gate);
    let new_window_counters = Arc::clone(&counters);
    let download_counters = Arc::clone(&counters);
    let page_events = pending.events.clone();
    let page_ticket = Arc::clone(&pending.ticket);
    let delay_page_finished =
        builder_spec.profile.scenario_fault == ScenarioFault::InitializationFinishedDelay;
    let builder = wry::WebViewBuilder::new()
        .with_id(SIGNATURE_WEBVIEW_ID)
        .with_url(builder_spec.initial_url)
        .with_visible(builder_spec.visible)
        .with_focused(builder_spec.focused)
        .with_devtools(builder_spec.devtools)
        .with_incognito(builder_spec.incognito)
        .with_clipboard(builder_spec.clipboard)
        .with_navigation_handler(move |raw_url| {
            let allowed =
                Url::parse(&raw_url).is_ok_and(|url| navigation_gate_callback.allows(&url));
            if !allowed {
                navigation_counters.blocked_navigation();
            }
            allowed
        })
        .with_new_window_req_handler(move |_, _| {
            new_window_counters.blocked_new_window();
            wry::NewWindowResponse::Deny
        })
        .with_download_started_handler(move |_, _| {
            download_counters.blocked_download();
            false
        })
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, wry::PageLoadEvent::Finished) {
                let callback_ticket = Arc::clone(&page_ticket);
                let callback_events = page_events.clone();
                if delay_page_finished {
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(SCENARIO_INIT_CALLBACK_DELAY).await;
                        if !callback_ticket.is_cancelled() {
                            let _ = callback_events.send(HostEvent::PageFinished {
                                generation,
                                operation_id,
                                url,
                            });
                        }
                    });
                } else if !callback_ticket.is_cancelled() {
                    let _ = callback_events.send(HostEvent::PageFinished {
                        generation,
                        operation_id,
                        url,
                    });
                }
            }
        });
    let builder =
        super::webview_resource_policy::macos::apply_configuration(builder, configuration);
    let policy_accepted = UI_ACTOR_MODEL.with(|actor| {
        actor
            .borrow_mut()
            .policy_ready(generation, operation_id)
            .map_err(|message| SignatureError::Webview(message.into()))
    })?;
    if !policy_accepted || pending.ticket.is_cancelled() {
        return Err(SignatureError::Cancelled);
    }
    let webview = builder.build_as_child(&pending.host).map_err(|error| {
        SignatureError::Webview(format!("raw WRY child creation failed: {error}"))
    })?;
    pending.trace.record(CreationStep::RawChildBuilt);
    pending.trace.record(CreationStep::NativeInterfacesFound);
    if pending.ticket.is_cancelled() {
        drop(webview);
        return Err(SignatureError::Cancelled);
    }
    UI_ACTOR_MODEL
        .with(|actor| actor.borrow_mut().mark_ready(generation, operation_id))
        .map_err(|message| SignatureError::Webview(message.into()))?;
    Ok((webview, counters, builder_spec))
}

#[cfg(target_os = "macos")]
fn activate_macos_raw_child_on_ui(
    mut pending: PendingMainThreadSignatureInstance,
    webview: wry::WebView,
    policy: ActiveResourcePolicy,
    counters: Arc<IsolationCounters>,
    builder_spec: RawWebViewBuilderSpec,
) {
    let generation = pending.generation;
    let operation_id = pending.operation_id;
    let initialization = HostInitialization {
        generation,
        operation_id,
        host_label: pending.ticket.host_label().to_string(),
        current_url: builder_spec.initial_url.to_string(),
        policy: policy.metadata().clone(),
    };
    pending.trace.record(CreationStep::ReadyTransition);
    let result_sender = pending.initialization_result.take();
    RAW_SIGNATURE_SLOT.with(|slot| {
        *slot.borrow_mut() = MainThreadSignatureSlot::Ready(MainThreadSignatureInstance {
            generation,
            operation_id,
            host: pending.host,
            webview,
            policy,
            counters,
            ticket: Arc::clone(&pending.ticket),
            trace: pending.trace,
            scenario_fault: builder_spec.profile.scenario_fault,
        });
    });
    if pending.ticket.is_cancelled() {
        if let Some(sender) = result_sender {
            let _ = sender.send(Err(SignatureError::Cancelled));
        }
        let _ = destroy_generation_on_ui(&pending.ticket);
        return;
    }
    let navigation_result = RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let MainThreadSignatureSlot::Ready(instance) = &mut *slot else {
            return Err(SignatureError::NotReady);
        };
        instance
            .webview
            .load_url(&builder_spec.profile.navigation_url)
            .map_err(|error| {
                SignatureError::Webview(format!("signature navigation failed: {error}"))
            })?;
        instance.trace.record(CreationStep::NetworkNavigation);
        Ok(())
    });
    match navigation_result {
        Ok(()) => {
            if let Some(sender) = result_sender {
                let _ = sender.send(Ok(initialization));
            }
        }
        Err(error) => {
            if let Some(sender) = result_sender {
                let _ = sender.send(Err(error));
            }
            let _ = destroy_generation_on_ui(&pending.ticket);
        }
    }
}

#[cfg(target_os = "macos")]
fn fail_macos_pending_on_ui(generation: u64, operation_id: u64, error: SignatureError) {
    let active = RAW_SIGNATURE_SLOT
        .with(|slot| mem::replace(&mut *slot.borrow_mut(), MainThreadSignatureSlot::Empty));
    match active {
        MainThreadSignatureSlot::Pending(pending)
            if pending.generation == generation && pending.operation_id == operation_id =>
        {
            let _ = transition_macos_pending_to_destroying(pending, error, true);
        }
        other => RAW_SIGNATURE_SLOT.with(|slot| *slot.borrow_mut() = other),
    }
}

#[cfg(target_os = "macos")]
fn transition_macos_pending_to_destroying(
    mut pending: PendingMainThreadSignatureInstance,
    error: SignatureError,
    cleanup_if_no_native: bool,
) -> Result<(), SignatureError> {
    pending.ticket.cancel();
    pending.trace.record_cancelled();
    if let Some(sender) = pending.initialization_result.take() {
        let _ = sender.send(Err(error));
    }
    let had_native_policy = pending.policy_build.native.is_some();
    let late_policy = pending.policy_build.native.take().and_then(|native| {
        match native.into_late_owner_on_ui() {
            Ok(owner) => Some(owner),
            Err(error) => {
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                eprintln!("macOS pending policy owner release failed: {error}");
                None
            }
        }
    });
    if let Some(owner) = &late_policy {
        super::signature_webview::add_late_policy_tombstone(
            owner.identity.generation,
            owner.identity.operation_id,
            owner.identity.identifier.clone(),
        );
    }
    let policy_already_clean = !had_native_policy && cleanup_if_no_native;
    if policy_already_clean {
        pending.ticket.mark_policy_cleanup();
        pending.ticket.mark_tombstones_empty();
    }
    UI_ACTOR_MODEL.with(|actor| {
        let _ = actor
            .borrow_mut()
            .request_destroy(pending.generation, pending.operation_id);
        if policy_already_clean {
            let _ = actor.borrow_mut().acknowledge(
                pending.generation,
                pending.operation_id,
                TeardownEvent::PolicyCleanup,
            );
        }
    });
    let cleanup = late_policy.as_ref().map(|owner| {
        (
            owner.identity.clone(),
            Arc::clone(&owner.latch),
            pending.host.app_handle().clone(),
        )
    });
    RAW_SIGNATURE_SLOT.with(|slot| {
        *slot.borrow_mut() =
            MainThreadSignatureSlot::Destroying(DestroyingMainThreadSignatureInstance {
                generation: pending.generation,
                operation_id: pending.operation_id,
                host_label: pending.ticket.host_label().to_string(),
                ticket: Arc::clone(&pending.ticket),
                late_policy,
            });
    });
    let destroy_result = pending.host.destroy().map_err(|error| {
        SignatureError::Webview(format!("native signature host destroy failed: {error}"))
    });
    if let Some((identity, latch, app)) = cleanup {
        reconcile_macos_cleanup_on_ui(identity, latch, &app);
    }
    destroy_result
}

#[cfg(target_os = "macos")]
pub(crate) fn reconcile_macos_cleanup_on_ui(
    identity: MacPolicyIdentity,
    latch: Arc<MacCleanupLatch>,
    app: &tauri::AppHandle<tauri::Wry>,
) {
    enum CleanupAction {
        None,
        OwnerMismatch,
        ReconcileAgain,
        StartRemoval,
        Acknowledge(Arc<CreationTicket>),
    }

    let action = RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let MainThreadSignatureSlot::Destroying(destroying) = &mut *slot else {
            return CleanupAction::OwnerMismatch;
        };
        if destroying.generation != identity.generation
            || destroying.operation_id != identity.operation_id
        {
            return CleanupAction::OwnerMismatch;
        }
        let Some(owner) = destroying.late_policy.as_mut() else {
            return CleanupAction::OwnerMismatch;
        };
        if owner.identity != identity || !Arc::ptr_eq(&owner.latch, &latch) {
            return CleanupAction::OwnerMismatch;
        }
        match latch.cleanup_completion() {
            CleanupCompletion::VerifiedAbsent => {
                if owner.acknowledge_verified_absence(&identity, &latch) {
                    CleanupAction::Acknowledge(Arc::clone(&destroying.ticket))
                } else {
                    CleanupAction::OwnerMismatch
                }
            }
            CleanupCompletion::Failed => CleanupAction::None,
            CleanupCompletion::Pending => match latch.compile_state() {
                MacCompileState::NotStarted => {
                    if latch.cancel_before_compile()
                        && latch.complete_cleanup(CleanupCompletion::VerifiedAbsent)
                    {
                        CleanupAction::ReconcileAgain
                    } else {
                        CleanupAction::None
                    }
                }
                MacCompileState::InFlight => CleanupAction::None,
                MacCompileState::Failed => {
                    if latch.complete_cleanup(CleanupCompletion::VerifiedAbsent) {
                        CleanupAction::ReconcileAgain
                    } else {
                        CleanupAction::None
                    }
                }
                MacCompileState::Succeeded | MacCompileState::UnknownAffinity => {
                    CleanupAction::StartRemoval
                }
            },
        }
    });

    match action {
        CleanupAction::None => {}
        CleanupAction::OwnerMismatch => {
            super::webview_resource_policy::macos::mark_policy_cleanup_fault();
            eprintln!("macOS cleanup callback had no exact late policy owner");
        }
        CleanupAction::ReconcileAgain => reconcile_macos_cleanup_on_ui(identity, latch, app),
        CleanupAction::StartRemoval => {
            if let Err(error) =
                super::webview_resource_policy::macos::begin_macos_store_removal_on_ui(
                    app, &identity, &latch,
                )
            {
                let _ = latch.complete_cleanup(CleanupCompletion::Failed);
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                eprintln!("macOS content-rule removal could not start: {error}");
            }
        }
        CleanupAction::Acknowledge(ticket) => {
            if !super::signature_webview::clear_late_policy_tombstone(
                identity.generation,
                identity.operation_id,
            ) {
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                eprintln!("macOS verified rule absence had no matching tombstone");
                return;
            }
            let matched = RAW_SIGNATURE_SLOT.with(|slot| {
                let mut slot = slot.borrow_mut();
                let MainThreadSignatureSlot::Destroying(destroying) = &mut *slot else {
                    return false;
                };
                if destroying.generation != identity.generation
                    || destroying.operation_id != identity.operation_id
                    || !destroying.late_policy.as_ref().is_some_and(|owner| {
                        owner.acknowledged()
                            && owner.identity == identity
                            && Arc::ptr_eq(&owner.latch, &latch)
                    })
                {
                    return false;
                }
                destroying.late_policy.take();
                destroying.ticket.mark_policy_cleanup();
                if super::signature_webview::generation_tombstones_empty(identity.generation) {
                    destroying.ticket.mark_tombstones_empty();
                }
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor.borrow_mut().acknowledge(
                        identity.generation,
                        identity.operation_id,
                        TeardownEvent::PolicyCleanup,
                    );
                });
                true
            });
            if matched {
                finalize_destroyed_slot(&ticket);
            } else {
                super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                eprintln!("macOS cleanup acknowledgement lost its exact late owner");
            }
        }
    }
}

#[cfg(windows)]
fn begin_destroy_without_webview(
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    trace: &mut UiCreationTrace,
) {
    ticket.cancel();
    ticket.mark_policy_cleanup();
    ticket.mark_tombstones_empty();
    trace.record_cancelled();
    UI_ACTOR_MODEL.with(|actor| {
        let _ = actor
            .borrow_mut()
            .request_destroy(ticket.generation, ticket.operation_id);
        let _ = actor.borrow_mut().acknowledge(
            ticket.generation,
            ticket.operation_id,
            TeardownEvent::PolicyCleanup,
        );
    });
    RAW_SIGNATURE_SLOT.with(|slot| {
        *slot.borrow_mut() =
            MainThreadSignatureSlot::Destroying(DestroyingMainThreadSignatureInstance {
                generation: ticket.generation,
                operation_id: ticket.operation_id,
                host_label: ticket.host_label().to_string(),
                ticket: Arc::clone(&ticket),
            });
    });
    let _ = host.destroy();
}

pub(crate) async fn evaluate_raw_signature_host(
    app: &tauri::AppHandle<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    operation: Arc<OperationTicket>,
    purpose: EvaluationPurpose,
    script: String,
) -> Result<String, SignatureError> {
    let (schedule_sender, schedule_receiver) = tokio::sync::oneshot::channel();
    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
    let result_sender = Arc::new(Mutex::new(Some(result_sender)));
    let callback_sender = Arc::clone(&result_sender);
    let callback_operation = Arc::clone(&operation);
    let generation = ticket.generation;
    let operation_id = operation.operation_id;
    app.run_on_main_thread(move || {
        let schedule_result = RAW_SIGNATURE_SLOT.with(|slot| {
            let slot = slot.borrow();
            let MainThreadSignatureSlot::Ready(instance) = &*slot else {
                return Err(SignatureError::NotReady);
            };
            if instance.generation != generation || instance.ticket.is_cancelled() {
                return Err(SignatureError::StaleCallback);
            }
            let delay_signature_callback = purpose == EvaluationPurpose::Signature
                && instance.scenario_fault == ScenarioFault::SignCallbackDelay;
            instance
                .webview
                .evaluate_script_with_callback(&script, move |raw| {
                    if delay_signature_callback {
                        let delayed_operation = Arc::clone(&callback_operation);
                        let delayed_sender = Arc::clone(&callback_sender);
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(SCENARIO_SIGN_CALLBACK_DELAY).await;
                            if !delayed_operation.accepts(generation, operation_id) {
                                mark_isolated_delayed_callback(generation);
                                return;
                            }
                            if let Some(sender) = delayed_sender
                                .lock()
                                .expect("raw signature result mutex poisoned")
                                .take()
                            {
                                let _ = sender.send(raw);
                            }
                        });
                    } else if callback_operation.accepts(generation, operation_id)
                        && let Some(sender) = callback_sender
                            .lock()
                            .expect("raw signature result mutex poisoned")
                            .take()
                    {
                        let _ = sender.send(raw);
                    }
                })
                .map_err(|error| {
                    SignatureError::Webview(format!("signature evaluation failed: {error}"))
                })
        });
        let _ = schedule_sender.send(schedule_result);
    })
    .map_err(|_| SignatureError::Webview("signature evaluation dispatch failed".into()))?;
    schedule_receiver
        .await
        .map_err(|_| SignatureError::Evaluation)??;
    result_receiver
        .await
        .map_err(|_| SignatureError::StaleCallback)
}

pub(crate) async fn current_raw_signature_url(
    app: &tauri::AppHandle<tauri::Wry>,
    ticket: Arc<CreationTicket>,
) -> Result<String, SignatureError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = RAW_SIGNATURE_SLOT.with(|slot| {
            let slot = slot.borrow();
            let MainThreadSignatureSlot::Ready(instance) = &*slot else {
                return Err(SignatureError::NotReady);
            };
            if instance.generation != ticket.generation {
                return Err(SignatureError::StaleCallback);
            }
            instance.webview.url().map_err(|error| {
                SignatureError::Webview(format!("signature URL query failed: {error}"))
            })
        });
        let _ = sender.send(result);
    })
    .map_err(|_| SignatureError::Webview("signature URL dispatch failed".into()))?;
    receiver
        .await
        .map_err(|_| SignatureError::Webview("signature URL callback stopped".into()))?
}

pub(crate) async fn raw_signature_host_probe_snapshot(
    app: &tauri::AppHandle<tauri::Wry>,
    ticket: Arc<CreationTicket>,
) -> Result<RawHostProbeSnapshot, SignatureError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = RAW_SIGNATURE_SLOT.with(|slot| {
            let slot = slot.borrow();
            let MainThreadSignatureSlot::Ready(instance) = &*slot else {
                return Err(SignatureError::NotReady);
            };
            if instance.generation != ticket.generation
                || instance.operation_id != ticket.operation_id
            {
                return Err(SignatureError::StaleCallback);
            }
            Ok(RawHostProbeSnapshot {
                generation: instance.generation,
                operation_id: instance.operation_id,
                host_label: instance.ticket.host_label().to_string(),
                managed_webviews_empty: instance.host.webviews().is_empty(),
                counters: instance.counters.snapshot(),
            })
        });
        let _ = sender.send(result);
    })
    .map_err(|_| SignatureError::Webview("signature snapshot dispatch failed".into()))?;
    receiver
        .await
        .map_err(|_| SignatureError::Webview("signature snapshot callback stopped".into()))?
}

pub(crate) async fn destroy_raw_signature_host(
    app: &tauri::AppHandle<tauri::Wry>,
    ticket: Arc<CreationTicket>,
) -> Result<(), SignatureError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let dispatch_ticket = Arc::clone(&ticket);
    app.run_on_main_thread(move || {
        let result = destroy_generation_on_ui(&dispatch_ticket);
        let _ = sender.send(result);
    })
    .map_err(|_| SignatureError::Webview("signature destroy dispatch failed".into()))?;
    receiver
        .await
        .map_err(|_| SignatureError::Webview("signature destroy callback stopped".into()))??;
    Ok(())
}

fn destroy_generation_on_ui(ticket: &Arc<CreationTicket>) -> Result<(), SignatureError> {
    ticket.cancel();
    let active = RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        mem::replace(&mut *slot, MainThreadSignatureSlot::Empty)
    });
    match active {
        MainThreadSignatureSlot::Empty => Ok(()),
        MainThreadSignatureSlot::Pending(mut pending)
            if pending.generation == ticket.generation
                && pending.operation_id == ticket.operation_id =>
        {
            #[cfg(target_os = "macos")]
            {
                return transition_macos_pending_to_destroying(
                    pending,
                    SignatureError::Cancelled,
                    true,
                );
            }

            #[cfg(windows)]
            {
                pending.trace.record_cancelled();
                let _ = &pending.builder_spec;
                let _ = &pending.events;
                let _ = pending.policy_build.synchronous_registration_started;
                ticket.mark_policy_cleanup();
                ticket.mark_tombstones_empty();
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor
                        .borrow_mut()
                        .request_destroy(ticket.generation, ticket.operation_id);
                    let _ = actor.borrow_mut().acknowledge(
                        ticket.generation,
                        ticket.operation_id,
                        TeardownEvent::PolicyCleanup,
                    );
                });
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(
                        DestroyingMainThreadSignatureInstance {
                            generation: ticket.generation,
                            operation_id: ticket.operation_id,
                            host_label: ticket.host_label().to_string(),
                            ticket: Arc::clone(ticket),
                        },
                    );
                });
                pending.host.destroy().map_err(|error| {
                    SignatureError::Webview(format!(
                        "native signature host destroy failed: {error}"
                    ))
                })
            }

            #[cfg(not(any(windows, target_os = "macos")))]
            {
                pending.trace.record_cancelled();
                let _ = &pending.builder_spec;
                let _ = &pending.events;
                let _ = pending.policy_build;
                ticket.mark_policy_cleanup();
                ticket.mark_tombstones_empty();
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor
                        .borrow_mut()
                        .request_destroy(ticket.generation, ticket.operation_id);
                    let _ = actor.borrow_mut().acknowledge(
                        ticket.generation,
                        ticket.operation_id,
                        TeardownEvent::PolicyCleanup,
                    );
                });
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(
                        DestroyingMainThreadSignatureInstance {
                            generation: ticket.generation,
                            operation_id: ticket.operation_id,
                            host_label: ticket.host_label().to_string(),
                            ticket: Arc::clone(ticket),
                        },
                    );
                });
                pending.host.destroy().map_err(|error| {
                    SignatureError::Webview(format!(
                        "native signature host destroy failed: {error}"
                    ))
                })
            }
        }
        MainThreadSignatureSlot::Ready(mut ready)
            if ready.generation == ticket.generation
                && ready.operation_id == ticket.operation_id =>
        {
            ready.trace.record_cancelled();

            #[cfg(target_os = "macos")]
            {
                let app = ready.host.app_handle().clone();
                let policy_result = ready.policy.into_late_owner_on_ui();
                let (late_policy, cleanup_error, policy_already_clean) = match policy_result {
                    Ok(Some(owner)) => (Some(owner), None, false),
                    Ok(None) => (None, None, true),
                    Err(error) => {
                        super::webview_resource_policy::macos::mark_policy_cleanup_fault();
                        (None, Some(error), false)
                    }
                };
                drop(ready.webview);
                if let Some(owner) = &late_policy {
                    super::signature_webview::add_late_policy_tombstone(
                        owner.identity.generation,
                        owner.identity.operation_id,
                        owner.identity.identifier.clone(),
                    );
                }
                if policy_already_clean {
                    ticket.mark_policy_cleanup();
                    ticket.mark_tombstones_empty();
                }
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor
                        .borrow_mut()
                        .request_destroy(ticket.generation, ticket.operation_id);
                    if policy_already_clean {
                        let _ = actor.borrow_mut().acknowledge(
                            ticket.generation,
                            ticket.operation_id,
                            TeardownEvent::PolicyCleanup,
                        );
                    }
                });
                let cleanup = late_policy
                    .as_ref()
                    .map(|owner| (owner.identity.clone(), Arc::clone(&owner.latch)));
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(
                        DestroyingMainThreadSignatureInstance {
                            generation: ticket.generation,
                            operation_id: ticket.operation_id,
                            host_label: ticket.host_label().to_string(),
                            ticket: Arc::clone(ticket),
                            late_policy,
                        },
                    );
                });
                let destroy_result = ready.host.destroy().map_err(|error| {
                    SignatureError::Webview(format!(
                        "native signature host destroy failed: {error}"
                    ))
                });
                if let Some((identity, latch)) = cleanup {
                    reconcile_macos_cleanup_on_ui(identity, latch, &app);
                }
                if let Some(error) = cleanup_error {
                    return Err(error);
                }
                return destroy_result;
            }

            #[cfg(windows)]
            {
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(
                        DestroyingMainThreadSignatureInstance {
                            generation: ticket.generation,
                            operation_id: ticket.operation_id,
                            host_label: ticket.host_label().to_string(),
                            ticket: Arc::clone(ticket),
                        },
                    );
                });
                let cleanup_result = ready.policy.uninstall();
                let policy_already_clean = matches!(cleanup_result, Ok(true));
                let _ = ready.counters.snapshot();
                drop(ready.webview);
                if policy_already_clean {
                    ticket.mark_policy_cleanup();
                    ticket.mark_tombstones_empty();
                }
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor
                        .borrow_mut()
                        .request_destroy(ticket.generation, ticket.operation_id);
                    if policy_already_clean {
                        let _ = actor.borrow_mut().acknowledge(
                            ticket.generation,
                            ticket.operation_id,
                            TeardownEvent::PolicyCleanup,
                        );
                    }
                });
                let destroy_result = ready.host.destroy().map_err(|error| {
                    SignatureError::Webview(format!(
                        "native signature host destroy failed: {error}"
                    ))
                });
                cleanup_result.map(|_| ()).and(destroy_result)
            }

            #[cfg(not(any(windows, target_os = "macos")))]
            {
                let _ = ready.counters.snapshot();
                drop(ready.policy);
                drop(ready.webview);
                ticket.mark_policy_cleanup();
                ticket.mark_tombstones_empty();
                UI_ACTOR_MODEL.with(|actor| {
                    let _ = actor
                        .borrow_mut()
                        .request_destroy(ticket.generation, ticket.operation_id);
                    let _ = actor.borrow_mut().acknowledge(
                        ticket.generation,
                        ticket.operation_id,
                        TeardownEvent::PolicyCleanup,
                    );
                });
                RAW_SIGNATURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(
                        DestroyingMainThreadSignatureInstance {
                            generation: ticket.generation,
                            operation_id: ticket.operation_id,
                            host_label: ticket.host_label().to_string(),
                            ticket: Arc::clone(ticket),
                        },
                    );
                });
                ready.host.destroy().map_err(|error| {
                    SignatureError::Webview(format!(
                        "native signature host destroy failed: {error}"
                    ))
                })
            }
        }
        MainThreadSignatureSlot::Destroying(destroying)
            if destroying.generation == ticket.generation
                && destroying.operation_id == ticket.operation_id =>
        {
            RAW_SIGNATURE_SLOT.with(|slot| {
                *slot.borrow_mut() = MainThreadSignatureSlot::Destroying(destroying);
            });
            Ok(())
        }
        other => {
            RAW_SIGNATURE_SLOT.with(|slot| *slot.borrow_mut() = other);
            Err(SignatureError::StaleCallback)
        }
    }
}

fn finalize_destroyed_slot(ticket: &Arc<CreationTicket>) {
    if !ticket.teardown_complete() {
        return;
    }
    RAW_SIGNATURE_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if matches!(
            &*slot,
            MainThreadSignatureSlot::Destroying(destroying)
                if destroying.generation == ticket.generation
                    && destroying.operation_id == ticket.operation_id
                    && destroying.host_label == ticket.host_label()
                    && Arc::ptr_eq(&destroying.ticket, ticket)
        ) {
            *slot = MainThreadSignatureSlot::Empty;
            RAW_SIGNATURE_SLOT_ACTIVE.store(false, Ordering::Release);
            ticket.mark_slot_empty();
        }
    });
}

pub(crate) fn signature_slot_active() -> bool {
    RAW_SIGNATURE_SLOT_ACTIVE.load(Ordering::Acquire)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FinalExitReadiness {
    on_main_thread: bool,
    slot_empty: bool,
    tombstones_empty: bool,
    callbacks_clean: bool,
}

impl FinalExitReadiness {
    pub(crate) fn new(
        on_main_thread: bool,
        slot_empty: bool,
        tombstones_empty: bool,
        callbacks_clean: bool,
    ) -> Self {
        Self {
            on_main_thread,
            slot_empty,
            tombstones_empty,
            callbacks_clean,
        }
    }

    pub(crate) fn allows_exit(self) -> bool {
        self.on_main_thread && self.slot_empty && self.tombstones_empty && self.callbacks_clean
    }
}

pub(crate) fn authoritative_final_exit_readiness_on_ui() -> FinalExitReadiness {
    #[cfg(target_os = "macos")]
    {
        let on_main_thread = objc2::MainThreadMarker::new().is_some();
        if !on_main_thread {
            return FinalExitReadiness::new(false, false, false, false);
        }
        let slot_empty = RAW_SIGNATURE_SLOT
            .with(|slot| matches!(&*slot.borrow(), MainThreadSignatureSlot::Empty));
        let tombstones_empty =
            super::signature_webview::late_policy_tombstone_identifiers().is_empty();
        let callbacks_clean =
            super::webview_resource_policy::assert_policy_cleanup_callbacks_clean().is_ok();
        FinalExitReadiness::new(
            on_main_thread,
            slot_empty,
            tombstones_empty,
            callbacks_clean,
        )
    }

    #[cfg(windows)]
    {
        let slot_empty = RAW_SIGNATURE_SLOT
            .with(|slot| matches!(&*slot.borrow(), MainThreadSignatureSlot::Empty));
        FinalExitReadiness::new(true, slot_empty, true, true)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let slot_empty = RAW_SIGNATURE_SLOT
            .with(|slot| matches!(&*slot.borrow(), MainThreadSignatureSlot::Empty));
        FinalExitReadiness::new(true, slot_empty, true, true)
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MacosFinalExitDecision {
    retain_owner: bool,
    clear_active: bool,
}

#[cfg(any(target_os = "macos", test))]
fn macos_final_exit_decision(
    on_main_thread: bool,
    slot_empty: bool,
    tombstones_empty: bool,
    callbacks_clean: bool,
) -> MacosFinalExitDecision {
    let safe = on_main_thread && slot_empty && tombstones_empty && callbacks_clean;
    MacosFinalExitDecision {
        retain_owner: !slot_empty,
        clear_active: safe,
    }
}

pub(crate) fn final_exit_drop() {
    #[cfg(target_os = "macos")]
    {
        let readiness = authoritative_final_exit_readiness_on_ui();
        if !readiness.on_main_thread {
            eprintln!("macOS final signature slot cleanup was requested off the UI thread");
            return;
        }
        let decision = macos_final_exit_decision(
            readiness.on_main_thread,
            readiness.slot_empty,
            readiness.tombstones_empty,
            readiness.callbacks_clean,
        );
        if decision.retain_owner {
            eprintln!("final signature slot cleanup retained an outstanding UI owner");
        }
        if decision.clear_active {
            RAW_SIGNATURE_SLOT_ACTIVE.store(false, Ordering::Release);
        } else {
            eprintln!(
                "final signature slot cleanup remained blocked by slot, tombstone, or callback state"
            );
        }
        return;
    }

    #[cfg(not(target_os = "macos"))]
    {
        RAW_SIGNATURE_SLOT.with(|slot| {
            let previous = mem::replace(&mut *slot.borrow_mut(), MainThreadSignatureSlot::Empty);
            drop(previous);
        });
        RAW_SIGNATURE_SLOT_ACTIVE.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        ActorModel, ActorPhase, CreationStep, CreationTicket, HostArrivalDecision, HostCounts,
        InitGate, OperationTicket, RawHostProfile, RawResourcePolicyProfile, RawWebViewBuilderSpec,
        TeardownEvent, UiCreationTrace, macos_final_exit_decision,
    };

    #[test]
    fn signature_webview_macos_abnormal_final_exit_attempts_cleanup_without_false_ack() {
        let safe = macos_final_exit_decision(true, true, true, true);
        assert!(!safe.retain_owner);
        assert!(safe.clear_active);

        for (on_main, slot_empty, tombstones_empty, callbacks_clean) in [
            (false, true, true, true),
            (true, false, true, true),
            (true, true, false, true),
            (true, true, true, false),
        ] {
            let unsafe_exit =
                macos_final_exit_decision(on_main, slot_empty, tombstones_empty, callbacks_clean);
            assert_eq!(unsafe_exit.retain_owner, !slot_empty);
            assert!(!unsafe_exit.clear_active);
        }
    }

    #[test]
    fn signature_webview_feature_profile_is_explicit_and_cannot_replace_live_defaults() {
        let live = RawHostProfile::live();
        assert_eq!(live.navigation_url, super::GD_PAGE_URL);
        assert_eq!(live.resource_policy, RawResourcePolicyProfile::Live);

        let controlled = RawHostProfile::controlled(
            "https://127.0.0.1:50001/".into(),
            RawResourcePolicyProfile::Counterfactual,
        )
        .unwrap();
        assert_eq!(controlled.navigation_url, "https://127.0.0.1:50001/");
        assert_eq!(
            controlled.resource_policy,
            RawResourcePolicyProfile::Counterfactual
        );
        assert!(
            RawHostProfile::controlled(
                "https://music.gdstudio.xyz/".into(),
                RawResourcePolicyProfile::ProtectedCanary,
            )
            .is_err()
        );
        assert!(
            RawHostProfile::controlled(
                "http://127.0.0.1:50001/".into(),
                RawResourcePolicyProfile::ProtectedCanary,
            )
            .is_err()
        );
    }

    #[test]
    fn signature_webview_actor_requires_policy_and_official_finish_before_polling() {
        let mut actor = ActorModel::default();
        actor.begin(7, 11).unwrap();

        assert_eq!(actor.phase(), ActorPhase::Pending);
        assert!(!actor.init_gate().may_poll());
        assert!(!actor.record_page_finished("about:blank"));
        assert!(!actor.record_page_finished("https://music.gdstudio.xyz/"));

        actor.policy_ready(7, 11).unwrap();
        assert!(!actor.init_gate().may_poll());
        assert!(actor.record_page_finished("https://music.gdstudio.xyz/"));
        assert!(actor.init_gate().may_poll());
        assert_eq!(actor.init_gate(), InitGate::OfficialFinishedAfterPolicy);
    }

    #[test]
    fn signature_webview_actor_cancellation_and_late_callbacks_are_generation_isolated() {
        let mut actor = ActorModel::default();
        let first = actor.begin(1, 1).unwrap();
        assert_eq!(first.host_label, "gd-signature-host-feasibility-1-1");
        actor.cancel(1, 1).unwrap();
        assert_eq!(actor.phase(), ActorPhase::Destroying);
        assert!(!actor.policy_ready(1, 1).unwrap());
        assert!(!actor.accepts_callback(1, 1));

        actor
            .acknowledge(1, 1, TeardownEvent::NativeDestroyed)
            .unwrap();
        assert_eq!(actor.phase(), ActorPhase::Destroying);
        actor
            .acknowledge(1, 1, TeardownEvent::PolicyCleanup)
            .unwrap();
        assert_eq!(actor.phase(), ActorPhase::Empty);

        let second = actor.begin(2, 2).unwrap();
        assert_eq!(second.host_label, "gd-signature-host-feasibility-2-2");
        assert!(!actor.accepts_callback(1, 1));
        assert!(actor.accepts_callback(2, 2));
    }

    #[test]
    fn signature_webview_actor_duplicate_destroy_has_one_composite_ack_and_retry_waits() {
        let mut actor = ActorModel::default();
        actor.begin(4, 9).unwrap();
        actor.policy_ready(4, 9).unwrap();
        actor.mark_ready(4, 9).unwrap();

        assert!(actor.request_destroy(4, 9).unwrap());
        assert!(!actor.request_destroy(4, 9).unwrap());
        assert!(actor.begin(5, 10).is_err());

        actor
            .acknowledge(4, 9, TeardownEvent::PolicyCleanup)
            .unwrap();
        assert_eq!(actor.phase(), ActorPhase::Destroying);
        actor
            .acknowledge(4, 9, TeardownEvent::NativeDestroyed)
            .unwrap();
        actor
            .acknowledge(4, 9, TeardownEvent::NativeDestroyed)
            .unwrap();
        assert_eq!(actor.phase(), ActorPhase::Empty);
        assert_eq!(actor.composite_teardown_count(), 1);
        assert!(actor.begin(5, 10).is_ok());
    }

    #[test]
    fn signature_webview_actor_twenty_cycles_return_all_counts_to_baseline() {
        let mut actor = ActorModel::default();
        assert_eq!(actor.host_counts(), HostCounts::default());

        for generation in 1..=20 {
            actor.begin(generation, generation).unwrap();
            actor.policy_ready(generation, generation).unwrap();
            actor.mark_ready(generation, generation).unwrap();
            actor.request_destroy(generation, generation).unwrap();
            actor
                .acknowledge(generation, generation, TeardownEvent::NativeDestroyed)
                .unwrap();
            actor
                .acknowledge(generation, generation, TeardownEvent::PolicyCleanup)
                .unwrap();
        }

        assert_eq!(actor.phase(), ActorPhase::Empty);
        assert_eq!(actor.host_counts(), HostCounts::default());
        assert_eq!(actor.composite_teardown_count(), 20);
    }

    #[test]
    fn signature_webview_raw_builder_spec_is_fixed_and_has_no_application_bridge() {
        let spec = RawWebViewBuilderSpec::new(3, 8);
        assert_eq!(spec.id, "gd-signature-raw-wry");
        assert_eq!(spec.initial_url, "about:blank");
        assert!(!spec.visible);
        assert!(!spec.focused);
        assert!(!spec.devtools);
        assert!(spec.incognito);
        assert!(!spec.clipboard);
        assert!(!spec.autofill);
        assert!(spec.deny_new_windows);
        assert!(spec.deny_downloads);
        assert_eq!(spec.generation, 3);
        assert_eq!(spec.operation_id, 8);
        assert_eq!(spec.initialization_script_count, 0);
        assert!(!spec.application_ipc_handler);
        assert!(!spec.custom_protocol);
    }

    #[test]
    fn signature_webview_creation_cancellation_never_reaches_network_navigation() {
        let ordered = [
            CreationStep::PendingInserted,
            CreationStep::RawChildBuilt,
            CreationStep::NativeInterfacesFound,
            CreationStep::PolicyInstalled,
            CreationStep::ReadyTransition,
        ];
        for cancelled_after in ordered {
            let trace = UiCreationTrace::run_with_cancellation(cancelled_after);
            assert!(trace.steps().contains(&CreationStep::DestroyRequested));
            assert!(!trace.steps().contains(&CreationStep::NetworkNavigation));
            assert!(trace.was_cancelled());
        }

        let success = UiCreationTrace::run_successfully();
        assert_eq!(
            success.steps().last(),
            Some(&CreationStep::NetworkNavigation)
        );
        assert!(
            success.position(CreationStep::PolicyInstalled)
                < success.position(CreationStep::NetworkNavigation)
        );
    }

    #[tokio::test]
    async fn signature_webview_creation_ticket_has_distinct_native_and_composite_barriers() {
        let ticket = CreationTicket::new(12, 20);
        assert_eq!(ticket.host_label(), "gd-signature-host-feasibility-12-20");
        assert_eq!(
            ticket.host_arrival_decision(),
            HostArrivalDecision::BuildRawChild
        );
        ticket.cancel();
        assert_eq!(
            ticket.host_arrival_decision(),
            HostArrivalDecision::DestroyWithoutBuilding
        );

        ticket.mark_native_destroyed();
        ticket.wait_native_destroyed().await;
        assert!(!ticket.teardown_complete());
        ticket.mark_manager_absent();
        ticket.mark_policy_cleanup();
        assert!(!ticket.teardown_complete());
        ticket.mark_tombstones_empty();
        ticket.wait_teardown_complete().await;
        assert!(ticket.teardown_complete());
        assert_eq!(ticket.composite_ack_count(), 1);
        let audit = ticket.teardown_audit();
        assert_eq!(audit.generation, 12);
        assert_eq!(audit.operation_id, 20);
        assert_eq!(
            audit.ordered_event_names(),
            [
                "native-destroyed",
                "manager-host-absent",
                "policy-cleanup-acknowledged",
                "policy-tombstones-empty",
                "teardown-complete",
            ]
        );
        assert!(audit.is_complete_and_unique());

        ticket.mark_native_destroyed();
        ticket.mark_manager_absent();
        ticket.mark_policy_cleanup();
        ticket.mark_tombstones_empty();
        assert_eq!(ticket.composite_ack_count(), 1);
        assert_eq!(ticket.teardown_audit(), audit);
    }

    #[tokio::test]
    async fn signature_webview_teardown_waits_for_the_final_slot_empty_ack() {
        let ticket = CreationTicket::new(21, 34);
        let waiter_ticket = Arc::clone(&ticket);
        let waiter = tokio::spawn(async move {
            waiter_ticket.wait_slot_empty().await;
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        ticket.mark_slot_empty();
        waiter.await.unwrap();
    }

    #[test]
    fn signature_webview_operation_ticket_rejects_cancelled_and_stale_callbacks() {
        let operation = OperationTicket::new(5, 9);
        assert!(operation.accepts(5, 9));
        assert!(!operation.accepts(4, 9));
        assert!(!operation.accepts(5, 10));
        operation.cancel();
        assert!(!operation.accepts(5, 9));
    }
}
