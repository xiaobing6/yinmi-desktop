pub mod gd_live;
pub mod signature_host;
pub mod signature_probe;
pub mod signature_webview;
pub mod webview_resource_policy;

use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;

use self::{
    gd_live::{GdProbeError, ProtocolProbeCase, ProtocolProbeReport, run_gd_probe},
    signature_probe::{IsolationReport, increment_ipc_canary},
    signature_webview::{SignatureError, SignatureInitReport, SignatureRuntime},
};
use crate::music::contract::EncodedComponent;

pub const FEASIBILITY_COMMANDS: [&str; 6] = [
    "feasibility_signature_initialize",
    "feasibility_signature_sign",
    "feasibility_signature_destroy",
    "feasibility_signature_isolation",
    "feasibility_run_gd_probe",
    "feasibility_ipc_canary",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeasibilityCommandError {
    code: &'static str,
    message: String,
}

impl From<SignatureError> for FeasibilityCommandError {
    fn from(error: SignatureError) -> Self {
        Self {
            code: "signature-runtime-failed",
            message: error.to_string(),
        }
    }
}

impl From<GdProbeError> for FeasibilityCommandError {
    fn from(error: GdProbeError) -> Self {
        Self {
            code: "gd-probe-failed",
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcCanaryReport {
    count: u64,
}

#[tauri::command]
pub async fn feasibility_signature_initialize(
    runtime: State<'_, Arc<SignatureRuntime>>,
) -> Result<SignatureInitReport, FeasibilityCommandError> {
    runtime.initialize().await.map_err(Into::into)
}

#[tauri::command]
pub async fn feasibility_signature_sign(
    runtime: State<'_, Arc<SignatureRuntime>>,
    input: String,
) -> Result<String, FeasibilityCommandError> {
    runtime
        .sign_text(&EncodedComponent::encode(&input))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn feasibility_signature_destroy(
    runtime: State<'_, Arc<SignatureRuntime>>,
) -> Result<(), FeasibilityCommandError> {
    runtime.destroy().await.map_err(Into::into)
}

#[tauri::command]
pub async fn feasibility_signature_isolation(
    runtime: State<'_, Arc<SignatureRuntime>>,
) -> Result<IsolationReport, FeasibilityCommandError> {
    runtime.run_isolation_probe().await.map_err(Into::into)
}

#[tauri::command]
pub async fn feasibility_run_gd_probe(
    runtime: State<'_, Arc<SignatureRuntime>>,
    probe_case: ProtocolProbeCase,
) -> Result<ProtocolProbeReport, FeasibilityCommandError> {
    run_gd_probe(&runtime, probe_case, &CancellationToken::new())
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn feasibility_ipc_canary() -> IpcCanaryReport {
    IpcCanaryReport {
        count: increment_ipc_canary(),
    }
}

#[cfg(test)]
mod tests {
    use super::FEASIBILITY_COMMANDS;

    #[test]
    fn signature_webview_feature_command_manifest_is_exact() {
        assert_eq!(
            FEASIBILITY_COMMANDS,
            [
                "feasibility_signature_initialize",
                "feasibility_signature_sign",
                "feasibility_signature_destroy",
                "feasibility_signature_isolation",
                "feasibility_run_gd_probe",
                "feasibility_ipc_canary",
            ]
        );
    }
}
