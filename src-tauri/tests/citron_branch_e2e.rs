mod common;

use common::{simple_progress_printer, TestConfigHelper};
use ns_emu_tools_lib::commands::yuzu::get_all_yuzu_versions;
use ns_emu_tools_lib::config::{get_config, Config, CONFIG};
#[cfg(target_os = "linux")]
use ns_emu_tools_lib::error::AppError;
use ns_emu_tools_lib::models::yuzu_branch::{CITRON_NIGHTLY_BRANCH, CITRON_STABLE_BRANCH};
use ns_emu_tools_lib::repositories::yuzu::{
    get_yuzu_release_info_by_version, yuzu_release_api_for_branch,
};
use ns_emu_tools_lib::services::yuzu::{install_yuzu, select_current_platform_yuzu_asset};
use once_cell::sync::Lazy;
use std::path::Path;
use tokio::sync::Mutex;
use tracing::info;

static CITRON_INSTALL_E2E_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn is_short_sha(value: &str) -> bool {
    value.len() == 7 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[ctor::ctor]
fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

struct ConfigRestoreGuard {
    original: Config,
}

impl ConfigRestoreGuard {
    fn capture() -> Self {
        Self {
            original: get_config(),
        }
    }
}

impl Drop for ConfigRestoreGuard {
    fn drop(&mut self) {
        let mut config = CONFIG.write();
        *config = self.original.clone();
        if let Err(error) = config.save() {
            eprintln!("failed to restore config after Citron E2E: {error}");
        }
    }
}

async fn assert_citron_backend_e2e(branch: &str, expected_api_fragment: &str) {
    assert!(
        yuzu_release_api_for_branch(branch)
            .unwrap()
            .contains(expected_api_fragment),
        "{branch} should use {expected_api_fragment}"
    );

    let versions_response = get_all_yuzu_versions(branch.to_string()).await.unwrap();
    let versions = versions_response.data.unwrap();
    assert!(
        !versions.is_empty(),
        "{branch} version list should not be empty"
    );
    if branch == CITRON_NIGHTLY_BRANCH {
        assert!(
            is_short_sha(&versions[0]),
            "{branch} latest version should be the upstream short SHA, got {}",
            versions[0]
        );
    }

    let release = get_yuzu_release_info_by_version(&versions[0], branch)
        .await
        .unwrap();
    assert_eq!(release.tag_name, versions[0]);
    assert!(
        !release.assets.is_empty(),
        "{branch} release should include downloadable assets"
    );

    let download_url = select_current_platform_yuzu_asset(&release, branch);

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    assert!(
        download_url.is_some(),
        "{branch} should have a current-platform asset"
    );

    #[cfg(target_os = "linux")]
    assert!(
        download_url.is_none(),
        "Citron Linux installs are intentionally blocked before download"
    );
}

fn apply_isolated_citron_config(test_helper: &TestConfigHelper) {
    let mut config = CONFIG.write();
    config.yuzu.yuzu_path = test_helper.emulator_path();
    config.yuzu.yuzu_version = None;
    config.yuzu.yuzu_firmware = None;
    config.yuzu.branch = CITRON_STABLE_BRANCH.to_string();
    config.setting.download.auto_delete_after_install = true;
    config.setting.download.backend = "rust".to_string();
    config.setting.other.rename_yuzu_to_cemu = false;
}

fn assert_citron_installed_at(yuzu_path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let citron_exe = yuzu_path.join("citron.exe");
        assert!(
            citron_exe.is_file(),
            "Citron executable should exist at {}",
            citron_exe.display()
        );
    }

    #[cfg(target_os = "macos")]
    {
        let citron_app = yuzu_path.join("Citron.app");
        let citron_exe = citron_app.join("Contents").join("MacOS").join("Citron");
        assert!(
            citron_app.is_dir(),
            "Citron app bundle should exist at {}",
            citron_app.display()
        );
        assert!(
            citron_exe.is_file(),
            "Citron bundle executable should exist at {}",
            citron_exe.display()
        );
    }

    #[cfg(target_os = "linux")]
    {
        let _ = yuzu_path;
    }
}

async fn latest_citron_version(branch: &str) -> String {
    let versions_response = get_all_yuzu_versions(branch.to_string())
        .await
        .unwrap_or_else(|error| panic!("{branch} version list request failed: {error}"));
    let versions = versions_response
        .data
        .unwrap_or_else(|| panic!("{branch} version list response should include data"));
    assert!(
        !versions.is_empty(),
        "{branch} version list should not be empty"
    );
    if branch == CITRON_NIGHTLY_BRANCH {
        assert!(
            is_short_sha(&versions[0]),
            "{branch} latest version should be the upstream short SHA, got {}",
            versions[0]
        );
    }
    versions[0].clone()
}

async fn assert_citron_install_backend_e2e(branch: &str, expected_api_fragment: &str) {
    let _install_guard = CITRON_INSTALL_E2E_LOCK.lock().await;
    let _config_guard = ConfigRestoreGuard::capture();

    assert!(
        yuzu_release_api_for_branch(branch)
            .unwrap()
            .contains(expected_api_fragment),
        "{branch} should use {expected_api_fragment}"
    );

    let test_helper = TestConfigHelper::new();
    apply_isolated_citron_config(&test_helper);
    info!(
        "Citron {} install E2E temp dir: {}",
        branch,
        test_helper.temp_dir.path().display()
    );

    #[cfg(target_os = "linux")]
    {
        let error = install_yuzu("stable", branch, simple_progress_printer(branch))
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::Unsupported(_)));
        assert!(error.to_string().contains("AppImage"));
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let target_version = latest_citron_version(branch).await;
        info!("Installing Citron {} version {}", branch, target_version);

        install_yuzu(
            &target_version,
            branch,
            simple_progress_printer(&format!("Citron {branch} install")),
        )
        .await
        .unwrap_or_else(|error| panic!("installing {branch} {target_version} failed: {error}"));

        let config = get_config();
        assert_eq!(
            config.yuzu.yuzu_version.as_deref(),
            Some(target_version.as_str()),
            "{branch} should save the installed version"
        );
        assert_eq!(
            config.yuzu.branch, branch,
            "{branch} should save the installed branch"
        );
        assert_citron_installed_at(&config.yuzu.yuzu_path);
    }
}

#[tokio::test]
#[ignore]
async fn citron_stable_backend_e2e() {
    assert_citron_backend_e2e(CITRON_STABLE_BRANCH, "citron-neo/emulator").await;
}

#[tokio::test]
#[ignore]
async fn citron_nightly_backend_e2e() {
    assert_citron_backend_e2e(CITRON_NIGHTLY_BRANCH, "citron-neo/CI").await;
}

#[tokio::test]
#[ignore]
async fn citron_stable_install_backend_e2e() {
    assert_citron_install_backend_e2e(CITRON_STABLE_BRANCH, "citron-neo/emulator").await;
}

#[tokio::test]
#[ignore]
async fn citron_nightly_install_backend_e2e() {
    assert_citron_install_backend_e2e(CITRON_NIGHTLY_BRANCH, "citron-neo/CI").await;
}
