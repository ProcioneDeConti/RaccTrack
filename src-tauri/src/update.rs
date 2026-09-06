//! Lightweight update check against the GitHub Releases API.
//!
//! Deliberately *not* the Tauri updater plugin: the app ships unsigned, has
//! no code-signing certificate and no CI, and a portable copy can't be
//! updated in place. So this just asks GitHub for the latest release, tells
//! the frontend whether it's newer than the running build, and hands back a
//! download URL — the existing `open_external` command opens it in the
//! browser. Nothing is downloaded or installed by the app itself.

use serde::{Deserialize, Serialize};

use crate::db::Db;

const RELEASES_API: &str =
    "https://api.github.com/repos/ProcioneDeConti/RaccTrack/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/ProcioneDeConti/RaccTrack/releases/latest";

/// On automatic (startup) checks, reuse a cached successful result younger
/// than this instead of hitting the API again.
const AUTO_CHECK_TTL_MS: i64 = 20 * 60 * 60 * 1000;
const CACHE_KEY: &str = "update:last-check";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// The running version (`CARGO_PKG_VERSION`).
    pub current: String,
    /// The latest release's version, `v` prefix stripped. Empty if the check
    /// failed before a release could be read.
    pub latest: String,
    /// True when `latest` parses as strictly newer than `current`.
    pub newer: bool,
    /// Release page, for a "what's new" link.
    pub url: String,
    /// Direct download URL for the asset matching this install kind (portable
    /// zip vs. setup exe), when one was found in the release.
    pub asset_url: Option<String>,
    /// Release notes (the GitHub release body), trimmed. `None` if empty or
    /// unavailable.
    pub notes: Option<String>,
    /// Release publish time, ISO-8601, exactly as GitHub returns it.
    pub published_at: Option<String>,
    /// Set when the check itself failed (network / parse). The frontend shows
    /// this on a manual check and silently ignores it on startup.
    pub error: Option<String>,
}

impl UpdateInfo {
    fn failed(current: String, error: String) -> Self {
        Self {
            current,
            latest: String::new(),
            newer: false,
            url: RELEASE_PAGE.to_string(),
            asset_url: None,
            notes: None,
            published_at: None,
            error: Some(error),
        }
    }
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Parse `MAJOR.MINOR[.PATCH]` (optionally `v`-prefixed, optionally with a
/// `-prerelease` / `+build` suffix) into a comparable tuple. Anything that
/// doesn't parse cleanly returns `None`.
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// True when `latest` is a strictly higher version than `current`. A version
/// that can't be parsed on either side is treated as "not newer" (never nag
/// on garbage).
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Pick the release asset that matches how this copy is installed — the
/// portable zip for a portable copy, the NSIS installer otherwise — falling
/// back to any `.exe` / `.zip` if the naming doesn't match what's expected.
fn pick_asset(assets: &[GhAsset], portable: bool) -> Option<String> {
    let matches = |a: &GhAsset| {
        let n = a.name.to_lowercase();
        if portable {
            n.contains("portable") && n.ends_with(".zip")
        } else {
            n.ends_with(".exe") && n.contains("setup")
        }
    };
    assets
        .iter()
        .find(|a| matches(a))
        .or_else(|| {
            assets.iter().find(|a| {
                let n = a.name.to_lowercase();
                n.ends_with(".exe") || n.ends_with(".zip")
            })
        })
        .map(|a| a.browser_download_url.clone())
}

/// Check GitHub for a newer release. `force` skips the cache (for the
/// "Check for updates" button); otherwise a fresh cached result is reused.
pub async fn check(http: &reqwest::Client, db: &Db, portable: bool, force: bool) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();

    if !force {
        if let Ok(Some(json)) = db.kv_get(CACHE_KEY, AUTO_CHECK_TTL_MS) {
            if let Ok(mut cached) = serde_json::from_str::<UpdateInfo>(&json) {
                // The running version may have moved on since the cache was
                // written (the user updated) — recompute against it.
                cached.newer = !cached.latest.is_empty() && is_newer(&cached.latest, &current);
                cached.current = current;
                return cached;
            }
        }
    }

    let resp = match http
        .get(RELEASES_API)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return UpdateInfo::failed(current, e.to_string()),
    };
    if !resp.status().is_success() {
        return UpdateInfo::failed(current, format!("GitHub API returned HTTP {}", resp.status()));
    }
    let rel: GhRelease = match resp.json().await {
        Ok(r) => r,
        Err(e) => return UpdateInfo::failed(current, e.to_string()),
    };

    let latest = rel
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    let newer = !rel.draft && !rel.prerelease && is_newer(&latest, &current);
    let notes = rel
        .body
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string);

    let info = UpdateInfo {
        current,
        latest,
        newer,
        url: if rel.html_url.is_empty() {
            RELEASE_PAGE.to_string()
        } else {
            rel.html_url
        },
        asset_url: pick_asset(&rel.assets, portable),
        notes,
        published_at: rel.published_at,
        error: None,
    };

    if let Ok(json) = serde_json::to_string(&info) {
        let _ = db.kv_put(CACHE_KEY, &json);
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.3.3", "0.3.2"));
        assert!(is_newer("v0.4.0", "0.3.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.3.2", "0.3.2"));
        assert!(!is_newer("0.3.1", "0.3.2"));
        assert!(!is_newer("0.3", "0.3.0"));
        assert!(!is_newer("garbage", "0.3.2"));
    }

    #[test]
    fn asset_selection() {
        let assets = vec![
            GhAsset {
                name: "RaccTrack-ADSB_0.3.2_x64-portable.zip".into(),
                browser_download_url: "https://example.com/portable.zip".into(),
            },
            GhAsset {
                name: "RaccTrack.ADS-B._0.3.2_x64-setup.exe".into(),
                browser_download_url: "https://example.com/setup.exe".into(),
            },
        ];
        assert_eq!(
            pick_asset(&assets, true).as_deref(),
            Some("https://example.com/portable.zip")
        );
        assert_eq!(
            pick_asset(&assets, false).as_deref(),
            Some("https://example.com/setup.exe")
        );
        assert_eq!(pick_asset(&[], false), None);
    }
}
