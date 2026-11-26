//! In-app updates from Kanso's official GitHub Releases.
//!
//! Kanso has no update server of its own: it reads GitHub's public Releases API for this
//! repository (`SpheralComa441/Kanso`), without an account or token, and compares the latest
//! stable release with the running build's version.
//!
//! When the user chooses to install an update, the installer (NSIS `-setup.exe` or WiX `.msi`,
//! matching how this copy of Kanso was installed) is streamed over HTTPS only from GitHub's own
//! hosts into the app's local `updates/` folder, checked against the published SHA-256 sum
//! (`SHA256SUMS.txt` and GitHub's asset digest), verified to be a real Windows installer header,
//! and then handed to Windows in unattended update mode so Kanso restarts into the new version.

use crate::i18n::T;
use ring::digest::{Context as Sha256Context, SHA256};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use url::Url;

pub const REPO_OWNER: &str = "SpheralComa441";
pub const REPO_NAME: &str = "Kanso";
pub const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/SpheralComa441/Kanso/releases/latest";
pub const RELEASES_PAGE_URL: &str = "https://github.com/SpheralComa441/Kanso/releases";

const ALLOWED_DOWNLOAD_PREFIX: &str =
    "https://github.com/SpheralComa441/Kanso/releases/download/";
const ALLOWED_RELEASE_URL_PREFIX: &str = "https://github.com/SpheralComa441/Kanso/releases/";
const CHECKSUMS_ASSET_NAME: &str = "SHA256SUMS.txt";

/// Largest installer accepted from GitHub: Kanso's are around 7–12 MB.
const MAX_INSTALLER_BYTES: u64 = 150 * 1024 * 1024;
/// Largest `SHA256SUMS.txt` accepted: a release lists a handful of files.
const MAX_CHECKSUMS_BYTES: u64 = 64 * 1024;
/// Largest GitHub release metadata response accepted.
const MAX_RELEASE_JSON_BYTES: u64 = 512 * 1024;
/// Most HTTP redirects followed when fetching a release asset from GitHub.
const MAX_REDIRECTS: usize = 5;
/// Chunk size used while streaming the installer and reporting progress.
const STREAM_CHUNK_BYTES: usize = 64 * 1024;

/// A canonical three-part release version (`major.minor.patch`), with no pre-release suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl AppVersion {
    /// Parses `0.2.3` or `v0.2.3`. Pre-releases (`0.3.0-rc.1`), build metadata and anything
    /// else are rejected so automatic updates only ever offer stable releases.
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let body = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
            .unwrap_or(trimmed);
        let mut parts = body.split('.');
        let major = parse_component(parts.next()?)?;
        let minor = parse_component(parts.next()?)?;
        let patch = parse_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self { major, minor, patch })
    }

    /// The version of this build of Kanso.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self {
            major: 0,
            minor: 0,
            patch: 0,
        })
    }
}

fn parse_component(part: &str) -> Option<u64> {
    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

impl fmt::Display for AppVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Which Windows installer format a release asset is, matching how Kanso was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallerKind {
    Nsis,
    Msi,
}

impl InstallerKind {
    fn matches_asset_name(self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        match self {
            Self::Nsis => lower.ends_with("-setup.exe"),
            Self::Msi => lower.ends_with(".msi"),
        }
    }
}

/// What the interface shows when a newer release is available. Never carries executable paths
/// or arbitrary URLs back to the backend: the backend keeps the validated [`CheckedRelease`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub version: String,
    pub name: String,
    pub notes: String,
    pub published_at: Option<String>,
    pub release_url: String,
    pub installer_kind: InstallerKind,
    pub asset_name: String,
    pub asset_size: u64,
}

/// A validated newer release kept in backend memory until the user decides to install it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedRelease {
    pub info: UpdateInfo,
    pub download_url: String,
    pub checksums_url: Option<String>,
    pub asset_digest_sha256: Option<String>,
}

/// Download progress emitted while fetching an installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Validates that `url` points to a release page of this repository over HTTPS.
pub fn is_allowed_release_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.port().is_none()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.as_str().starts_with(ALLOWED_RELEASE_URL_PREFIX)
}

/// Validates that `url` points to an official release asset download of this repository over HTTPS.
pub fn is_allowed_download_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.port().is_none()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.as_str().starts_with(ALLOWED_DOWNLOAD_PREFIX)
}

/// Validates that a redirect target stays on HTTPS and on GitHub's own asset hosts
/// (`github.com`, `api.github.com`, or `*.githubusercontent.com`).
pub fn is_allowed_redirect_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https"
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == "github.com"
        || host == "api.github.com"
        || (host.ends_with(".githubusercontent.com") && host.len() > ".githubusercontent.com".len())
}

/// Returns `true` for a plain release asset file name with no path separators or control chars.
fn is_safe_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.chars().any(char::is_control)
}

/// Removes the manual-installation footer `.github/workflows/release.yml` appends to every
/// release body (`### Installing` onwards), keeping only the actual changelog notes.
pub fn extract_release_notes(body: &str) -> String {
    let normalized = body.replace("\r\n", "\n");
    let mut kept_lines = Vec::new();
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("### Installing")
            || trimmed.eq_ignore_ascii_case("## Installing")
        {
            break;
        }
        kept_lines.push(line);
    }
    kept_lines.join("\n").trim().to_string()
}

/// Parses a GitHub API asset `digest` field such as `"sha256:<64 hex chars>"`.
pub fn parse_github_digest(digest: &str) -> Option<String> {
    let hex = digest.trim().strip_prefix("sha256:")?.trim();
    normalize_sha256_hex(hex)
}

/// Looks up `asset_name`'s SHA-256 sum inside a standard `sha256sum` listing (`SHA256SUMS.txt`).
pub fn parse_sha256_sums(listing: &str, asset_name: &str) -> Option<String> {
    for line in listing.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Standard `sha256sum` separates the 64-hex hash and filename with whitespace
        // (optionally prefixed with `*` for binary mode or `./`).
        let Some((hash, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let file = rest.trim().trim_start_matches('*').trim_start_matches("./");
        if file.eq_ignore_ascii_case(asset_name) {
            return normalize_sha256_hex(hash);
        }
    }
    None
}

fn normalize_sha256_hex(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() == 64 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(trimmed.to_ascii_lowercase())
    } else {
        None
    }
}

/// Checks the leading magic bytes of a downloaded installer so an HTML error page or corrupted
/// payload is rejected even before launch.
pub fn is_valid_installer_header(kind: InstallerKind, header: &[u8]) -> bool {
    match kind {
        // PE executable (`MZ`)
        InstallerKind::Nsis => header.len() >= 2 && &header[..2] == b"MZ",
        // MSI is an OLE Compound File (`D0 CF 11 E0 A1 B1 1A E1`)
        InstallerKind::Msi => {
            header.len() >= 8 && header[..8] == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// Reads Kanso's latest release from GitHub and downloads verified installers.
pub struct UpdateClient {
    api_url: String,
    strict_hosts: bool,
    agent: ureq::Agent,
}

impl Default for UpdateClient {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateClient {
    pub fn new() -> Self {
        Self::build(LATEST_RELEASE_API_URL, true)
    }

    #[cfg(test)]
    pub(crate) fn for_tests(api_url: &str) -> Self {
        Self::build(api_url, false)
    }

    fn build(api_url: &str, strict_hosts: bool) -> Self {
        // Redirects are followed manually in `get_validated` so every hop is checked.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(60))
            .redirects(0)
            .user_agent(crate::USER_AGENT)
            .build();
        Self {
            api_url: api_url.to_string(),
            strict_hosts,
            agent,
        }
    }

    fn check_initial_download_url(&self, url: &str) -> bool {
        if self.strict_hosts {
            is_allowed_download_url(url)
        } else {
            Url::parse(url).is_ok()
        }
    }

    fn check_redirect_url(&self, url: &str) -> bool {
        if self.strict_hosts {
            is_allowed_redirect_url(url)
        } else {
            Url::parse(url).is_ok()
        }
    }

    fn check_release_page_url(&self, url: &str) -> bool {
        if self.strict_hosts {
            is_allowed_release_url(url)
        } else {
            Url::parse(url).is_ok()
        }
    }

    /// Performs a GET request while validating every redirect hop.
    fn get_validated(
        &self,
        initial_url: &str,
        accept: &str,
    ) -> Result<ureq::Response, Box<ureq::Error>> {
        let mut current = initial_url.to_string();
        let is_api_call = accept == "application/vnd.github+json";
        for hop in 0..=MAX_REDIRECTS {
            let mut req = self.agent.get(&current).set("Accept", accept);
            if is_api_call && hop == 0 {
                req = req.set("X-GitHub-Api-Version", "2022-11-28");
            }
            let response = req.call().map_err(Box::new)?;
            let status = response.status();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let Some(location) = response.header("location").map(str::to_string) else {
                    return Err(Box::new(ureq::Error::Status(status, response)));
                };
                let base = Url::parse(&current).map_err(|e| {
                    Box::new(ureq::Error::from(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?;
                let next = base.join(&location).map_err(|e| {
                    Box::new(ureq::Error::from(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                })?;
                let next_str = next.as_str();
                if !self.check_redirect_url(next_str) {
                    return Err(Box::new(ureq::Error::from(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("{}: {next_str}", T::UrlNotAllowed.s()),
                    ))));
                }
                current = next_str.to_string();
                continue;
            }
            if !(200..300).contains(&status) {
                return Err(Box::new(ureq::Error::Status(status, response)));
            }
            return Ok(response);
        }
        Err(Box::new(ureq::Error::from(std::io::Error::other(
            "too many redirects",
        ))))
    }

    /// Checks whether a newer stable release than `current` is published on GitHub.
    /// Returns `Ok(None)` when already up to date (or when no stable release is published yet).
    pub fn check(
        &self,
        current: AppVersion,
        preferred: InstallerKind,
    ) -> Result<Option<CheckedRelease>, String> {
        let response = match self.get_validated(&self.api_url, "application/vnd.github+json") {
            Ok(resp) => resp,
            Err(err) => match *err {
                ureq::Error::Status(404, _) => return Ok(None),
                ureq::Error::Status(403 | 429, _) => {
                    return Err(T::UpdateRateLimited.s().to_string());
                }
                other => {
                    return Err(format!("{}: {other}", T::UpdateCheckFailed.s()));
                }
            },
        };

        let mut raw = Vec::new();
        response
            .into_reader()
            .take(MAX_RELEASE_JSON_BYTES)
            .read_to_end(&mut raw)
            .map_err(|e| format!("{}: {e}", T::UpdateCheckFailed.s()))?;
        let release: GitHubRelease = serde_json::from_slice(&raw)
            .map_err(|e| format!("{}: {e}", T::UpdateCheckFailed.s()))?;

        if release.draft || release.prerelease {
            return Ok(None);
        }
        let Some(remote_version) = AppVersion::parse(&release.tag_name) else {
            return Ok(None);
        };
        if remote_version <= current {
            return Ok(None);
        }

        let (installer_kind, asset) = select_installer_asset(&release.assets, preferred)
            .ok_or_else(|| T::UpdateNoInstaller.s().to_string())?;
        if !self.check_initial_download_url(&asset.browser_download_url) {
            return Err(format!("{}: {}", T::UrlNotAllowed.s(), asset.browser_download_url));
        }

        let checksums_url = release
            .assets
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(CHECKSUMS_ASSET_NAME))
            .map(|entry| entry.browser_download_url.clone())
            .filter(|url| self.check_initial_download_url(url));

        let asset_digest_sha256 = asset.digest.as_deref().and_then(parse_github_digest);

        let release_url = release
            .html_url
            .filter(|url| self.check_release_page_url(url))
            .unwrap_or_else(|| RELEASES_PAGE_URL.to_string());

        let name = release
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Kanso {remote_version}"));
        let notes = release
            .body
            .as_deref()
            .map(extract_release_notes)
            .unwrap_or_default();

        Ok(Some(CheckedRelease {
            info: UpdateInfo {
                current_version: current.to_string(),
                version: remote_version.to_string(),
                name,
                notes,
                published_at: release.published_at,
                release_url,
                installer_kind,
                asset_name: asset.name.clone(),
                asset_size: asset.size,
            },
            download_url: asset.browser_download_url.clone(),
            checksums_url,
            asset_digest_sha256,
        }))
    }

    /// Resolves the expected SHA-256 sum for `release` from `SHA256SUMS.txt` and/or GitHub's
    /// asset digest. If both exist, they must agree.
    fn resolve_expected_sha256(&self, release: &CheckedRelease) -> Result<String, String> {
        let from_file = match release.checksums_url.as_deref() {
            Some(url) => {
                if !self.check_initial_download_url(url) {
                    return Err(format!("{}: {url}", T::UrlNotAllowed.s()));
                }
                let resp = self
                    .get_validated(url, "text/plain")
                    .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
                let mut text = String::new();
                resp.into_reader()
                    .take(MAX_CHECKSUMS_BYTES)
                    .read_to_string(&mut text)
                    .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
                parse_sha256_sums(&text, &release.info.asset_name)
            }
            None => None,
        };

        match (from_file, release.asset_digest_sha256.as_deref()) {
            (Some(file_hash), Some(api_hash)) => {
                if !file_hash.eq_ignore_ascii_case(api_hash) {
                    return Err(T::UpdateChecksumMismatch.s().to_string());
                }
                Ok(file_hash)
            }
            (Some(file_hash), None) => Ok(file_hash),
            (None, Some(api_hash)) => Ok(api_hash.to_ascii_lowercase()),
            (None, None) => Err(T::UpdateNoChecksum.s().to_string()),
        }
    }

    /// Downloads `release` into `updates_dir`, verifies its SHA-256 checksum and executable
    /// header, and returns the path of the verified installer ready to launch.
    pub fn download_and_verify<F>(
        &self,
        release: &CheckedRelease,
        updates_dir: &Path,
        cancel: &AtomicBool,
        mut on_progress: F,
    ) -> Result<PathBuf, String>
    where
        F: FnMut(DownloadProgress),
    {
        if cancel.load(Ordering::Relaxed) {
            return Err(T::UpdateCancelled.s().to_string());
        }
        if !is_safe_asset_name(&release.info.asset_name) {
            return Err(T::UpdateNoInstaller.s().to_string());
        }
        if !self.check_initial_download_url(&release.download_url) {
            return Err(format!("{}: {}", T::UrlNotAllowed.s(), release.download_url));
        }

        let expected_sha256 = self.resolve_expected_sha256(release)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(T::UpdateCancelled.s().to_string());
        }

        fs::create_dir_all(updates_dir).map_err(|e| {
            format!("{} {}: {e}", T::CannotCreateFolder.s(), updates_dir.display())
        })?;

        let destination = updates_dir.join(&release.info.asset_name);
        let incoming = updates_dir.join(format!("{}.part", release.info.asset_name));
        let _ = fs::remove_file(&incoming);

        let response = self
            .get_validated(&release.download_url, "application/octet-stream")
            .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;

        let total = response
            .header("content-length")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&n| n > 0)
            .or((release.info.asset_size > 0).then_some(release.info.asset_size));

        if let Some(len) = total {
            if len > MAX_INSTALLER_BYTES {
                return Err(format!("{}: file too large", T::UpdateDownloadFailed.s()));
            }
        }

        on_progress(DownloadProgress {
            downloaded: 0,
            total,
        });

        let outcome = (|| -> Result<PathBuf, String> {
            let mut reader = response.into_reader();
            let mut file = File::create(&incoming)
                .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
            let mut hasher = Sha256Context::new(&SHA256);
            let mut header = Vec::with_capacity(8);
            let mut buf = vec![0u8; STREAM_CHUNK_BYTES];
            let mut downloaded: u64 = 0;

            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(T::UpdateCancelled.s().to_string());
                }
                let read = reader
                    .read(&mut buf)
                    .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
                if read == 0 {
                    break;
                }
                downloaded = downloaded.saturating_add(read as u64);
                if downloaded > MAX_INSTALLER_BYTES {
                    return Err(format!("{}: file too large", T::UpdateDownloadFailed.s()));
                }
                let chunk = &buf[..read];
                if header.len() < 8 {
                    let take = (8 - header.len()).min(chunk.len());
                    header.extend_from_slice(&chunk[..take]);
                }
                hasher.update(chunk);
                file.write_all(chunk)
                    .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
                on_progress(DownloadProgress { downloaded, total });
            }

            file.flush()
                .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
            drop(file);

            if release.info.asset_size > 0 && downloaded != release.info.asset_size {
                return Err(T::UpdateChecksumMismatch.s().to_string());
            }
            if !is_valid_installer_header(release.info.installer_kind, &header) {
                return Err(T::UpdateChecksumMismatch.s().to_string());
            }
            let actual_sha256 = hex_encode(hasher.finish().as_ref());
            if actual_sha256 != expected_sha256 {
                return Err(T::UpdateChecksumMismatch.s().to_string());
            }

            let _ = fs::remove_file(&destination);
            fs::rename(&incoming, &destination)
                .map_err(|e| format!("{}: {e}", T::UpdateDownloadFailed.s()))?;
            Ok(destination)
        })();

        if outcome.is_err() {
            let _ = fs::remove_file(&incoming);
        }
        outcome
    }
}

fn select_installer_asset(
    assets: &[GitHubAsset],
    preferred: InstallerKind,
) -> Option<(InstallerKind, &GitHubAsset)> {
    let fallback = match preferred {
        InstallerKind::Nsis => InstallerKind::Msi,
        InstallerKind::Msi => InstallerKind::Nsis,
    };
    for kind in [preferred, fallback] {
        if let Some(asset) = assets
            .iter()
            .find(|a| is_safe_asset_name(&a.name) && kind.matches_asset_name(&a.name))
        {
            return Some((kind, asset));
        }
    }
    None
}

/// Removes leftover installers from previous updates. Safe to call when the folder does not exist.
pub fn cleanup_old_installers(updates_dir: &Path) {
    let Ok(entries) = fs::read_dir(updates_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = fs::remove_file(path);
        }
    }
}

/// Returns `(program, parameters)` used to launch a verified Windows installer in passive
/// update mode with automatic relaunch once finished (matching Tauri's own NSIS and WiX scripts).
pub fn installer_launch_spec(kind: InstallerKind, installer_path: &Path) -> (OsString, OsString) {
    match kind {
        InstallerKind::Nsis => (
            installer_path.as_os_str().to_os_string(),
            OsString::from("/P /UPDATE /R"),
        ),
        InstallerKind::Msi => {
            let msiexec = std::env::var_os("SYSTEMROOT")
                .map(|root| {
                    let mut p = PathBuf::from(root);
                    p.push("System32");
                    p.push("msiexec.exe");
                    p.into_os_string()
                })
                .unwrap_or_else(|| OsString::from("msiexec.exe"));
            let mut params = OsString::from("/i \"");
            params.push(installer_path.as_os_str());
            params.push("\" /passive /promptrestart AUTOLAUNCHAPP=True");
            (msiexec, params)
        }
    }
}

/// Starts a verified installer on Windows using `ShellExecuteW`, which properly requests UAC
/// elevation when an MSI or per-machine install needs it. Returns `Ok(())` once Windows has
/// launched the installer process so the caller can exit immediately.
#[cfg(target_os = "windows")]
pub fn launch_installer(kind: InstallerKind, installer_path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW;

    if !installer_path.is_file() {
        return Err(format!("{}: {}", T::UpdateInstallFailed.s(), installer_path.display()));
    }
    let (program, params) = installer_launch_spec(kind, installer_path);
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = program.encode_wide().chain(Some(0)).collect();
    let parameters: Vec<u16> = params.encode_wide().chain(Some(0)).collect();

    // SAFETY: `verb`, `file`, and `parameters` are NUL-terminated UTF-16 buffers that outlive the
    // synchronous `ShellExecuteW` call. Return values <= 32 indicate an error code.
    let result = unsafe {
        ShellExecuteW(
            0,
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            ptr::null(),
            SW_SHOW,
        )
    };
    if result <= 32 {
        return Err(format!("{} (code {result})", T::UpdateInstallFailed.s()));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn launch_installer(_kind: InstallerKind, _installer_path: &Path) -> Result<(), String> {
    Err(T::UpdateWindowsOnly.s().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_http;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn parses_and_orders_canonical_versions_only() {
        let v023 = AppVersion::parse("v0.2.3").unwrap();
        let v024 = AppVersion::parse("0.2.4").unwrap();
        let v0100 = AppVersion::parse("v0.10.0").unwrap();
        let v100 = AppVersion::parse("1.0.0").unwrap();
        assert!(v023 < v024);
        assert!(v024 < v0100);
        assert!(v0100 < v100);
        assert_eq!(v023.to_string(), "0.2.3");
        assert_eq!(AppVersion::parse("V1.2.3"), Some(AppVersion { major: 1, minor: 2, patch: 3 }));

        for bad in ["", "0.2", "0.2.3.4", "v0.3.0-beta.1", "v0.3.0+build", "abc", "0.2.-1"] {
            assert_eq!(AppVersion::parse(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn extracts_changelog_notes_without_manual_install_footer() {
        let body = "\n- **Emulated games in Favorites.** Emulated games marked with the star are now counted.\n- **In-app updates.** Kanso can now update itself.\n\n\n### Installing\n\nDownload the `-setup.exe` file below...\n";
        assert_eq!(
            extract_release_notes(body),
            "- **Emulated games in Favorites.** Emulated games marked with the star are now counted.\n- **In-app updates.** Kanso can now update itself."
        );

        let crlf = "- Fix one.\r\n\r\n### Installing\r\nManual steps";
        assert_eq!(extract_release_notes(crlf), "- Fix one.");

        let no_footer = "- Just release notes.";
        assert_eq!(extract_release_notes(no_footer), "- Just release notes.");
    }

    #[test]
    fn parses_sha256_sums_and_github_digests() {
        let hash_exe = "95699ef180a6b4bf4b4da1ed7b3fd67f00acd1abac59d632aa420725003c0ed9";
        let hash_msi = "d0ce33464f05f029801cf64400c34279fece8f13a31ca754e64c96dfa374fd20";
        let sums = format!(
            "malformed-line-without-spaces\n# comment\n{hash_exe}  Kanso_0.3.0_x64-setup.exe\n{hash_msi} *./Kanso_0.3.0_x64_en-US.msi\n"
        );
        assert_eq!(
            parse_sha256_sums(&sums, "Kanso_0.3.0_x64-setup.exe").as_deref(),
            Some(hash_exe)
        );
        assert_eq!(
            parse_sha256_sums(&sums, "Kanso_0.3.0_x64_en-US.msi").as_deref(),
            Some(hash_msi)
        );
        assert_eq!(parse_sha256_sums(&sums, "missing.exe"), None);

        assert_eq!(
            parse_github_digest(&format!("sha256:{}", hash_exe.to_ascii_uppercase())).as_deref(),
            Some(hash_exe)
        );
        assert_eq!(parse_github_digest("md5:1234"), None);
        assert_eq!(parse_github_digest("sha256:short"), None);
    }

    #[test]
    fn validates_github_urls_and_redirects_strictly() {
        assert!(is_allowed_release_url(
            "https://github.com/SpheralComa441/Kanso/releases/tag/v0.3.0"
        ));
        assert!(!is_allowed_release_url(
            "http://github.com/SpheralComa441/Kanso/releases/tag/v0.3.0"
        ));
        assert!(!is_allowed_release_url(
            "https://github.com/SomeoneElse/Kanso/releases/tag/v0.3.0"
        ));

        assert!(is_allowed_download_url(
            "https://github.com/SpheralComa441/Kanso/releases/download/v0.3.0/Kanso_0.3.0_x64-setup.exe"
        ));
        assert!(!is_allowed_download_url(
            "https://evil.example/Kanso_0.3.0_x64-setup.exe"
        ));
        assert!(!is_allowed_download_url(
            "https://github.com/Other/Kanso/releases/download/v0.3.0/Kanso_0.3.0_x64-setup.exe"
        ));

        assert!(is_allowed_redirect_url(
            "https://release-assets.githubusercontent.com/github-production-release-asset/123"
        ));
        assert!(is_allowed_redirect_url(
            "https://objects.githubusercontent.com/github-production-release-asset/123"
        ));
        assert!(!is_allowed_redirect_url(
            "https://objects.githubusercontent.com:8443/file"
        ));
        assert!(!is_allowed_redirect_url(
            "http://objects.githubusercontent.com/file"
        ));
        assert!(!is_allowed_redirect_url(
            "https://githubusercontent.com.evil.com/file"
        ));
        assert!(!is_allowed_redirect_url(
            "https://github.com@evil.com/file"
        ));
    }

    #[test]
    fn builds_passive_relaunch_commands_for_nsis_and_msi() {
        let exe = Path::new(r"C:\Users\Alex\AppData\Local\com.kanso.launcher\updates\Kanso_0.3.0_x64-setup.exe");
        let (prog, params) = installer_launch_spec(InstallerKind::Nsis, exe);
        assert_eq!(prog, exe.as_os_str());
        assert_eq!(params, "/P /UPDATE /R");

        let msi = Path::new(r"C:\Users\Alex\AppData\Local\com.kanso.launcher\updates\Kanso_0.3.0_x64_en-US.msi");
        let (prog, params) = installer_launch_spec(InstallerKind::Msi, msi);
        assert!(prog.to_string_lossy().to_ascii_lowercase().ends_with("msiexec.exe"));
        assert_eq!(
            params.to_string_lossy(),
            format!("/i \"{}\" /passive /promptrestart AUTOLAUNCHAPP=True", msi.display())
        );
    }

    #[test]
    fn checks_releases_and_respects_preferred_installer() {
        let base = test_http::spawn(|req| {
            if req.path == "/latest" {
                let body = r#"{
                    "tag_name": "v0.3.0",
                    "name": "Kanso 0.3.0",
                    "body": "- **New updater.** Works right inside Kanso.\n\n### Installing\nManual steps",
                    "draft": false,
                    "prerelease": false,
                    "published_at": "2026-11-26T12:00:00Z",
                    "html_url": "https://github.com/SpheralComa441/Kanso/releases/tag/v0.3.0",
                    "assets": [
                        {
                            "name": "Kanso_0.3.0_x64-setup.exe",
                            "browser_download_url": "https://github.com/SpheralComa441/Kanso/releases/download/v0.3.0/Kanso_0.3.0_x64-setup.exe",
                            "size": 7000000,
                            "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        },
                        {
                            "name": "Kanso_0.3.0_x64_en-US.msi",
                            "browser_download_url": "https://github.com/SpheralComa441/Kanso/releases/download/v0.3.0/Kanso_0.3.0_x64_en-US.msi",
                            "size": 9000000,
                            "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                        },
                        {
                            "name": "SHA256SUMS.txt",
                            "browser_download_url": "https://github.com/SpheralComa441/Kanso/releases/download/v0.3.0/SHA256SUMS.txt",
                            "size": 180
                        }
                    ]
                }"#;
                (200, body.to_string())
            } else {
                (404, "{}".to_string())
            }
        });

        let client = UpdateClient::for_tests(&format!("{base}/latest"));
        let current = AppVersion { major: 0, minor: 2, patch: 3 };

        let nsis = client.check(current, InstallerKind::Nsis).unwrap().unwrap();
        assert_eq!(nsis.info.version, "0.3.0");
        assert_eq!(nsis.info.installer_kind, InstallerKind::Nsis);
        assert_eq!(nsis.info.asset_name, "Kanso_0.3.0_x64-setup.exe");
        assert_eq!(nsis.info.notes, "- **New updater.** Works right inside Kanso.");
        assert!(nsis.checksums_url.is_some());

        let msi = client.check(current, InstallerKind::Msi).unwrap().unwrap();
        assert_eq!(msi.info.installer_kind, InstallerKind::Msi);
        assert_eq!(msi.info.asset_name, "Kanso_0.3.0_x64_en-US.msi");

        // Already on 0.3.0 -> no update
        assert_eq!(
            client
                .check(AppVersion { major: 0, minor: 3, patch: 0 }, InstallerKind::Nsis)
                .unwrap(),
            None
        );
    }

    #[test]
    fn ignores_prereleases_and_drafts() {
        let base = test_http::spawn(|_| {
            (
                200,
                r#"{"tag_name":"v9.0.0","prerelease":true,"draft":false,"assets":[]}"#.to_string(),
            )
        });
        let client = UpdateClient::for_tests(&base);
        assert_eq!(
            client
                .check(AppVersion { major: 0, minor: 2, patch: 3 }, InstallerKind::Nsis)
                .unwrap(),
            None
        );
    }

    type BinaryResponse = (u16, Vec<(String, String)>, Vec<u8>);
    type BinaryRouteHandler = Arc<dyn Fn(&str) -> BinaryResponse + Send + Sync>;

    fn spawn_binary_server(routes: BinaryRouteHandler) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.is_empty() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                }
                let (status, headers, body) = routes(&path);
                let mut resp = format!(
                    "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (k, v) in headers {
                    resp.push_str(&format!("{k}: {v}\r\n"));
                }
                resp.push_str("\r\n");
                let mut writer = &stream;
                let _ = writer.write_all(resp.as_bytes());
                let _ = writer.write_all(&body);
                let _ = writer.flush();
            }
        });
        addr
    }

    fn sha256_of(bytes: &[u8]) -> String {
        let mut ctx = Sha256Context::new(&SHA256);
        ctx.update(bytes);
        hex_encode(ctx.finish().as_ref())
    }

    #[test]
    fn downloads_follows_redirect_reports_progress_and_verifies_sha256() {
        let mut payload = b"MZ".to_vec();
        payload.extend(vec![0x42u8; 140_000]);
        let expected_hash = sha256_of(&payload);
        let sums_body = format!("{expected_hash}  Kanso_0.3.0_x64-setup.exe\n").into_bytes();

        let payload_clone = payload.clone();
        let base = spawn_binary_server(Arc::new(move |path| match path {
            "/download/setup.exe" => (
                302,
                vec![("Location".into(), "/cdn/setup.exe".into())],
                Vec::new(),
            ),
            "/cdn/setup.exe" => (200, Vec::new(), payload_clone.clone()),
            "/download/SHA256SUMS.txt" => (200, Vec::new(), sums_body.clone()),
            _ => (404, Vec::new(), Vec::new()),
        }));

        let release = CheckedRelease {
            info: UpdateInfo {
                current_version: "0.2.3".into(),
                version: "0.3.0".into(),
                name: "Kanso 0.3.0".into(),
                notes: String::new(),
                published_at: None,
                release_url: RELEASES_PAGE_URL.into(),
                installer_kind: InstallerKind::Nsis,
                asset_name: "Kanso_0.3.0_x64-setup.exe".into(),
                asset_size: payload.len() as u64,
            },
            download_url: format!("{base}/download/setup.exe"),
            checksums_url: Some(format!("{base}/download/SHA256SUMS.txt")),
            asset_digest_sha256: Some(expected_hash),
        };

        let dir = tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        let client = UpdateClient::for_tests(&base);
        let path = client
            .download_and_verify(&release, dir.path(), &cancel, |p| events.push(p))
            .unwrap();

        assert_eq!(path, dir.path().join("Kanso_0.3.0_x64-setup.exe"));
        assert_eq!(fs::read(&path).unwrap(), payload);
        assert!(events.len() >= 3, "progress reported across chunks");
        assert_eq!(events.last().unwrap().downloaded, payload.len() as u64);
        assert_eq!(events.last().unwrap().total, Some(payload.len() as u64));

        // Cleanup removes old installers
        cleanup_old_installers(dir.path());
        assert!(!path.exists());
    }

    #[test]
    fn rejects_tampered_download_and_removes_partial_file() {
        let good = b"MZvalid-installer-bytes".to_vec();
        let tampered = b"MZtampered-bytes-other!".to_vec();
        let expected_hash = sha256_of(&good);

        let base = spawn_binary_server(Arc::new(move |path| match path {
            "/setup.exe" => (200, Vec::new(), tampered.clone()),
            _ => (404, Vec::new(), Vec::new()),
        }));

        let release = CheckedRelease {
            info: UpdateInfo {
                current_version: "0.2.3".into(),
                version: "0.3.0".into(),
                name: "Kanso 0.3.0".into(),
                notes: String::new(),
                published_at: None,
                release_url: RELEASES_PAGE_URL.into(),
                installer_kind: InstallerKind::Nsis,
                asset_name: "Kanso_0.3.0_x64-setup.exe".into(),
                asset_size: good.len() as u64,
            },
            download_url: format!("{base}/setup.exe"),
            checksums_url: None,
            asset_digest_sha256: Some(expected_hash),
        };

        let dir = tempdir().unwrap();
        let client = UpdateClient::for_tests(&base);
        let err = client
            .download_and_verify(&release, dir.path(), &AtomicBool::new(false), |_| {})
            .unwrap_err();
        assert!(err.contains(T::UpdateChecksumMismatch.s()), "unexpected error: {err}");
        assert!(!dir.path().join("Kanso_0.3.0_x64-setup.exe").exists());
        assert!(!dir.path().join("Kanso_0.3.0_x64-setup.exe.part").exists());
    }

    #[test]
    fn cancels_download_cleanly_when_requested() {
        let payload = b"MZ0123456789".to_vec();
        let hash = sha256_of(&payload);
        let base = spawn_binary_server(Arc::new(move |_| (200, Vec::new(), payload.clone())));
        let release = CheckedRelease {
            info: UpdateInfo {
                current_version: "0.2.3".into(),
                version: "0.3.0".into(),
                name: "Kanso 0.3.0".into(),
                notes: String::new(),
                published_at: None,
                release_url: RELEASES_PAGE_URL.into(),
                installer_kind: InstallerKind::Nsis,
                asset_name: "Kanso_0.3.0_x64-setup.exe".into(),
                asset_size: 12,
            },
            download_url: format!("{base}/setup.exe"),
            checksums_url: None,
            asset_digest_sha256: Some(hash),
        };
        let dir = tempdir().unwrap();
        let cancel = AtomicBool::new(true);
        let err = UpdateClient::for_tests(&base)
            .download_and_verify(&release, dir.path(), &cancel, |_| {})
            .unwrap_err();
        assert_eq!(err, T::UpdateCancelled.s());
    }
}
