#[cfg(feature = "feasibility")]
const FEASIBILITY_COMMANDS: &[&str] = &[
    "feasibility_signature_initialize",
    "feasibility_signature_sign",
    "feasibility_signature_destroy",
    "feasibility_signature_isolation",
    "feasibility_run_gd_probe",
    "feasibility_ipc_canary",
    "music_search",
    "music_get_search_snapshot",
    "music_download_batch",
    "music_get_download_snapshot",
    "music_scan_existing",
    "music_retry_failed",
    "music_get_default_directory",
    "music_cancel_current_download",
    "music_cancel_all_downloads",
    "music_open_download_directory",
    "app_initialize",
    "app_open_log_directory",
    "app_cancel_exit",
    "app_set_update_active",
    "app_get_activity_status",
    "app_confirm_exit",
    "app_prepare_restart",
];

fn main() {
    let attributes = tauri_build::Attributes::new();
    #[cfg(feature = "feasibility")]
    let attributes = attributes.capabilities_path_pattern("capabilities/*main.json");
    #[cfg(not(feature = "feasibility"))]
    let attributes = attributes.capabilities_path_pattern("capabilities/main.json");
    #[cfg(feature = "feasibility")]
    let attributes = attributes.app_manifest(
        tauri_build::AppManifest::new()
            .commands(FEASIBILITY_COMMANDS)
            .permissions_path_pattern("permissions/*.toml"),
    );
    #[cfg(not(feature = "feasibility"))]
    let attributes = attributes.app_manifest(
        tauri_build::AppManifest::new().permissions_path_pattern("permissions/default/**/*"),
    );
    tauri_build::try_build(attributes).expect("failed to build Tauri configuration");
}
