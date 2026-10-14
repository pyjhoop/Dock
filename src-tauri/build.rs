fn main() {
    let commands = &[
        "get_state", "add_account", "update_account", "remove_account", "reorder_accounts",
        "set_settings", "show_view", "set_rect", "set_overlay", "reload_view", "view_nav",
        "open_compose", "set_passcode", "clear_passcode", "unlock", "lock_app", "list_downloads",
        "download_action", "clear_downloads", "report_event", "quit_app", "finish_login",
        "open_external", "test_notification", "check_update", "install_update_cmd",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("tauri build failed");
}
