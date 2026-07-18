use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use super::signature_host::{
    CreationTicket, EvaluationPurpose, FinalExitReadiness, GenerationTeardownAudit, HostEvent,
    OperationTicket, RawHostProbeSnapshot, RawHostProfile, create_raw_signature_host,
    current_raw_signature_url, destroy_raw_signature_host, evaluate_raw_signature_host,
    raw_signature_host_probe_snapshot,
};
use crate::music::contract::{EncodedComponent, SignatureValue};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use thiserror::Error;
use url::Url;

#[cfg(target_os = "macos")]
use std::{cell::RefCell, collections::BTreeMap};

#[cfg(test)]
use serde::de::DeserializeOwned;
#[cfg(test)]
use tokio::sync::oneshot;

pub const GD_PAGE_URL: &str = "https://music.gdstudio.xyz/";
pub const SIGNATURE_HOST_WINDOW_LABEL: &str = "gd-signature-host-feasibility";
pub const SIGNATURE_WEBVIEW_ID: &str = "gd-signature-raw-wry";
pub const INIT_TIMEOUT: Duration = Duration::from_secs(20);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);
pub const DESTROY_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const SCENARIO_INIT_CALLBACK_DELAY: Duration = Duration::from_secs(21);
pub(crate) const SCENARIO_SIGN_CALLBACK_DELAY: Duration = Duration::from_secs(10);
pub const MAX_SIGNATURE_BYTES: usize = 128;

#[cfg(target_os = "macos")]
thread_local! {
    static LATE_POLICY_TOMBSTONES: RefCell<BTreeMap<(u64, u64), String>> =
        const { RefCell::new(BTreeMap::new()) };
}

#[cfg(target_os = "macos")]
static LATE_POLICY_TOMBSTONE_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
pub(crate) fn add_late_policy_tombstone(generation: u64, operation_id: u64, identifier: String) {
    let inserted = LATE_POLICY_TOMBSTONES.with(|entries| {
        entries
            .borrow_mut()
            .insert((generation, operation_id), identifier)
            .is_none()
    });
    if inserted {
        LATE_POLICY_TOMBSTONE_COUNT.fetch_add(1, Ordering::Release);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn clear_late_policy_tombstone(generation: u64, operation_id: u64) -> bool {
    let removed = LATE_POLICY_TOMBSTONES.with(|entries| {
        entries
            .borrow_mut()
            .remove(&(generation, operation_id))
            .is_some()
    });
    if removed {
        let decremented = LATE_POLICY_TOMBSTONE_COUNT.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |count| count.checked_sub(1),
        );
        debug_assert!(decremented.is_ok());
    }
    removed
}

#[cfg(target_os = "macos")]
fn observed_late_policy_tombstones_empty() -> bool {
    LATE_POLICY_TOMBSTONE_COUNT.load(Ordering::Acquire) == 0
}

#[cfg(target_os = "macos")]
pub(crate) fn generation_tombstones_empty(generation: u64) -> bool {
    LATE_POLICY_TOMBSTONES.with(|entries| {
        !entries
            .borrow()
            .keys()
            .any(|(entry_generation, _)| *entry_generation == generation)
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn late_policy_tombstone_identifiers() -> Vec<String> {
    LATE_POLICY_TOMBSTONES.with(|entries| entries.borrow().values().cloned().collect())
}

#[derive(Clone, Copy, Debug)]
struct InitDeadline {
    deadline: Instant,
}

impl InitDeadline {
    fn from_start(start: Instant) -> Self {
        Self {
            deadline: start + INIT_TIMEOUT,
        }
    }

    fn start_now() -> Self {
        Self::from_start(Instant::now())
    }

    fn remaining_at(self, now: Instant) -> Option<Duration> {
        self.deadline
            .checked_duration_since(now)
            .filter(|left| !left.is_zero())
    }

    fn remaining(self) -> Result<Duration, SignatureError> {
        self.remaining_at(Instant::now())
            .ok_or(SignatureError::Timeout)
    }
}

#[derive(Clone, Debug, Error)]
pub enum SignatureError {
    #[error("signature runtime timed out")]
    Timeout,
    #[error("signature page origin was rejected")]
    OriginRejected,
    #[error("official signing function is unavailable")]
    MissingFunction,
    #[error("official signing function returned invalid data: {0}")]
    InvalidReturn(&'static str),
    #[error("signature JavaScript evaluation failed")]
    Evaluation,
    #[error("signature WebView failed: {0}")]
    Webview(String),
    #[error("signature operation was cancelled")]
    Cancelled,
    #[error("signature runtime is not ready")]
    NotReady,
    #[error("stale signature callback was isolated")]
    StaleCallback,
    #[error("signature teardown timed out")]
    DestroyTimeout,
    #[error("signature runtime is terminally poisoned: {0}")]
    TerminalPoisoned(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInitReport {
    pub generation: u64,
    pub operation_id: u64,
    pub host_label: String,
    pub webview_id: String,
    pub current_url: String,
    pub runtime_mode: String,
    pub webview_runtime_version: String,
    pub resource_policy_mode: String,
    pub strong_source_kinds_interface_available: bool,
    pub official_finished_before_polling: bool,
    pub policy_installed_before_first_network_navigation: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCleanup {
    AwaitingLate,
    ExitOnlyVerified,
    InvalidAudit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalReauditObservation {
    StillPending,
    CompletedValid,
    CompletedInvalid,
}

fn terminal_cleanup_after_reaudit(
    current: TerminalCleanup,
    observation: TerminalReauditObservation,
) -> TerminalCleanup {
    match (current, observation) {
        (TerminalCleanup::AwaitingLate, TerminalReauditObservation::CompletedValid) => {
            TerminalCleanup::ExitOnlyVerified
        }
        (TerminalCleanup::AwaitingLate, TerminalReauditObservation::CompletedInvalid) => {
            TerminalCleanup::InvalidAudit
        }
        (other, _) => other,
    }
}

fn terminal_cleanup_allows_exit(cleanup: TerminalCleanup) -> bool {
    cleanup == TerminalCleanup::ExitOnlyVerified
}

enum RuntimeState {
    Idle,
    Creating {
        ticket: Arc<CreationTicket>,
    },
    Ready {
        ticket: Arc<CreationTicket>,
    },
    Poisoned {
        ticket: Arc<CreationTicket>,
        reason: SignatureError,
    },
    Destroying {
        ticket: Arc<CreationTicket>,
    },
    TerminalPoisoned {
        ticket: Arc<CreationTicket>,
        reason: SignatureError,
        cleanup: TerminalCleanup,
    },
}

pub struct SignatureRuntime {
    app: tauri::AppHandle<tauri::Wry>,
    generation: AtomicU64,
    operation_id: AtomicU64,
    active: AtomicBool,
    shutdown_requested: ShutdownAdmissionGate,
    teardown_verified_for_exit: AtomicBool,
    state: tokio::sync::Mutex<RuntimeState>,
    completed_teardown_audit: CompletedTeardownAuditSlot,
}

#[derive(Default)]
struct ShutdownAdmissionState {
    shutdown_requested: bool,
    in_flight: usize,
}

#[derive(Default)]
struct ShutdownAdmissionGate {
    state: Mutex<ShutdownAdmissionState>,
    drained: tokio::sync::Notify,
}

struct ShutdownAdmission<'a> {
    gate: &'a ShutdownAdmissionGate,
    active: bool,
}

impl ShutdownAdmissionGate {
    fn begin(&self) -> Option<ShutdownAdmission<'_>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.shutdown_requested {
            return None;
        }
        state.in_flight += 1;
        Some(ShutdownAdmission {
            gate: self,
            active: true,
        })
    }

    fn request(&self) -> usize {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.shutdown_requested = true;
        state.in_flight
    }

    fn shutdown_requested(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .shutdown_requested
    }

    fn in_flight(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .in_flight
    }

    async fn wait_drained(&self) {
        loop {
            let notified = self.drained.notified();
            if self.in_flight() == 0 {
                return;
            }
            notified.await;
        }
    }
}

impl ShutdownAdmission<'_> {
    fn commit_allowed(&self) -> bool {
        !self.gate.shutdown_requested()
    }
}

impl Drop for ShutdownAdmission<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let drained = {
            let mut state = self
                .gate
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.in_flight = state.in_flight.saturating_sub(1);
            state.in_flight == 0
        };
        self.active = false;
        if drained {
            self.gate.drained.notify_waiters();
        }
    }
}

#[derive(Default)]
struct CompletedTeardownAuditSlot(Mutex<Option<GenerationTeardownAudit>>);

impl CompletedTeardownAuditSlot {
    fn record(&self, audit: GenerationTeardownAudit) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(audit);
    }

    fn take(&self, generation: u64, operation_id: u64) -> Option<GenerationTeardownAudit> {
        let mut audit = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if audit.as_ref().is_some_and(|audit| {
            audit.generation == generation && audit.operation_id == operation_id
        }) {
            audit.take()
        } else {
            None
        }
    }

    fn take_latest(&self) -> Option<GenerationTeardownAudit> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        usize::from(
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some(),
        )
    }
}

#[repr(u8)]
enum ExitCleanupPhase {
    Idle = 0,
    Pending = 1,
}

pub struct SignatureExitCoordinator {
    phase: AtomicU8,
    lifecycle_probe_armed: AtomicBool,
    lifecycle_cleanup_complete: AtomicBool,
    final_dispatch_failed: AtomicBool,
    lifecycle_cleanup_notify: tokio::sync::Notify,
}

impl SignatureExitCoordinator {
    pub fn new() -> Self {
        Self {
            phase: AtomicU8::new(ExitCleanupPhase::Idle as u8),
            lifecycle_probe_armed: AtomicBool::new(false),
            lifecycle_cleanup_complete: AtomicBool::new(false),
            final_dispatch_failed: AtomicBool::new(false),
            lifecycle_cleanup_notify: tokio::sync::Notify::new(),
        }
    }

    pub(crate) fn try_begin(&self) -> bool {
        self.phase
            .compare_exchange(
                ExitCleanupPhase::Idle as u8,
                ExitCleanupPhase::Pending as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn reset(&self) {
        self.phase
            .store(ExitCleanupPhase::Idle as u8, Ordering::Release);
    }

    pub(crate) fn arm_lifecycle_probe(&self) -> bool {
        self.lifecycle_cleanup_complete
            .store(false, Ordering::Release);
        self.final_dispatch_failed.store(false, Ordering::Release);
        self.lifecycle_probe_armed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn complete_lifecycle_probe_cleanup(&self) -> bool {
        if self
            .lifecycle_probe_armed
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.lifecycle_cleanup_complete
            .store(true, Ordering::Release);
        self.lifecycle_cleanup_notify.notify_waiters();
        true
    }

    fn fail_final_dispatch(&self) {
        self.final_dispatch_failed.store(true, Ordering::Release);
        if self
            .lifecycle_probe_armed
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.lifecycle_cleanup_complete
                .store(true, Ordering::Release);
            self.lifecycle_cleanup_notify.notify_waiters();
        }
    }

    #[cfg(test)]
    fn final_dispatch_failed(&self) -> bool {
        self.final_dispatch_failed.load(Ordering::Acquire)
    }

    pub(crate) async fn wait_lifecycle_probe_cleanup(&self) -> Result<(), SignatureError> {
        loop {
            let notified = self.lifecycle_cleanup_notify.notified();
            if self
                .lifecycle_cleanup_complete
                .swap(false, Ordering::AcqRel)
            {
                return if self.final_dispatch_failed.load(Ordering::Acquire) {
                    Err(SignatureError::Webview(
                        "signature cleanup final UI dispatch exhausted its bounded retries".into(),
                    ))
                } else {
                    Ok(())
                };
            }
            notified.await;
        }
    }
}

impl Default for SignatureExitCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn maybe_complete_teardown(
    coordinator: &SignatureExitCoordinator,
    safe_to_exit: bool,
    exit: impl FnOnce(),
) -> bool {
    coordinator.reset();
    if safe_to_exit {
        exit();
        true
    } else {
        false
    }
}

fn complete_authoritative_ui_exit(
    coordinator: &SignatureExitCoordinator,
    background_snapshot_safe: bool,
    readiness: FinalExitReadiness,
    exit: impl FnOnce(),
) -> bool {
    if !readiness.allows_exit() {
        coordinator.reset();
        coordinator.fail_final_dispatch();
        return false;
    }
    maybe_complete_teardown(coordinator, background_snapshot_safe, exit)
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum ReadinessEvaluation {
    Ok { ready: bool },
    Error,
}

impl SignatureRuntime {
    pub fn new(app: tauri::AppHandle<tauri::Wry>) -> Self {
        Self {
            app,
            generation: AtomicU64::new(0),
            operation_id: AtomicU64::new(0),
            active: AtomicBool::new(false),
            shutdown_requested: ShutdownAdmissionGate::default(),
            teardown_verified_for_exit: AtomicBool::new(false),
            state: tokio::sync::Mutex::new(RuntimeState::Idle),
            completed_teardown_audit: CompletedTeardownAuditSlot::default(),
        }
    }

    pub async fn initialize(&self) -> Result<SignatureInitReport, SignatureError> {
        self.initialize_with_profile(RawHostProfile::live()).await
    }

    pub async fn ensure_initialized(&self) -> Result<(), SignatureError> {
        {
            let state = self.state.lock().await;
            match &*state {
                RuntimeState::Ready { ticket } if !ticket.is_cancelled() => return Ok(()),
                RuntimeState::Idle => {}
                RuntimeState::TerminalPoisoned { reason, .. } => {
                    return Err(SignatureError::TerminalPoisoned(reason.to_string()));
                }
                _ => return Err(SignatureError::NotReady),
            }
        }
        self.initialize().await.map(|_| ())
    }

    pub(crate) async fn initialize_with_profile(
        &self,
        profile: RawHostProfile,
    ) -> Result<SignatureInitReport, SignatureError> {
        let admission = self
            .shutdown_requested
            .begin()
            .ok_or(SignatureError::NotReady)?;
        let deadline = InitDeadline::start_now();
        let ticket = {
            let mut state = self.state.lock().await;
            if !admission.commit_allowed() {
                return Err(SignatureError::NotReady);
            }
            match &*state {
                RuntimeState::Idle => {}
                RuntimeState::TerminalPoisoned { reason, .. } => {
                    return Err(SignatureError::TerminalPoisoned(reason.to_string()));
                }
                _ => return Err(SignatureError::NotReady),
            }
            let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
            let operation_id = self.next_operation_id();
            let ticket = CreationTicket::new(generation, operation_id);
            *state = RuntimeState::Creating {
                ticket: Arc::clone(&ticket),
            };
            self.teardown_verified_for_exit
                .store(false, Ordering::Release);
            self.active.store(true, Ordering::Release);
            ticket
        };
        drop(admission);

        let result = self
            .initialize_ticket(Arc::clone(&ticket), deadline, profile)
            .await;
        match result {
            Ok(report) => Ok(report),
            Err(error) => Err(self.poison_and_destroy(ticket, error).await),
        }
    }

    async fn initialize_ticket(
        &self,
        ticket: Arc<CreationTicket>,
        deadline: InitDeadline,
        profile: RawHostProfile,
    ) -> Result<SignatureInitReport, SignatureError> {
        let (initialization, mut events) = tokio::time::timeout(
            deadline.remaining()?,
            create_raw_signature_host(self.app.clone(), Arc::clone(&ticket), profile.clone()),
        )
        .await
        .map_err(|_| SignatureError::Timeout)??;

        if ticket.is_cancelled()
            || initialization.generation != ticket.generation
            || initialization.operation_id != ticket.operation_id
            || initialization.current_url != "about:blank"
        {
            return Err(SignatureError::Cancelled);
        }

        loop {
            let event = tokio::time::timeout(deadline.remaining()?, events.recv())
                .await
                .map_err(|_| SignatureError::Timeout)?
                .ok_or_else(|| {
                    SignatureError::Webview("signature host event channel stopped".into())
                })?;
            match event {
                HostEvent::PageFinished {
                    generation,
                    operation_id,
                    url,
                } if generation == ticket.generation
                    && operation_id == ticket.operation_id
                    && url == "about:blank" => {}
                HostEvent::PageFinished {
                    generation,
                    operation_id,
                    url,
                } if generation == ticket.generation && operation_id == ticket.operation_id => {
                    let url = Url::parse(&url).map_err(|_| SignatureError::OriginRejected)?;
                    if !profile.allows_navigation(&url) {
                        return Err(SignatureError::OriginRejected);
                    }
                    break;
                }
                HostEvent::PolicyFault {
                    generation,
                    operation_id,
                } if generation == ticket.generation && operation_id == ticket.operation_id => {
                    return Err(SignatureError::Webview(
                        "native signature resource policy fault".into(),
                    ));
                }
                _ => {}
            }
        }

        loop {
            while let Ok(event) = events.try_recv() {
                if matches!(
                    event,
                    HostEvent::PolicyFault {
                        generation,
                        operation_id,
                    } if generation == ticket.generation
                        && operation_id == ticket.operation_id
                ) {
                    return Err(SignatureError::Webview(
                        "native signature resource policy fault".into(),
                    ));
                }
            }
            let operation_id = self.next_operation_id();
            let operation = Arc::new(OperationTicket::new(ticket.generation, operation_id));
            let raw = tokio::time::timeout(
                deadline.remaining()?,
                evaluate_raw_signature_host(
                    &self.app,
                    Arc::clone(&ticket),
                    Arc::clone(&operation),
                    EvaluationPurpose::Probe,
                    readiness_script().to_string(),
                ),
            )
            .await
            .map_err(|_| {
                operation.cancel();
                SignatureError::Timeout
            })??;
            if !operation.accepts(ticket.generation, operation_id) {
                return Err(SignatureError::StaleCallback);
            }
            match serde_json::from_str::<ReadinessEvaluation>(&raw)
                .map_err(|_| SignatureError::Evaluation)?
            {
                ReadinessEvaluation::Ok { ready: true } => break,
                ReadinessEvaluation::Ok { ready: false } => {
                    let remaining = deadline.remaining()?;
                    tokio::time::sleep(remaining.min(Duration::from_millis(100))).await;
                }
                ReadinessEvaluation::Error => return Err(SignatureError::Evaluation),
            }
        }

        let current_url = current_raw_signature_url(&self.app, Arc::clone(&ticket)).await?;
        let parsed_url = Url::parse(&current_url).map_err(|_| SignatureError::OriginRejected)?;
        if !profile.allows_navigation(&parsed_url) {
            return Err(SignatureError::OriginRejected);
        }
        {
            let mut state = self.state.lock().await;
            match &*state {
                RuntimeState::Creating { ticket: current }
                    if Arc::ptr_eq(current, &ticket) && !ticket.is_cancelled() =>
                {
                    *state = RuntimeState::Ready {
                        ticket: Arc::clone(&ticket),
                    };
                }
                _ => return Err(SignatureError::Cancelled),
            }
        }

        Ok(SignatureInitReport {
            generation: ticket.generation,
            operation_id: ticket.operation_id,
            host_label: initialization.host_label,
            webview_id: SIGNATURE_WEBVIEW_ID.into(),
            current_url,
            runtime_mode: "native-host-raw-wry-0.55.1".into(),
            webview_runtime_version: initialization.policy.runtime_version,
            resource_policy_mode: initialization.policy.mode,
            strong_source_kinds_interface_available: initialization
                .policy
                .strong_source_kinds_interface_available,
            official_finished_before_polling: true,
            policy_installed_before_first_network_navigation: profile.resource_policy
                != super::signature_host::RawResourcePolicyProfile::Counterfactual,
        })
    }

    pub async fn sign(&self, input: &EncodedComponent) -> Result<SignatureValue, SignatureError> {
        let value = self.sign_text(input).await?;
        SignatureValue::try_from(value.as_str())
            .map_err(|_| SignatureError::InvalidReturn("rust-validation"))
    }

    pub async fn run_isolation_probe(
        &self,
    ) -> Result<super::signature_probe::IsolationReport, SignatureError> {
        super::signature_probe::run_isolation_probe(self).await
    }

    pub(crate) async fn sign_text(
        &self,
        input: &EncodedComponent,
    ) -> Result<String, SignatureError> {
        let ticket = {
            let state = self.state.lock().await;
            match &*state {
                RuntimeState::Ready { ticket } if !ticket.is_cancelled() => Arc::clone(ticket),
                RuntimeState::TerminalPoisoned { reason, .. } => {
                    return Err(SignatureError::TerminalPoisoned(reason.to_string()));
                }
                _ => return Err(SignatureError::NotReady),
            }
        };
        let operation_id = self.next_operation_id();
        let operation = Arc::new(OperationTicket::new(ticket.generation, operation_id));
        let result = tokio::time::timeout(
            CALL_TIMEOUT,
            evaluate_raw_signature_host(
                &self.app,
                Arc::clone(&ticket),
                Arc::clone(&operation),
                EvaluationPurpose::Signature,
                signature_script(input),
            ),
        )
        .await
        .map_err(|_| {
            operation.cancel();
            SignatureError::Timeout
        })
        .and_then(|result| result)
        .and_then(|raw| parse_signature_text(&raw));

        let result = match result {
            Ok(value) => {
                let state = self.state.lock().await;
                match &*state {
                    RuntimeState::Ready { ticket: current }
                        if Arc::ptr_eq(current, &ticket)
                            && operation.accepts(ticket.generation, operation_id) =>
                    {
                        Ok(value)
                    }
                    _ => Err(SignatureError::StaleCallback),
                }
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(value) => Ok(value),
            Err(error) => Err(self.poison_and_destroy(ticket, error).await),
        }
    }

    pub(crate) async fn evaluate_probe_script(
        &self,
        script: String,
    ) -> Result<String, SignatureError> {
        let ticket = {
            let state = self.state.lock().await;
            match &*state {
                RuntimeState::Ready { ticket } if !ticket.is_cancelled() => Arc::clone(ticket),
                RuntimeState::TerminalPoisoned { reason, .. } => {
                    return Err(SignatureError::TerminalPoisoned(reason.to_string()));
                }
                _ => return Err(SignatureError::NotReady),
            }
        };
        let operation_id = self.next_operation_id();
        let operation = Arc::new(OperationTicket::new(ticket.generation, operation_id));
        let result = tokio::time::timeout(
            CALL_TIMEOUT,
            evaluate_raw_signature_host(
                &self.app,
                Arc::clone(&ticket),
                Arc::clone(&operation),
                EvaluationPurpose::Probe,
                script,
            ),
        )
        .await
        .map_err(|_| {
            operation.cancel();
            SignatureError::Timeout
        })
        .and_then(|result| result);
        let result = match result {
            Ok(raw) => {
                let state = self.state.lock().await;
                match &*state {
                    RuntimeState::Ready { ticket: current }
                        if Arc::ptr_eq(current, &ticket)
                            && operation.accepts(ticket.generation, operation_id) =>
                    {
                        Ok(raw)
                    }
                    _ => Err(SignatureError::StaleCallback),
                }
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(raw) => Ok(raw),
            Err(error) => Err(self.poison_and_destroy(ticket, error).await),
        }
    }

    pub(crate) async fn probe_host_snapshot(&self) -> Result<RawHostProbeSnapshot, SignatureError> {
        let ticket = {
            let state = self.state.lock().await;
            match &*state {
                RuntimeState::Ready { ticket } if !ticket.is_cancelled() => Arc::clone(ticket),
                _ => return Err(SignatureError::NotReady),
            }
        };
        raw_signature_host_probe_snapshot(&self.app, ticket).await
    }

    pub(crate) fn app_handle(&self) -> &tauri::AppHandle<tauri::Wry> {
        &self.app
    }

    pub async fn destroy(&self) -> Result<(), SignatureError> {
        let (ticket, poisoned_reason, terminal_reaudit) = {
            let state = self.state.lock().await;
            match &*state {
                RuntimeState::Idle => return Ok(()),
                RuntimeState::TerminalPoisoned {
                    ticket,
                    reason,
                    cleanup,
                } => {
                    if *cleanup != TerminalCleanup::AwaitingLate {
                        return Err(SignatureError::TerminalPoisoned(format!(
                            "generation {}: {reason}",
                            ticket.generation
                        )));
                    }
                    (Arc::clone(ticket), Some(reason.clone()), true)
                }
                RuntimeState::Poisoned { ticket, reason } => {
                    (Arc::clone(ticket), Some(reason.clone()), false)
                }
                RuntimeState::Creating { ticket }
                | RuntimeState::Ready { ticket }
                | RuntimeState::Destroying { ticket } => (Arc::clone(ticket), None, false),
            }
        };
        if terminal_reaudit {
            return self
                .reaudit_terminal_ticket(
                    ticket,
                    poisoned_reason.expect("terminal re-audit retains its diagnostic"),
                )
                .await;
        }
        self.teardown_ticket(ticket).await?;
        match poisoned_reason {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    pub async fn retry(&self) -> Result<SignatureInitReport, SignatureError> {
        self.destroy().await?;
        self.initialize().await
    }

    async fn poison_and_destroy(
        &self,
        ticket: Arc<CreationTicket>,
        reason: SignatureError,
    ) -> SignatureError {
        {
            let mut state = self.state.lock().await;
            let current = match &*state {
                RuntimeState::Creating { ticket }
                | RuntimeState::Ready { ticket }
                | RuntimeState::Poisoned { ticket, .. }
                | RuntimeState::Destroying { ticket } => Some(ticket),
                _ => None,
            };
            if current.is_some_and(|current| Arc::ptr_eq(current, &ticket)) {
                *state = RuntimeState::Poisoned {
                    ticket: Arc::clone(&ticket),
                    reason: reason.clone(),
                };
            }
        }
        match self.teardown_ticket(ticket).await {
            Ok(()) => reason,
            Err(error) => error,
        }
    }

    async fn drive_and_verify_ticket(
        &self,
        ticket: Arc<CreationTicket>,
    ) -> Result<GenerationTeardownAudit, SignatureError> {
        ticket.cancel();
        let teardown = async {
            let request_result = destroy_raw_signature_host(&self.app, Arc::clone(&ticket)).await;
            ticket.wait_native_destroyed().await;
            ticket.wait_teardown_complete().await;
            ticket.wait_slot_empty().await;
            request_result?;
            if ticket.composite_ack_count() != 1 {
                return Err(SignatureError::Webview(
                    "signature teardown acknowledgement count was invalid".into(),
                ));
            }
            let audit = ticket.teardown_audit();
            self.completed_teardown_audit.record(audit.clone());
            if !audit.is_complete_and_unique() {
                return Err(SignatureError::Webview(
                    "signature teardown audit was incomplete or duplicated".into(),
                ));
            }
            Ok(audit)
        };
        tokio::time::timeout(DESTROY_TIMEOUT, teardown)
            .await
            .map_err(|_| SignatureError::DestroyTimeout)?
    }

    async fn reaudit_terminal_ticket(
        &self,
        ticket: Arc<CreationTicket>,
        original_reason: SignatureError,
    ) -> Result<(), SignatureError> {
        let result = self.drive_and_verify_ticket(Arc::clone(&ticket)).await;
        let mut state = self.state.lock().await;
        let RuntimeState::TerminalPoisoned {
            ticket: current,
            reason,
            cleanup,
        } = &mut *state
        else {
            return Err(SignatureError::StaleCallback);
        };
        if !Arc::ptr_eq(current, &ticket) || *cleanup != TerminalCleanup::AwaitingLate {
            return Err(SignatureError::StaleCallback);
        }
        match result {
            Ok(audit) => {
                *cleanup = terminal_cleanup_after_reaudit(
                    *cleanup,
                    if audit.is_complete_and_unique()
                        && !super::signature_host::signature_slot_active()
                    {
                        TerminalReauditObservation::CompletedValid
                    } else {
                        TerminalReauditObservation::CompletedInvalid
                    },
                );
                if terminal_cleanup_allows_exit(*cleanup) {
                    self.active.store(false, Ordering::Release);
                    self.teardown_verified_for_exit
                        .store(true, Ordering::Release);
                }
            }
            Err(error) => {
                if matches!(error, SignatureError::DestroyTimeout) {
                    *cleanup = terminal_cleanup_after_reaudit(
                        *cleanup,
                        TerminalReauditObservation::StillPending,
                    );
                } else {
                    *cleanup = terminal_cleanup_after_reaudit(
                        *cleanup,
                        TerminalReauditObservation::CompletedInvalid,
                    );
                    *reason = error;
                }
                self.active.store(true, Ordering::Release);
                self.teardown_verified_for_exit
                    .store(false, Ordering::Release);
            }
        }
        Err(SignatureError::TerminalPoisoned(format!(
            "generation {}: {original_reason}",
            ticket.generation
        )))
    }

    async fn teardown_ticket(&self, ticket: Arc<CreationTicket>) -> Result<(), SignatureError> {
        ticket.cancel();
        {
            let mut state = self.state.lock().await;
            let current = match &*state {
                RuntimeState::Creating { ticket }
                | RuntimeState::Ready { ticket }
                | RuntimeState::Poisoned { ticket, .. }
                | RuntimeState::Destroying { ticket } => Some(ticket),
                RuntimeState::TerminalPoisoned { ticket, reason, .. } => {
                    return Err(SignatureError::TerminalPoisoned(format!(
                        "generation {}: {reason}",
                        ticket.generation
                    )));
                }
                RuntimeState::Idle => return Ok(()),
            };
            if !current.is_some_and(|current| Arc::ptr_eq(current, &ticket)) {
                return Err(SignatureError::StaleCallback);
            }
            *state = RuntimeState::Destroying {
                ticket: Arc::clone(&ticket),
            };
        }

        match self.drive_and_verify_ticket(Arc::clone(&ticket)).await {
            Ok(_audit) => {
                let mut state = self.state.lock().await;
                if matches!(
                    &*state,
                    RuntimeState::Destroying { ticket: current }
                        if Arc::ptr_eq(current, &ticket)
                ) {
                    *state = RuntimeState::Idle;
                    self.active.store(false, Ordering::Release);
                    self.teardown_verified_for_exit
                        .store(true, Ordering::Release);
                }
                Ok(())
            }
            Err(reason) => {
                let cleanup = if matches!(reason, SignatureError::DestroyTimeout) {
                    TerminalCleanup::AwaitingLate
                } else {
                    TerminalCleanup::InvalidAudit
                };
                self.teardown_verified_for_exit
                    .store(false, Ordering::Release);
                self.active.store(true, Ordering::Release);
                let mut state = self.state.lock().await;
                if matches!(
                    &*state,
                    RuntimeState::Destroying { ticket: current }
                        if Arc::ptr_eq(current, &ticket)
                ) {
                    *state = RuntimeState::TerminalPoisoned {
                        ticket: Arc::clone(&ticket),
                        reason: reason.clone(),
                        cleanup,
                    };
                }
                Err(reason)
            }
        }
    }

    pub(crate) fn scenario_ids(&self) -> (u64, u64) {
        (
            self.generation.load(Ordering::Acquire),
            self.operation_id.load(Ordering::Acquire),
        )
    }

    pub(crate) fn take_teardown_audit(
        &self,
        generation: u64,
        operation_id: u64,
    ) -> Option<GenerationTeardownAudit> {
        self.completed_teardown_audit.take(generation, operation_id)
    }

    pub(crate) fn take_latest_teardown_audit(&self) -> Option<GenerationTeardownAudit> {
        self.completed_teardown_audit.take_latest()
    }

    pub(crate) async fn wait_scenario_stage(&self, stage: u8) -> Result<(), SignatureError> {
        let deadline = tokio::time::Instant::now() + DESTROY_TIMEOUT;
        let ticket = loop {
            let ticket = {
                let state = self.state.lock().await;
                match &*state {
                    RuntimeState::Creating { ticket }
                    | RuntimeState::Ready { ticket }
                    | RuntimeState::Poisoned { ticket, .. }
                    | RuntimeState::Destroying { ticket } => Some(Arc::clone(ticket)),
                    RuntimeState::Idle => None,
                    RuntimeState::TerminalPoisoned { reason, .. } => {
                        return Err(SignatureError::TerminalPoisoned(reason.to_string()));
                    }
                }
            };
            if let Some(ticket) = ticket {
                break ticket;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(SignatureError::Timeout);
            }
            tokio::task::yield_now().await;
        };
        tokio::time::timeout(DESTROY_TIMEOUT, ticket.wait_scenario_stage(stage))
            .await
            .map_err(|_| SignatureError::Timeout)
    }

    fn next_operation_id(&self) -> u64 {
        self.operation_id.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    fn teardown_verified_for_exit(&self) -> bool {
        self.teardown_verified_for_exit.load(Ordering::Acquire)
    }

    fn request_shutdown(&self) -> usize {
        self.shutdown_requested.request()
    }

    fn shutdown_requested(&self) -> bool {
        self.shutdown_requested.shutdown_requested()
    }

    async fn verified_exit_snapshot(&self) -> bool {
        let state = self.state.lock().await;
        let runtime_clean = match &*state {
            RuntimeState::Idle => true,
            RuntimeState::TerminalPoisoned { cleanup, .. } => {
                terminal_cleanup_allows_exit(*cleanup)
            }
            _ => false,
        };
        let (policy_tombstones_empty, callbacks_clean) = observed_policy_exit_conditions();
        cleanup_observation_allows_exit(
            runtime_clean,
            self.shutdown_requested(),
            self.generation.load(Ordering::Acquire) == 0 || self.teardown_verified_for_exit(),
            self.is_active(),
            super::signature_host::signature_slot_active(),
            policy_tombstones_empty,
            callbacks_clean,
        )
    }

    async fn wait_shutdown_admissions_drained(&self) {
        self.shutdown_requested.wait_drained().await;
    }
}

fn cleanup_observation_allows_exit(
    runtime_idle: bool,
    shutdown_requested: bool,
    teardown_verified: bool,
    runtime_active: bool,
    signature_slot_active: bool,
    policy_tombstones_empty: bool,
    callbacks_clean: bool,
) -> bool {
    runtime_idle
        && shutdown_requested
        && teardown_verified
        && !runtime_active
        && !signature_slot_active
        && policy_tombstones_empty
        && callbacks_clean
}

fn observed_policy_exit_conditions() -> (bool, bool) {
    #[cfg(target_os = "macos")]
    let policy_tombstones_empty = observed_late_policy_tombstones_empty();
    #[cfg(windows)]
    let policy_tombstones_empty = true;

    let callbacks_clean =
        super::webview_resource_policy::assert_policy_cleanup_callbacks_clean().is_ok();
    (policy_tombstones_empty, callbacks_clean)
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiExecutionState {
    Pending = 0,
    Running = 1,
    ExecutedSafe = 2,
    ExecutedUnsafe = 3,
    Cancelled = 4,
}

struct UiExecutionAck {
    state: AtomicU8,
    notify: tokio::sync::Notify,
}

impl UiExecutionAck {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(UiExecutionState::Pending as u8),
            notify: tokio::sync::Notify::new(),
        }
    }

    fn state(&self) -> UiExecutionState {
        match self.state.load(Ordering::Acquire) {
            value if value == UiExecutionState::Pending as u8 => UiExecutionState::Pending,
            value if value == UiExecutionState::Running as u8 => UiExecutionState::Running,
            value if value == UiExecutionState::ExecutedSafe as u8 => {
                UiExecutionState::ExecutedSafe
            }
            value if value == UiExecutionState::ExecutedUnsafe as u8 => {
                UiExecutionState::ExecutedUnsafe
            }
            value if value == UiExecutionState::Cancelled as u8 => UiExecutionState::Cancelled,
            _ => unreachable!("UI execution ACK state is always a declared discriminant"),
        }
    }

    fn executed(&self) -> bool {
        matches!(
            self.state(),
            UiExecutionState::ExecutedSafe | UiExecutionState::ExecutedUnsafe
        )
    }

    fn execute(&self, safe_to_exit: bool, effect: impl FnOnce()) -> bool {
        if self
            .state
            .compare_exchange(
                UiExecutionState::Pending as u8,
                UiExecutionState::Running as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return false;
        }
        effect();
        let completion = if safe_to_exit {
            UiExecutionState::ExecutedSafe
        } else {
            UiExecutionState::ExecutedUnsafe
        };
        self.state.store(completion as u8, Ordering::Release);
        self.notify.notify_waiters();
        true
    }

    fn cancel(&self) -> bool {
        let cancelled = self
            .state
            .compare_exchange(
                UiExecutionState::Pending as u8,
                UiExecutionState::Cancelled as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok();
        if cancelled {
            self.notify.notify_waiters();
        }
        cancelled
    }

    async fn wait_executed_for(&self, timeout: Duration) -> bool {
        let wait = async {
            loop {
                let notified = self.notify.notified();
                if self.executed() {
                    return true;
                }
                if self.state() == UiExecutionState::Cancelled {
                    return false;
                }
                notified.await;
            }
        };
        match tokio::time::timeout(timeout, wait).await {
            Ok(executed) => executed,
            Err(_) => self.executed(),
        }
    }
}

async fn bounded_ui_dispatch(mut dispatch: impl FnMut(Arc<UiExecutionAck>) -> bool) -> bool {
    const ATTEMPTS: usize = 3;
    const EXECUTION_ACK_TIMEOUT: Duration = Duration::from_millis(25);
    let ack = Arc::new(UiExecutionAck::new());
    for _ in 0..ATTEMPTS {
        let _queued = dispatch(Arc::clone(&ack));
        if ack.wait_executed_for(EXECUTION_ACK_TIMEOUT).await {
            return true;
        }
    }
    let _ = ack.cancel();
    ack.executed()
}

fn handle_final_dispatch_exhaustion(coordinator: &SignatureExitCoordinator) -> bool {
    coordinator.reset();
    coordinator.fail_final_dispatch();
    false
}

fn exit_request_requires_cleanup(
    in_flight: usize,
    runtime_active: bool,
    signature_slot_active: bool,
    teardown_pending: bool,
    readiness: FinalExitReadiness,
) -> bool {
    in_flight > 0
        || runtime_active
        || signature_slot_active
        || teardown_pending
        || !readiness.allows_exit()
}

fn request_shutdown_and_cleanup_required(app: &tauri::AppHandle<tauri::Wry>) -> bool {
    let runtime = app.state::<Arc<SignatureRuntime>>();
    let in_flight = runtime.request_shutdown();
    let runtime_active = runtime.is_active();
    let signature_slot_active = super::signature_host::signature_slot_active();
    let teardown_pending =
        runtime.generation.load(Ordering::Acquire) > 0 && !runtime.teardown_verified_for_exit();
    let readiness = super::signature_host::authoritative_final_exit_readiness_on_ui();
    exit_request_requires_cleanup(
        in_flight,
        runtime_active,
        signature_slot_active,
        teardown_pending,
        readiness,
    )
}

pub fn queue_signature_exit_cleanup(app: &tauri::AppHandle<tauri::Wry>) {
    let coordinator = Arc::clone(app.state::<Arc<SignatureExitCoordinator>>().inner());
    let runtime = Arc::clone(app.state::<Arc<SignatureRuntime>>().inner());
    runtime.request_shutdown();
    if !coordinator.try_begin() {
        return;
    }
    let task_app = app.clone();
    tauri::async_runtime::spawn(async move {
        runtime.wait_shutdown_admissions_drained().await;
        let destroy_result = runtime.destroy().await;
        if let Err(error) = &destroy_result {
            eprintln!("signature cleanup retained operation diagnostic: {error}");
        }
        let safe_to_exit = runtime.verified_exit_snapshot().await;
        let dispatched = bounded_ui_dispatch(|execution_ack| {
            let dispatch_app = task_app.clone();
            let dispatch_coordinator = Arc::clone(&coordinator);
            task_app
                .run_on_main_thread(move || {
                    let readiness =
                        super::signature_host::authoritative_final_exit_readiness_on_ui();
                    let final_safe_to_exit = safe_to_exit && readiness.allows_exit();
                    if !readiness.allows_exit() {
                        eprintln!(
                            "signature cleanup final UI recheck rejected exit: {readiness:?}"
                        );
                    }
                    execution_ack.execute(final_safe_to_exit, || {
                        let exit_coordinator = Arc::clone(&dispatch_coordinator);
                        complete_authoritative_ui_exit(
                            &dispatch_coordinator,
                            safe_to_exit,
                            readiness,
                            move || {
                                if !exit_coordinator.complete_lifecycle_probe_cleanup() {
                                    dispatch_app.exit(0);
                                }
                            },
                        );
                    });
                })
                .is_ok()
        })
        .await;
        if !dispatched {
            handle_final_dispatch_exhaustion(&coordinator);
            eprintln!("signature cleanup could not dispatch its final UI decision");
        }
    });
}

pub fn handle_main_window_event(window: &tauri::Window<tauri::Wry>, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if request_shutdown_and_cleanup_required(window.app_handle()) {
                api.prevent_close();
                queue_signature_exit_cleanup(window.app_handle());
            }
        }
        tauri::WindowEvent::Destroyed
            if request_shutdown_and_cleanup_required(window.app_handle()) =>
        {
            queue_signature_exit_cleanup(window.app_handle());
        }
        _ => {}
    }
}

pub fn handle_exit_requested(app: &tauri::AppHandle<tauri::Wry>, api: &tauri::ExitRequestApi) {
    if request_shutdown_and_cleanup_required(app) {
        api.prevent_exit();
        queue_signature_exit_cleanup(app);
    }
}

pub fn final_exit_cleanup() {
    super::signature_host::final_exit_drop();
}

fn readiness_script() -> &'static str {
    r#"(() => {
  try {
    return { status: 'ok', ready: typeof globalThis.crc32 === 'function' };
  } catch (_) {
    return { status: 'error' };
  }
})()"#
}

pub struct NavigationGate {
    bootstrap_pending: AtomicBool,
    allowed_origin: Url,
}

impl NavigationGate {
    pub fn new() -> Self {
        Self {
            bootstrap_pending: AtomicBool::new(true),
            allowed_origin: Url::parse(GD_PAGE_URL).expect("fixed GD origin is valid"),
        }
    }

    pub(crate) fn for_origin(allowed_origin: Url) -> Self {
        Self {
            bootstrap_pending: AtomicBool::new(true),
            allowed_origin,
        }
    }

    pub fn allows(&self, url: &Url) -> bool {
        if url.as_str() == "about:blank" {
            return self.bootstrap_pending.swap(false, Ordering::SeqCst);
        }
        self.bootstrap_pending.store(false, Ordering::SeqCst);
        super::webview_resource_policy::is_allowed_network_request_for(&self.allowed_origin, url)
    }
}

impl Default for NavigationGate {
    fn default() -> Self {
        Self::new()
    }
}

pub fn is_allowed_gd_navigation(url: &Url) -> bool {
    let authority_has_userinfo = url
        .as_str()
        .split_once("://")
        .and_then(|(_, remainder)| remainder.split(['/', '?', '#']).next())
        .is_some_and(|authority| authority.contains('@'));

    url.scheme() == "https"
        && url.host_str() == Some("music.gdstudio.xyz")
        && url.port_or_known_default() == Some(443)
        && !authority_has_userinfo
        && url.username().is_empty()
        && url.password().is_none()
}

pub fn validate_signature_result(value: &str) -> Result<SignatureValue, SignatureError> {
    SignatureValue::try_from(value).map_err(|_| SignatureError::InvalidReturn("rust-validation"))
}

#[cfg(test)]
type EvalCallback = Box<dyn Fn(String) + Send + 'static>;

#[cfg(test)]
async fn eval_json_with<T, F, E>(
    script: String,
    timeout: Duration,
    evaluate: F,
) -> Result<T, SignatureError>
where
    T: DeserializeOwned,
    F: FnOnce(String, EvalCallback) -> Result<(), E>,
{
    let (sender, receiver) = oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let callback_sender = Arc::clone(&sender);
    let callback = Box::new(move |value| {
        let sender = callback_sender
            .lock()
            .expect("signature callback sender mutex poisoned")
            .take();
        if let Some(sender) = sender {
            let _ = sender.send(value);
        }
    });

    evaluate(script, callback).map_err(|_| SignatureError::Evaluation)?;
    let raw = tokio::time::timeout(timeout, receiver)
        .await
        .map_err(|_| SignatureError::Timeout)?
        .map_err(|_| SignatureError::Evaluation)?;
    serde_json::from_str(&raw).map_err(|_| SignatureError::Evaluation)
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum SignatureEvaluation {
    Ok { value: String },
    Error { code: String },
}

fn parse_signature_text(raw: &str) -> Result<String, SignatureError> {
    let evaluation: SignatureEvaluation =
        serde_json::from_str(raw).map_err(|_| SignatureError::Evaluation)?;
    match evaluation {
        SignatureEvaluation::Ok { value } => {
            validate_signature_result(&value)?;
            Ok(value)
        }
        SignatureEvaluation::Error { code } if code == "MISSING_FUNCTION" => {
            Err(SignatureError::MissingFunction)
        }
        SignatureEvaluation::Error { code }
            if matches!(
                code.as_str(),
                "INVALID_TYPE" | "EMPTY_VALUE" | "RETURN_TOO_LARGE"
            ) =>
        {
            Err(SignatureError::InvalidReturn("javascript-validation"))
        }
        SignatureEvaluation::Error { .. } => Err(SignatureError::Evaluation),
    }
}

#[cfg(test)]
fn parse_signature_evaluation(raw: &str) -> Result<SignatureValue, SignatureError> {
    let value = parse_signature_text(raw)?;
    validate_signature_result(&value)
}

fn signature_script(input: &EncodedComponent) -> String {
    const SCRIPT: &str = r#"(() => {
  try {
    const fn = globalThis.crc32;
    if (typeof fn !== 'function') return { status: 'error', code: 'MISSING_FUNCTION' };
    const value = fn(ENCODED_INPUT_JSON);
    if (typeof value !== 'string') return { status: 'error', code: 'INVALID_TYPE' };
    if (value.length === 0) return { status: 'error', code: 'EMPTY_VALUE' };
    if (new TextEncoder().encode(value).byteLength > 128) return { status: 'error', code: 'RETURN_TOO_LARGE' };
    return { status: 'ok', value };
  } catch (_) {
    return { status: 'error', code: 'CALL_THROWN' };
  }
})()"#;
    let encoded = serde_json::to_string(input.as_str())
        .expect("serializing an encoded component as JSON cannot fail");
    SCRIPT.replacen("ENCODED_INPUT_JSON", &encoded, 1)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use super::super::{
        signature_host::{CreationTicket, FinalExitReadiness},
        webview_resource_policy::IsolationCounters,
    };
    use super::{
        CALL_TIMEOUT, CompletedTeardownAuditSlot, DESTROY_TIMEOUT, GD_PAGE_URL, INIT_TIMEOUT,
        InitDeadline, MAX_SIGNATURE_BYTES, NavigationGate, RuntimeState,
        SIGNATURE_HOST_WINDOW_LABEL, SIGNATURE_WEBVIEW_ID, ShutdownAdmissionGate, SignatureError,
        SignatureExitCoordinator, SignatureInitReport, SignatureRuntime, TerminalCleanup,
        TerminalReauditObservation, UiExecutionAck, bounded_ui_dispatch,
        cleanup_observation_allows_exit, complete_authoritative_ui_exit, eval_json_with,
        exit_request_requires_cleanup, handle_final_dispatch_exhaustion, is_allowed_gd_navigation,
        maybe_complete_teardown, parse_signature_evaluation, signature_script,
        terminal_cleanup_after_reaudit, terminal_cleanup_allows_exit, validate_signature_result,
    };
    use crate::music::contract::EncodedComponent;
    use serde_json::{Value, json};
    use url::Url;

    #[test]
    fn navigation_allows_only_the_exact_credential_free_https_origin() {
        for allowed in [
            "https://music.gdstudio.xyz/",
            "https://music.gdstudio.xyz/js/player.js?v=20260616",
            "https://music.gdstudio.xyz:443/api.php",
        ] {
            assert!(
                is_allowed_gd_navigation(&Url::parse(allowed).unwrap()),
                "expected allow: {allowed}"
            );
        }

        for denied in [
            "http://music.gdstudio.xyz/",
            "https://evil.example/",
            "https://music.gdstudio.xyz.evil.example/",
            "https://user:pass@music.gdstudio.xyz/",
            "https://music.gdstudio.xyz:444/",
        ] {
            assert!(
                !is_allowed_gd_navigation(&Url::parse(denied).unwrap()),
                "expected deny: {denied}"
            );
        }
    }

    #[test]
    fn signature_validation_enforces_the_rust_side_byte_and_delimiter_bounds() {
        for valid in ["a", "签名", &"x".repeat(128)] {
            assert!(validate_signature_result(valid).is_ok(), "expected valid");
        }

        for invalid in [
            "".to_string(),
            "x\n".to_string(),
            "left&right".to_string(),
            "left=right".to_string(),
            "x".repeat(129),
            "签".repeat(43),
        ] {
            assert!(
                validate_signature_result(&invalid).is_err(),
                "expected invalid"
            );
        }
    }

    #[test]
    fn signature_webview_bootstrap_navigation_is_single_use_and_fail_closed() {
        let gate = NavigationGate::new();
        assert!(gate.allows(&Url::parse("about:blank").unwrap()));
        assert!(!gate.allows(&Url::parse("about:blank").unwrap()));
        assert!(gate.allows(&Url::parse("https://music.gdstudio.xyz/").unwrap()));

        let skipped_bootstrap = NavigationGate::new();
        assert!(skipped_bootstrap.allows(&Url::parse("https://music.gdstudio.xyz/").unwrap()));
        assert!(!skipped_bootstrap.allows(&Url::parse("about:blank").unwrap()));

        let rejected_first = NavigationGate::new();
        assert!(!rejected_first.allows(&Url::parse("https://evil.example/").unwrap()));
        assert!(rejected_first.allows(&Url::parse("https://music.gdstudio.xyz/").unwrap()));
    }

    #[tokio::test]
    async fn signature_webview_eval_json_is_callback_bounded() {
        let value: Value = eval_json_with(
            "fixed".to_string(),
            Duration::from_millis(100),
            |script, callback| {
                assert_eq!(script, "fixed");
                callback(json!({ "ready": true }).to_string());
                Ok::<_, ()>(())
            },
        )
        .await
        .unwrap();
        assert_eq!(value, json!({ "ready": true }));

        let scheduling_error = eval_json_with::<Value, _, _>(
            "fixed".to_string(),
            Duration::from_millis(100),
            |_, _| Err("dispatch failed"),
        )
        .await;
        assert!(matches!(scheduling_error, Err(SignatureError::Evaluation)));

        let timeout = eval_json_with::<Value, _, ()>(
            "fixed".to_string(),
            Duration::from_millis(1),
            |_, _| Ok(()),
        )
        .await;
        assert!(matches!(timeout, Err(SignatureError::Timeout)));
    }

    #[tokio::test]
    async fn signature_webview_ignores_a_callback_arriving_after_timeout() {
        let callback_ran = Arc::new(AtomicBool::new(false));
        let callback_ran_in_thread = Arc::clone(&callback_ran);

        let result = eval_json_with::<Value, _, ()>(
            "fixed".to_string(),
            Duration::from_millis(1),
            move |_, callback| {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(20));
                    callback_ran_in_thread.store(true, Ordering::SeqCst);
                    callback("null".to_string());
                });
                Ok(())
            },
        )
        .await;

        assert!(matches!(result, Err(SignatureError::Timeout)));
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(callback_ran.load(Ordering::SeqCst));
    }

    #[test]
    fn signature_webview_maps_self_catching_evaluation_results_without_raw_data() {
        parse_signature_evaluation(r#"{"status":"ok","value":"abc123"}"#)
            .expect("valid signature envelope");

        assert!(matches!(
            parse_signature_evaluation(r#"{"status":"error","code":"MISSING_FUNCTION"}"#),
            Err(SignatureError::MissingFunction)
        ));

        for code in ["INVALID_TYPE", "EMPTY_VALUE", "RETURN_TOO_LARGE"] {
            let raw = format!(r#"{{"status":"error","code":"{code}"}}"#);
            assert!(matches!(
                parse_signature_evaluation(&raw),
                Err(SignatureError::InvalidReturn(_))
            ));
        }

        for raw in [
            r#"{"status":"error","code":"CALL_THROWN"}"#,
            r#"{"status":"error","code":"UNKNOWN"}"#,
            "not-json",
        ] {
            let error = parse_signature_evaluation(raw).unwrap_err();
            assert!(matches!(error, SignatureError::Evaluation));
            assert!(!error.to_string().contains(raw));
        }
    }

    #[test]
    fn signature_webview_script_uses_only_the_official_function_and_encoded_input() {
        let input = EncodedComponent::encode("A B!'()*/?=%");
        let script = signature_script(&input);
        assert!(script.contains("globalThis.crc32"));
        assert!(script.contains(r#""A%20B%21%27%28%29%2A%2F%3F%3D%25""#));
        for code in [
            "MISSING_FUNCTION",
            "INVALID_TYPE",
            "EMPTY_VALUE",
            "RETURN_TOO_LARGE",
            "CALL_THROWN",
        ] {
            assert!(script.contains(code));
        }
        assert!(!script.contains("function crc32"));
    }

    #[test]
    fn signature_webview_runtime_bounds_and_labels_are_fixed() {
        assert_eq!(GD_PAGE_URL, "https://music.gdstudio.xyz/");
        assert_eq!(SIGNATURE_HOST_WINDOW_LABEL, "gd-signature-host-feasibility");
        assert_eq!(SIGNATURE_WEBVIEW_ID, "gd-signature-raw-wry");
        assert_eq!(INIT_TIMEOUT, Duration::from_secs(20));
        assert_eq!(CALL_TIMEOUT, Duration::from_secs(5));
        assert_eq!(DESTROY_TIMEOUT, Duration::from_secs(5));
        assert_eq!(MAX_SIGNATURE_BYTES, 128);
    }

    #[test]
    fn signature_webview_initialization_uses_one_shared_twenty_second_deadline() {
        let start = Instant::now();
        let deadline = InitDeadline::from_start(start);
        assert_eq!(deadline.remaining_at(start), Some(Duration::from_secs(20)));
        assert_eq!(
            deadline.remaining_at(start + Duration::from_secs(7)),
            Some(Duration::from_secs(13))
        );
        assert_eq!(
            deadline.remaining_at(start + Duration::from_millis(19_999)),
            Some(Duration::from_millis(1))
        );
        assert_eq!(deadline.remaining_at(start + Duration::from_secs(20)), None);
    }

    #[test]
    fn signature_webview_runtime_facade_is_send_sync_and_init_report_is_closed() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SignatureRuntime>();

        let report = SignatureInitReport {
            generation: 2,
            operation_id: 4,
            host_label: "gd-signature-host-feasibility-2-4".into(),
            webview_id: "gd-signature-raw-wry".into(),
            current_url: "https://music.gdstudio.xyz/".into(),
            runtime_mode: "native-host-raw-wry-0.55.1".into(),
            webview_runtime_version: "111.0.1661.62".into(),
            resource_policy_mode: "webview2-22-all-source-kinds".into(),
            strong_source_kinds_interface_available: true,
            official_finished_before_polling: true,
            policy_installed_before_first_network_navigation: true,
        };
        assert_eq!(
            serde_json::to_value(report).unwrap(),
            json!({
                "generation": 2,
                "operationId": 4,
                "hostLabel": "gd-signature-host-feasibility-2-4",
                "webviewId": "gd-signature-raw-wry",
                "currentUrl": "https://music.gdstudio.xyz/",
                "runtimeMode": "native-host-raw-wry-0.55.1",
                "webviewRuntimeVersion": "111.0.1661.62",
                "resourcePolicyMode": "webview2-22-all-source-kinds",
                "strongSourceKindsInterfaceAvailable": true,
                "officialFinishedBeforePolling": true,
                "policyInstalledBeforeFirstNetworkNavigation": true
            })
        );
    }

    #[test]
    fn signature_webview_isolation_counters_snapshot_every_blocking_boundary() {
        let counters = IsolationCounters::default();
        counters.blocked_navigation();
        counters.blocked_new_window();
        counters.blocked_download();
        counters.blocked_resource_request(false);
        counters.blocked_resource_request(true);
        counters.policy_fault();

        let snapshot = counters.snapshot();
        assert_eq!(snapshot.blocked_navigations, 1);
        assert_eq!(snapshot.blocked_new_windows, 1);
        assert_eq!(snapshot.blocked_downloads, 1);
        assert_eq!(snapshot.blocked_resource_requests, 2);
        assert_eq!(snapshot.resource_canary_hits, 1);
        assert_eq!(snapshot.policy_faults, 1);
    }

    #[test]
    fn signature_webview_exit_coordinator_is_nonblocking_and_completes_once() {
        let coordinator = SignatureExitCoordinator::new();
        assert!(coordinator.try_begin());
        assert!(!coordinator.try_begin());

        let exits = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let failed_exits = Arc::clone(&exits);
        assert!(!maybe_complete_teardown(&coordinator, false, move || {
            failed_exits.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(exits.load(Ordering::SeqCst), 0);
        assert!(coordinator.try_begin());

        let successful_exits = Arc::clone(&exits);
        assert!(maybe_complete_teardown(&coordinator, true, move || {
            successful_exits.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(exits.load(Ordering::SeqCst), 1);
        assert!(coordinator.try_begin());
    }

    #[test]
    fn signature_webview_poisoned_business_error_does_not_block_verified_cleanup_exit() {
        let original_operation = Err::<(), _>(SignatureError::Evaluation);
        assert!(original_operation.is_err());
        assert!(cleanup_observation_allows_exit(
            true, true, true, false, false, true, true
        ));
        assert!(!cleanup_observation_allows_exit(
            false, true, true, false, false, true, true
        ));
        assert!(!cleanup_observation_allows_exit(
            true, false, true, false, false, true, true
        ));
        assert!(!cleanup_observation_allows_exit(
            true, true, false, false, false, true, true
        ));
        assert!(!cleanup_observation_allows_exit(
            true, true, true, true, false, true, true
        ));
        assert!(!cleanup_observation_allows_exit(
            true, true, true, false, true, true, true
        ));
    }

    #[test]
    fn signature_webview_exit_admission_rejects_macos_tombstone_residue() {
        let readiness = FinalExitReadiness::new(true, true, false, true);
        assert!(!cleanup_observation_allows_exit(
            true, true, true, false, false, false, true
        ));
        assert!(exit_request_requires_cleanup(
            0, false, false, false, readiness
        ));
    }

    #[test]
    fn signature_webview_exit_admission_rejects_macos_callback_sticky_fault() {
        let readiness = FinalExitReadiness::new(true, true, true, false);
        assert!(!cleanup_observation_allows_exit(
            true, true, true, false, false, true, false
        ));
        assert!(exit_request_requires_cleanup(
            0, false, false, false, readiness
        ));
    }

    #[test]
    fn signature_webview_final_ui_recheck_rejects_policy_fault_after_safe_snapshot() {
        for readiness in [
            FinalExitReadiness::new(true, true, false, true),
            FinalExitReadiness::new(true, true, true, false),
        ] {
            let coordinator = SignatureExitCoordinator::new();
            assert!(coordinator.try_begin());
            let exits = std::cell::Cell::new(0);
            assert!(!complete_authoritative_ui_exit(
                &coordinator,
                true,
                readiness,
                || exits.set(exits.get() + 1),
            ));
            assert_eq!(exits.get(), 0);
            assert!(coordinator.final_dispatch_failed());
            assert!(coordinator.try_begin());
        }
    }

    #[test]
    fn terminal_timeout_retains_ticket_and_late_valid_cleanup_becomes_exit_only_but_never_reusable()
    {
        let ticket = CreationTicket::new(41, 73);
        let mut state = RuntimeState::TerminalPoisoned {
            ticket: Arc::clone(&ticket),
            reason: SignatureError::DestroyTimeout,
            cleanup: TerminalCleanup::AwaitingLate,
        };
        let RuntimeState::TerminalPoisoned {
            ticket: retained,
            cleanup,
            ..
        } = &mut state
        else {
            unreachable!();
        };
        assert!(Arc::ptr_eq(retained, &ticket));
        *cleanup =
            terminal_cleanup_after_reaudit(*cleanup, TerminalReauditObservation::StillPending);
        assert_eq!(*cleanup, TerminalCleanup::AwaitingLate);
        *cleanup =
            terminal_cleanup_after_reaudit(*cleanup, TerminalReauditObservation::CompletedValid);
        assert_eq!(*cleanup, TerminalCleanup::ExitOnlyVerified);
        assert!(terminal_cleanup_allows_exit(*cleanup));
        assert!(!matches!(state, RuntimeState::Idle));
    }

    #[test]
    fn terminal_invalid_audit_never_upgrades_to_exit_only() {
        let cleanup = terminal_cleanup_after_reaudit(
            TerminalCleanup::InvalidAudit,
            TerminalReauditObservation::CompletedValid,
        );
        assert_eq!(cleanup, TerminalCleanup::InvalidAudit);
        assert!(!terminal_cleanup_allows_exit(cleanup));

        let invalid = terminal_cleanup_after_reaudit(
            TerminalCleanup::AwaitingLate,
            TerminalReauditObservation::CompletedInvalid,
        );
        assert_eq!(invalid, TerminalCleanup::InvalidAudit);
    }

    #[test]
    fn signature_webview_shutdown_admission_closes_all_initialize_interleavings() {
        let close_first = ShutdownAdmissionGate::default();
        assert_eq!(close_first.request(), 0);
        assert!(close_first.begin().is_none());

        let between_checks = ShutdownAdmissionGate::default();
        let admission = between_checks.begin().unwrap();
        assert_eq!(between_checks.request(), 1);
        assert!(!admission.commit_allowed());
        drop(admission);
        assert_eq!(between_checks.in_flight(), 0);

        let published = ShutdownAdmissionGate::default();
        let admission = published.begin().unwrap();
        assert!(admission.commit_allowed());
        drop(admission);
        assert_eq!(published.in_flight(), 0);
        assert_eq!(published.request(), 0);

        let idle_never_used = ShutdownAdmissionGate::default();
        assert_eq!(idle_never_used.request(), 0);
        assert!(cleanup_observation_allows_exit(
            true, true, true, false, false, true, true
        ));
    }

    #[tokio::test]
    async fn signature_webview_shutdown_waits_for_inflight_admission_to_drain() {
        let gate = Arc::new(ShutdownAdmissionGate::default());
        let admission = gate.begin().unwrap();
        assert_eq!(gate.request(), 1);
        let waiting_gate = Arc::clone(&gate);
        let waiter = tokio::spawn(async move {
            waiting_gate.wait_drained().await;
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(admission);
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn signature_webview_final_ui_dispatch_retries_and_recovers_coordinator() {
        let attempts = std::cell::Cell::new(0);
        assert!(
            bounded_ui_dispatch(|ack| {
                let next = attempts.get() + 1;
                attempts.set(next);
                if next == 3 {
                    assert!(ack.execute(true, || {}));
                }
                true
            })
            .await
        );
        assert_eq!(attempts.get(), 3);

        let failed_attempts = std::cell::Cell::new(0);
        assert!(
            !bounded_ui_dispatch(|_ack| {
                failed_attempts.set(failed_attempts.get() + 1);
                false
            })
            .await
        );
        assert_eq!(failed_attempts.get(), 3);

        let coordinator = SignatureExitCoordinator::new();
        assert!(coordinator.arm_lifecycle_probe());
        assert!(!handle_final_dispatch_exhaustion(&coordinator));
        assert!(coordinator.final_dispatch_failed());
        assert!(coordinator.wait_lifecycle_probe_cleanup().await.is_err());
        assert!(coordinator.try_begin());

        let ordinary = SignatureExitCoordinator::new();
        assert!(!handle_final_dispatch_exhaustion(&ordinary));
        assert!(ordinary.final_dispatch_failed());

        let unsafe_exit = SignatureExitCoordinator::new();
        assert!(!handle_final_dispatch_exhaustion(&unsafe_exit));
        assert!(unsafe_exit.final_dispatch_failed());
    }

    #[tokio::test]
    async fn signature_webview_enqueue_success_without_execution_is_cancelled_fail_closed() {
        let queued = std::cell::RefCell::new(Vec::<Arc<UiExecutionAck>>::new());
        assert!(
            !bounded_ui_dispatch(|ack| {
                queued.borrow_mut().push(ack);
                true
            })
            .await
        );
        assert_eq!(queued.borrow().len(), 3);

        let late_executions = std::cell::Cell::new(0);
        for ack in queued.borrow_mut().drain(..) {
            assert!(!ack.execute(true, || { late_executions.set(late_executions.get() + 1) }));
        }
        assert_eq!(late_executions.get(), 0);
    }

    #[tokio::test]
    async fn signature_webview_duplicate_queued_ui_closures_execute_effect_exactly_once() {
        let queued = std::cell::RefCell::new(Vec::<Arc<UiExecutionAck>>::new());
        let executions = std::cell::Cell::new(0);
        assert!(
            bounded_ui_dispatch(|ack| {
                queued.borrow_mut().push(Arc::clone(&ack));
                assert!(ack.execute(false, || executions.set(executions.get() + 1)));
                true
            })
            .await
        );
        for ack in queued.borrow_mut().drain(..) {
            assert!(!ack.execute(true, || executions.set(executions.get() + 1)));
        }
        assert_eq!(executions.get(), 1);
    }

    #[test]
    fn signature_webview_completed_teardown_audit_slot_stays_constant_across_cycles() {
        let slot = CompletedTeardownAuditSlot::default();
        for generation in 1..=25 {
            let ticket = CreationTicket::new(generation, generation * 10);
            ticket.mark_native_destroyed();
            ticket.mark_manager_absent();
            ticket.mark_policy_cleanup();
            ticket.mark_tombstones_empty();
            slot.record(ticket.teardown_audit());
            assert_eq!(slot.len(), 1);
        }
        let audit = slot.take_latest().unwrap();
        assert_eq!((audit.generation, audit.operation_id), (25, 250));
        assert_eq!(slot.len(), 0);
    }

    #[tokio::test]
    async fn signature_webview_lifecycle_autorun_owns_the_final_exit_boundary() {
        let coordinator = SignatureExitCoordinator::new();
        assert!(coordinator.arm_lifecycle_probe());
        assert!(!coordinator.arm_lifecycle_probe());
        assert!(coordinator.complete_lifecycle_probe_cleanup());
        coordinator.wait_lifecycle_probe_cleanup().await.unwrap();
        assert!(!coordinator.complete_lifecycle_probe_cleanup());
    }
}
