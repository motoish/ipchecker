use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use reqwest::{Url, blocking::Client};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const TRUSTED_RELEASE_PATH: &str = "/motoish/ipchecker/releases/download/";
const MAX_MANIFEST_SIZE: u64 = 64 * 1024;
const MAX_ARCHIVE_SIZE: u64 = 250 * 1024 * 1024;
pub const UPDATE_MANIFEST_URL: &str =
    "https://github.com/motoish/ipchecker/releases/latest/download/update.json";
pub const RELEASES_URL: &str = "https://github.com/motoish/ipchecker/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub build: u64,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRelease {
    pub version: String,
    pub build: u64,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    Current,
    Available(UpdateRelease),
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("invalid update manifest: {0}")]
    InvalidManifest(String),
    #[error("untrusted update download URL: {0}")]
    UntrustedDownloadUrl(String),
    #[error("update archive size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("update archive checksum mismatch")]
    ChecksumMismatch,
    #[error("invalid update archive layout: {0}")]
    InvalidArchiveLayout(String),
    #[error("update manifest exceeds the size limit")]
    ManifestTooLarge,
    #[error("update network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("update download redirected to an untrusted URL: {0}")]
    UntrustedRedirect(String),
    #[error("the Downloads directory is unavailable")]
    DownloadsUnavailable,
    #[error("update tool failed: {program} exited with {status}")]
    ToolFailed {
        program: &'static str,
        status: String,
    },
    #[error("update file operation failed: {0}")]
    Io(#[from] io::Error),
}

pub fn local_build() -> u64 {
    env!("IPCHECKER_BUILD_NUMBER")
        .parse()
        .expect("IPCHECKER_BUILD_NUMBER must be numeric")
}

pub fn check_for_updates() -> Result<UpdateStatus, UpdateError> {
    fetch_update_status(UPDATE_MANIFEST_URL, local_build())
}

pub fn fetch_update_status(
    manifest_url: &str,
    installed_build: u64,
) -> Result<UpdateStatus, UpdateError> {
    let client = update_client()?;
    let response = client.get(manifest_url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_SIZE)
    {
        return Err(UpdateError::ManifestTooLarge);
    }

    let mut body = Vec::new();
    response
        .take(MAX_MANIFEST_SIZE + 1)
        .read_to_end(&mut body)?;
    if body.len() as u64 > MAX_MANIFEST_SIZE {
        return Err(UpdateError::ManifestTooLarge);
    }
    let json = std::str::from_utf8(&body)
        .map_err(|error| UpdateError::InvalidManifest(error.to_string()))?;
    Ok(classify_update(installed_build, parse_manifest(json)?))
}

pub fn download_and_extract(release: &UpdateRelease) -> Result<PathBuf, UpdateError> {
    if release.size == 0 || release.size > MAX_ARCHIVE_SIZE {
        return Err(UpdateError::InvalidManifest(
            "archive size is outside the allowed range".to_owned(),
        ));
    }
    validate_download_url(&release.url)?;

    let client = update_client()?;
    let response = client.get(&release.url).send()?.error_for_status()?;
    if !is_trusted_asset_response_url(response.url().as_str()) {
        return Err(UpdateError::UntrustedRedirect(response.url().to_string()));
    }
    if let Some(length) = response.content_length()
        && length != release.size
    {
        return Err(UpdateError::SizeMismatch {
            expected: release.size,
            actual: length,
        });
    }

    let temporary = tempfile::tempdir()?;
    let archive = temporary.path().join("ipchecker.zip");
    let mut file = File::create(&archive)?;
    let copied = io::copy(&mut response.take(release.size + 1), &mut file)?;
    file.flush()?;
    if copied != release.size {
        return Err(UpdateError::SizeMismatch {
            expected: release.size,
            actual: copied,
        });
    }
    verify_archive(&archive, release.size, &release.sha256)?;

    let downloads = dirs::download_dir().ok_or(UpdateError::DownloadsUnavailable)?;
    let destination = next_download_directory(&downloads, &release.version);
    fs::create_dir(&destination)?;
    let extraction_result =
        extract_archive(&archive, &destination).and_then(|()| validate_extracted_app(&destination));
    match extraction_result {
        Ok(app) => Ok(app),
        Err(error) => {
            if let Err(cleanup_error) = fs::remove_dir_all(&destination) {
                log::warn!(
                    "failed to clean update directory {}: {cleanup_error}",
                    destination.display()
                );
            }
            Err(error)
        }
    }
}

pub fn reveal_in_finder(app: &Path) -> Result<(), UpdateError> {
    let status = Command::new("/usr/bin/open").arg("-R").arg(app).status()?;
    if !status.success() {
        return Err(UpdateError::ToolFailed {
            program: "open",
            status: status.to_string(),
        });
    }
    Ok(())
}

pub fn open_releases_page() -> Result<(), UpdateError> {
    let status = Command::new("/usr/bin/open").arg(RELEASES_URL).status()?;
    if !status.success() {
        return Err(UpdateError::ToolFailed {
            program: "open",
            status: status.to_string(),
        });
    }
    Ok(())
}

pub fn parse_manifest(json: &str) -> Result<UpdateManifest, UpdateError> {
    let mut manifest: UpdateManifest = serde_json::from_str(json)
        .map_err(|error| UpdateError::InvalidManifest(error.to_string()))?;

    if manifest.version.is_empty()
        || !manifest
            .version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return Err(UpdateError::InvalidManifest(
            "version must contain only ASCII letters, digits, dots, and hyphens".to_owned(),
        ));
    }
    if manifest.build == 0 {
        return Err(UpdateError::InvalidManifest(
            "build must be greater than zero".to_owned(),
        ));
    }
    if manifest.size == 0 || manifest.size > MAX_ARCHIVE_SIZE {
        return Err(UpdateError::InvalidManifest(format!(
            "archive size must be between 1 and {MAX_ARCHIVE_SIZE} bytes"
        )));
    }
    if manifest.sha256.len() != 64 || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(UpdateError::InvalidManifest(
            "sha256 must be 64 hexadecimal characters".to_owned(),
        ));
    }
    manifest.sha256.make_ascii_lowercase();
    validate_download_url(&manifest.url)?;

    Ok(manifest)
}

pub fn classify_update(local_build: u64, manifest: UpdateManifest) -> UpdateStatus {
    if manifest.build <= local_build {
        return UpdateStatus::Current;
    }

    UpdateStatus::Available(UpdateRelease {
        version: manifest.version,
        build: manifest.build,
        url: manifest.url,
        size: manifest.size,
        sha256: manifest.sha256,
    })
}

pub fn verify_archive(
    archive: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateError> {
    let actual_size = archive.metadata()?.len();
    if actual_size != expected_size {
        return Err(UpdateError::SizeMismatch {
            expected: expected_size,
            actual: actual_size,
        });
    }

    let mut file = File::open(archive)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(UpdateError::ChecksumMismatch);
    }

    Ok(())
}

pub fn next_download_directory(downloads: &Path, version: &str) -> PathBuf {
    let base = downloads.join(format!("ipchecker-{version}"));
    if !base.exists() {
        return base;
    }

    for suffix in 2u64.. {
        let candidate = downloads.join(format!("ipchecker-{version}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u64 destination suffixes exhausted")
}

pub fn validate_extracted_app(extraction: &Path) -> Result<PathBuf, UpdateError> {
    let entries = extraction.read_dir()?.collect::<Result<Vec<_>, _>>()?;
    if entries.len() != 1 || entries[0].file_name() != "ipchecker.app" {
        return Err(UpdateError::InvalidArchiveLayout(
            "archive must contain only ipchecker.app".to_owned(),
        ));
    }

    let app = entries[0].path();
    let app_metadata = app.symlink_metadata()?;
    if !app_metadata.is_dir() || app_metadata.file_type().is_symlink() {
        return Err(UpdateError::InvalidArchiveLayout(
            "ipchecker.app must be a directory".to_owned(),
        ));
    }

    let canonical_app = app.canonicalize()?;
    validate_tree(&app, &canonical_app)?;
    Ok(app)
}

fn validate_tree(path: &Path, canonical_app: &Path) -> Result<(), UpdateError> {
    for entry in path.read_dir()? {
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        if metadata.file_type().is_symlink() {
            let target = entry.path().canonicalize().map_err(|error| {
                UpdateError::InvalidArchiveLayout(format!(
                    "unresolvable symlink {}: {error}",
                    entry.path().display()
                ))
            })?;
            if !target.starts_with(canonical_app) {
                return Err(UpdateError::InvalidArchiveLayout(format!(
                    "symlink escapes ipchecker.app: {}",
                    entry.path().display()
                )));
            }
        } else if metadata.is_dir() {
            validate_tree(&entry.path(), canonical_app)?;
        }
    }
    Ok(())
}

fn validate_download_url(value: &str) -> Result<(), UpdateError> {
    let url = Url::parse(value)
        .map_err(|error| UpdateError::InvalidManifest(format!("invalid URL: {error}")))?;
    let trusted = url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.path().starts_with(TRUSTED_RELEASE_PATH)
        && url.query().is_none()
        && url.fragment().is_none();
    if !trusted {
        return Err(UpdateError::UntrustedDownloadUrl(value.to_owned()));
    }
    Ok(())
}

pub fn is_trusted_asset_response_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    matches!(
        url.host_str(),
        Some(
            "github.com"
                | "release-assets.githubusercontent.com"
                | "objects.githubusercontent.com"
                | "github-releases.githubusercontent.com"
        )
    )
}

fn extract_archive(archive: &Path, destination: &Path) -> Result<(), UpdateError> {
    let status = Command::new("/usr/bin/ditto")
        .arg("-x")
        .arg("-k")
        .arg(archive)
        .arg(destination)
        .status()?;
    if !status.success() {
        return Err(UpdateError::ToolFailed {
            program: "ditto",
            status: status.to_string(),
        });
    }
    Ok(())
}

fn update_client() -> Result<Client, UpdateError> {
    Ok(Client::builder()
        .user_agent(concat!("ipchecker/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}
