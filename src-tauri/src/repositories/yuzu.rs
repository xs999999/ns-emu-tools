//! Release metadata access for Eden/Citron emulator branches.

use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::models::yuzu_branch::{
    normalize_yuzu_branch, CITRON_NIGHTLY_BRANCH, CITRON_STABLE_BRANCH, EDEN_BRANCH,
};
use crate::services::network::{request_git_api, request_github_api};
use std::collections::HashSet;
use tracing::{debug, info};

const EDEN_RELEASES_API: &str = "https://git.eden-emu.dev/api/v1/repos/eden-emu/eden/releases";
const CITRON_STABLE_RELEASES_API: &str =
    "https://api.github.com/repos/citron-neo/emulator/releases";
const CITRON_NIGHTLY_RELEASES_API: &str = "https://api.github.com/repos/citron-neo/CI/releases";

fn unsupported_yuzu_branch_error(branch: &str) -> AppError {
    AppError::InvalidArgument(format!(
        "Unsupported Yuzu branch: {}; supported branches: eden, citron-stable, citron-nightly",
        branch
    ))
}

pub async fn get_all_yuzu_release_versions(branch: &str) -> AppResult<Vec<String>> {
    match normalize_yuzu_branch(branch) {
        Some(EDEN_BRANCH) => get_eden_all_release_versions().await,
        Some(CITRON_STABLE_BRANCH) => get_citron_all_release_versions(CITRON_STABLE_BRANCH).await,
        Some(CITRON_NIGHTLY_BRANCH) => get_citron_all_release_versions(CITRON_NIGHTLY_BRANCH).await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

pub async fn get_yuzu_all_release_info(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    match normalize_yuzu_branch(branch) {
        Some(EDEN_BRANCH) => get_eden_all_release_info().await,
        Some(CITRON_STABLE_BRANCH) => get_citron_all_release_info(CITRON_STABLE_BRANCH).await,
        Some(CITRON_NIGHTLY_BRANCH) => get_citron_all_release_info(CITRON_NIGHTLY_BRANCH).await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

pub async fn get_yuzu_release_info_by_version(
    version: &str,
    branch: &str,
) -> AppResult<ReleaseInfo> {
    match normalize_yuzu_branch(branch) {
        Some(EDEN_BRANCH) => get_eden_release_info_by_version(version).await,
        Some(CITRON_STABLE_BRANCH) => {
            get_citron_release_info_by_version(version, CITRON_STABLE_BRANCH).await
        }
        Some(CITRON_NIGHTLY_BRANCH) => {
            get_citron_release_info_by_version(version, CITRON_NIGHTLY_BRANCH).await
        }
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

pub async fn get_eden_all_release_info() -> AppResult<Vec<ReleaseInfo>> {
    info!("Fetching Eden release information");

    let data = request_git_api(EDEN_RELEASES_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("Invalid Forgejo API response".to_string()))?
        .iter()
        .filter_map(ReleaseInfo::from_forgejo_api)
        .collect();

    debug!("Fetched {} Eden releases", releases.len());
    Ok(releases)
}

pub async fn get_eden_all_release_versions() -> AppResult<Vec<String>> {
    let releases = get_eden_all_release_info().await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

pub async fn get_eden_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("Fetching Eden release information for {}", version);

    let url = format!("{}/tags/{}", EDEN_RELEASES_API, version);
    let data = request_git_api(&url).await?;

    ReleaseInfo::from_forgejo_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("Unable to parse Eden release {}", version))
    })
}

fn citron_releases_api(branch: &str) -> Option<&'static str> {
    match normalize_yuzu_branch(branch)? {
        CITRON_STABLE_BRANCH => Some(CITRON_STABLE_RELEASES_API),
        CITRON_NIGHTLY_BRANCH => Some(CITRON_NIGHTLY_RELEASES_API),
        _ => None,
    }
}

pub fn yuzu_release_api_for_branch(branch: &str) -> Option<&'static str> {
    match normalize_yuzu_branch(branch)? {
        EDEN_BRANCH => Some(EDEN_RELEASES_API),
        CITRON_STABLE_BRANCH => Some(CITRON_STABLE_RELEASES_API),
        CITRON_NIGHTLY_BRANCH => Some(CITRON_NIGHTLY_RELEASES_API),
        _ => None,
    }
}

async fn get_citron_all_release_info(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    let api = citron_releases_api(branch).ok_or_else(|| unsupported_yuzu_branch_error(branch))?;
    info!("Fetching Citron release information for {}", branch);

    let data = request_github_api(api).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("Invalid GitHub API response".to_string()))?
        .iter()
        .filter_map(ReleaseInfo::from_github_api)
        .collect();

    let releases = if normalize_yuzu_branch(branch) == Some(CITRON_NIGHTLY_BRANCH) {
        normalize_citron_nightly_releases(releases)
    } else {
        releases
    };

    debug!("Fetched {} Citron releases for {}", releases.len(), branch);
    Ok(releases)
}

async fn get_citron_all_release_versions(branch: &str) -> AppResult<Vec<String>> {
    let releases = get_citron_all_release_info(branch).await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

async fn get_citron_release_info_by_version(version: &str, branch: &str) -> AppResult<ReleaseInfo> {
    let api = citron_releases_api(branch).ok_or_else(|| unsupported_yuzu_branch_error(branch))?;
    info!(
        "Fetching Citron release information for {} ({})",
        version, branch
    );

    if normalize_yuzu_branch(branch) == Some(CITRON_NIGHTLY_BRANCH) {
        let normalized_version =
            normalize_short_sha(version).unwrap_or_else(|| version.to_string());
        let releases = get_citron_all_release_info(CITRON_NIGHTLY_BRANCH).await?;

        return releases
            .into_iter()
            .find(|release| release.tag_name.eq_ignore_ascii_case(&normalized_version))
            .ok_or_else(|| {
                AppError::InvalidArgument(format!("Unable to parse Citron release {}", version))
            });
    }

    let url = format!("{}/tags/{}", api, version);
    let data = request_github_api(&url).await?;

    ReleaseInfo::from_github_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("Unable to parse Citron release {}", version))
    })
}

fn normalize_citron_nightly_releases(releases: Vec<ReleaseInfo>) -> Vec<ReleaseInfo> {
    let mut normalized_releases: Vec<ReleaseInfo> = Vec::new();

    for mut release in releases {
        let Some(upstream_sha) = citron_nightly_upstream_sha(&release) else {
            continue;
        };

        release.tag_name = upstream_sha.clone();

        if let Some(existing) = normalized_releases
            .iter_mut()
            .find(|existing| existing.tag_name == upstream_sha)
        {
            merge_release_assets(existing, release.assets);
        } else {
            normalized_releases.push(release);
        }
    }

    normalized_releases
}

fn merge_release_assets(
    existing: &mut ReleaseInfo,
    assets: Vec<crate::models::release::ReleaseAsset>,
) {
    let mut seen_assets: HashSet<(String, String)> = existing
        .assets
        .iter()
        .map(|asset| (asset.name.clone(), asset.download_url.clone()))
        .collect();

    for asset in assets {
        let key = (asset.name.clone(), asset.download_url.clone());
        if seen_assets.insert(key) {
            existing.assets.push(asset);
        }
    }
}

fn citron_nightly_upstream_sha(release: &ReleaseInfo) -> Option<String> {
    extract_labeled_upstream_sha(&release.description)
        .or_else(|| extract_labeled_upstream_sha(&release.name))
        .or_else(|| extract_short_sha_from_text(&release.name))
        .or_else(|| {
            release
                .assets
                .iter()
                .find_map(|asset| extract_short_sha_from_text(&asset.name))
        })
}

fn extract_labeled_upstream_sha(text: &str) -> Option<String> {
    let label = "citron upstream commit:";
    let text_lower = text.to_ascii_lowercase();
    let start = text_lower.find(label)? + label.len();
    extract_short_sha_from_text(&text[start..])
}

fn extract_short_sha_from_text(text: &str) -> Option<String> {
    let mut run_start: Option<usize> = None;

    for (index, ch) in text.char_indices() {
        if ch.is_ascii_hexdigit() {
            if run_start.is_none() {
                run_start = Some(index);
            }
        } else if let Some(start) = run_start.take() {
            if let Some(sha) = normalize_short_sha(&text[start..index]) {
                return Some(sha);
            }
        }
    }

    run_start.and_then(|start| normalize_short_sha(&text[start..]))
}

fn normalize_short_sha(value: &str) -> Option<String> {
    let candidate = value.trim_matches(|ch: char| !ch.is_ascii_hexdigit());
    if candidate.len() < 7 || !candidate.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    Some(candidate[..7].to_ascii_lowercase())
}

pub async fn get_latest_change_log(branch: &str) -> AppResult<String> {
    let releases = get_yuzu_all_release_info(branch).await?;

    if releases.is_empty() {
        return Ok(format!(
            "Unable to fetch latest changelog information for {}",
            branch
        ));
    }

    Ok(releases[0].description.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::release::ReleaseAsset;
    use crate::models::yuzu_branch::LEGACY_CITRON_BRANCH;

    #[tokio::test]
    #[ignore]
    async fn test_get_eden_releases() {
        let versions = get_eden_all_release_versions().await.unwrap();
        assert!(!versions.is_empty());
        println!("Eden versions: {:?}", &versions[..5.min(versions.len())]);
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_citron_stable_releases() {
        let versions = get_citron_all_release_versions(CITRON_STABLE_BRANCH)
            .await
            .unwrap();
        assert!(!versions.is_empty());
        println!(
            "Citron Stable versions: {:?}",
            &versions[..5.min(versions.len())]
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_citron_nightly_releases() {
        let versions = get_citron_all_release_versions(CITRON_NIGHTLY_BRANCH)
            .await
            .unwrap();
        assert!(!versions.is_empty());
        println!(
            "Citron Nightly versions: {:?}",
            &versions[..5.min(versions.len())]
        );
    }

    #[test]
    fn test_parse_citron_github_release() {
        let json = serde_json::json!({
            "name": "2026-04-27",
            "tag_name": "2026-04-27",
            "body": "Citron release notes",
            "published_at": "2026-04-27T00:00:00Z",
            "prerelease": false,
            "html_url": "https://github.com/citron-neo/emulator/releases/tag/2026-04-27",
            "assets": [
                {
                    "name": "Citron-windows-nightly-0237a9b88-x64-msvc.zip",
                    "browser_download_url": "https://github.com/citron-neo/emulator/releases/download/2026-04-27/Citron-windows-nightly-0237a9b88-x64-msvc.zip",
                    "size": 40495923,
                    "content_type": "application/zip"
                }
            ]
        });

        let release = ReleaseInfo::from_github_api(&json).unwrap();

        assert_eq!(release.tag_name, "2026-04-27");
        assert_eq!(release.description, "Citron release notes");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(
            release.assets[0].content_type.as_deref(),
            Some("application/zip")
        );
    }

    #[test]
    fn test_citron_nightly_release_versions_use_upstream_sha() {
        let releases = vec![
            ReleaseInfo {
                name: "Windows Nightly: fab192f77".to_string(),
                tag_name: "nightly-windows".to_string(),
                description: "Citron Upstream Commit: fab192f77".to_string(),
                assets: vec![ReleaseAsset {
                    name: "Citron-windows-nightly-fab192f77-x64-msvc.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                }],
                published_at: None,
                prerelease: true,
                html_url: None,
            },
            ReleaseInfo {
                name: "macOS Nightly: FAB192F77".to_string(),
                tag_name: "nightly-macos".to_string(),
                description: "Citron Upstream Commit: FAB192F77".to_string(),
                assets: vec![ReleaseAsset {
                    name: "Citron-macOS-nightly-fab192f77.dmg".to_string(),
                    download_url: "https://example.com/macos.dmg".to_string(),
                    size: 0,
                    content_type: None,
                }],
                published_at: None,
                prerelease: true,
                html_url: None,
            },
        ];

        let normalized = normalize_citron_nightly_releases(releases);

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].tag_name, "fab192f");
        assert_eq!(normalized[0].assets.len(), 2);
        assert!(normalized[0]
            .assets
            .iter()
            .any(|asset| asset.name.contains("windows")));
        assert!(normalized[0]
            .assets
            .iter()
            .any(|asset| asset.name.contains("macOS")));
    }

    #[test]
    fn test_citron_nightly_sha_falls_back_to_asset_name() {
        let release = ReleaseInfo {
            name: "Linux Nightly".to_string(),
            tag_name: "nightly-linux".to_string(),
            description: String::new(),
            assets: vec![ReleaseAsset {
                name: "citron_nightly-0237a9b88-linux-x86_64.AppImage".to_string(),
                download_url: "https://example.com/linux.AppImage".to_string(),
                size: 0,
                content_type: None,
            }],
            published_at: None,
            prerelease: true,
            html_url: None,
        };

        assert_eq!(
            citron_nightly_upstream_sha(&release),
            Some("0237a9b".to_string())
        );
    }

    #[test]
    fn test_unknown_yuzu_branch_is_rejected() {
        let error = unsupported_yuzu_branch_error("unknown");
        assert!(matches!(error, AppError::InvalidArgument(_)));
        assert!(error.to_string().contains("eden"));
        assert!(error.to_string().contains("citron-stable"));
        assert!(error.to_string().contains("citron-nightly"));
    }

    #[test]
    fn test_citron_release_api_by_branch() {
        assert_eq!(
            yuzu_release_api_for_branch(CITRON_STABLE_BRANCH),
            Some(CITRON_STABLE_RELEASES_API)
        );
        assert_eq!(
            yuzu_release_api_for_branch(LEGACY_CITRON_BRANCH),
            Some(CITRON_STABLE_RELEASES_API)
        );
        assert_eq!(
            yuzu_release_api_for_branch(CITRON_NIGHTLY_BRANCH),
            Some(CITRON_NIGHTLY_RELEASES_API)
        );
    }
}
