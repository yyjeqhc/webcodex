use super::{executable_name, ProductError};
use reqwest::header::USER_AGENT;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const CLOUDFLARED_VERSION: &str = "2026.7.3";
const CLOUDFLARED_RELEASE_BASE: &str =
    "https://github.com/cloudflare/cloudflared/releases/download/2026.7.3";
const CLOUDFLARED_MAX_DOWNLOAD_BYTES: usize = 64 * 1024 * 1024;
const CLOUDFLARED_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const CLOUDFLARED_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const CLOUDFLARED_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CloudflaredAsset {
    target: &'static str,
    file_name: &'static str,
    archive_sha256: &'static str,
    binary_sha256: &'static str,
    gzip_archive: bool,
}

pub(super) async fn resolve_cloudflared() -> Result<PathBuf, ProductError> {
    let override_bin = std::env::var_os("WEBCODEX_CLOUDFLARED_BIN").map(PathBuf::from);
    if let Some(binary) =
        existing_cloudflared_from(override_bin.as_deref(), std::env::var_os("PATH").as_deref())?
    {
        return Ok(binary);
    }

    let asset = cloudflared_asset_for(std::env::consts::OS, std::env::consts::ARCH)?;
    let root = managed_cloudflared_root()?;
    let url = format!("{CLOUDFLARED_RELEASE_BASE}/{}", asset.file_name);
    ensure_managed_cloudflared_at(&root, asset, &url).await
}

fn existing_cloudflared_from(
    override_bin: Option<&Path>,
    path: Option<&OsStr>,
) -> Result<Option<PathBuf>, ProductError> {
    if let Some(binary) = override_bin {
        if binary.is_file() {
            return Ok(Some(binary.to_path_buf()));
        }
        return Err(ProductError::new(
            "tunnel_unavailable",
            "WEBCODEX_CLOUDFLARED_BIN does not point to a cloudflared file",
            Some("Fix or unset WEBCODEX_CLOUDFLARED_BIN, then retry webcodex share."),
        ));
    }
    let Some(path) = path else {
        return Ok(None);
    };
    Ok(std::env::split_paths(path)
        .map(|directory| directory.join(executable_name("cloudflared")))
        .find(|candidate| candidate.is_file()))
}

fn cloudflared_asset_for(os: &str, arch: &str) -> Result<CloudflaredAsset, ProductError> {
    let asset = match (os, arch) {
        ("linux", "x86_64") => CloudflaredAsset {
            target: "linux-amd64",
            file_name: "cloudflared-linux-amd64",
            archive_sha256: "9d71c677db00134c1bd4144b7783486b654ad281b1ea62b4972098d19f770f17",
            binary_sha256: "9d71c677db00134c1bd4144b7783486b654ad281b1ea62b4972098d19f770f17",
            gzip_archive: false,
        },
        ("linux", "aarch64") => CloudflaredAsset {
            target: "linux-arm64",
            file_name: "cloudflared-linux-arm64",
            archive_sha256: "65259e652a7bea08bf5df603233ab22b8bf3116af8df9f9206209af6a1b955c0",
            binary_sha256: "65259e652a7bea08bf5df603233ab22b8bf3116af8df9f9206209af6a1b955c0",
            gzip_archive: false,
        },
        ("macos", "x86_64") => CloudflaredAsset {
            target: "darwin-amd64",
            file_name: "cloudflared-darwin-amd64.tgz",
            archive_sha256: "70d1c8684fa6d14b5843787ec8d1ea8e18b23650e424f4ea43d849a506487c3b",
            binary_sha256: "e88fe5874d42a94f49a7ea59cabc3722d2962d0449232b0f3b1a426a712e275c",
            gzip_archive: true,
        },
        ("macos", "aarch64") => CloudflaredAsset {
            target: "darwin-arm64",
            file_name: "cloudflared-darwin-arm64.tgz",
            archive_sha256: "90c5a4f914d705fd70c135dba6d80b1791d254b08d6d4136301941f88330dd09",
            binary_sha256: "f35c50089cd25f77a4cb5a2152036bc26db15aa31fbe11f7995d2e42a4ed6257",
            gzip_archive: true,
        },
        _ => {
            return Err(ProductError::new(
                "tunnel_unavailable",
                format!("automatic cloudflared installation is unsupported on {os}/{arch}"),
                Some("Install cloudflared and set WEBCODEX_CLOUDFLARED_BIN, or use webcodex share --tunnel none."),
            ))
        }
    };
    Ok(asset)
}

fn managed_cloudflared_root() -> Result<PathBuf, ProductError> {
    managed_cloudflared_root_from(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

fn managed_cloudflared_root_from(
    xdg_state_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, ProductError> {
    if let Some(path) = xdg_state_home.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(managed_user_root_error("XDG_STATE_HOME"));
        }
        return Ok(path.join("webcodex/tools/cloudflared"));
    }
    if let Some(path) = home.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(managed_user_root_error("HOME"));
        }
        return Ok(path.join(".local/state/webcodex/tools/cloudflared"));
    }
    Err(ProductError::new(
        "tunnel_unavailable",
        "WebCodex cannot choose a private user directory for managed cloudflared",
        Some("Set HOME or XDG_STATE_HOME, set WEBCODEX_CLOUDFLARED_BIN, or use webcodex share --tunnel none."),
    ))
}

fn managed_binary_path(root: &Path, asset: CloudflaredAsset) -> PathBuf {
    root.join(CLOUDFLARED_VERSION)
        .join(asset.target)
        .join(executable_name("cloudflared"))
}

async fn ensure_managed_cloudflared_at(
    root: &Path,
    asset: CloudflaredAsset,
    url: &str,
) -> Result<PathBuf, ProductError> {
    let destination = managed_binary_path(root, asset);
    if managed_binary_is_valid(&destination, asset).await {
        return Ok(destination);
    }

    eprintln!(
        "WebCodex: cloudflared was not found; downloading verified Cloudflare Tunnel {CLOUDFLARED_VERSION}..."
    );
    let install_dir = destination.parent().ok_or_else(managed_tool_path_error)?;
    create_private_tool_dir(install_dir)?;
    let temporary = install_dir.join(format!(".install-{}", uuid::Uuid::new_v4().simple()));
    create_private_tool_dir(&temporary)?;

    let result = async {
        let downloaded = temporary.join(asset.file_name);
        download_cloudflared_asset(url, &downloaded).await?;
        verify_sha256(&downloaded, asset.archive_sha256, "downloaded cloudflared artifact")?;

        let candidate = if asset.gzip_archive {
            let extracted = temporary.join("extract");
            create_private_tool_dir(&extracted)?;
            extract_cloudflared_archive(&downloaded, &extracted).await?;
            extracted.join(executable_name("cloudflared"))
        } else {
            downloaded
        };

        verify_sha256(&candidate, asset.binary_sha256, "downloaded cloudflared binary")?;
        make_private_executable(&candidate)?;
        verify_cloudflared_version(&candidate).await?;
        fs::rename(&candidate, &destination).map_err(|_| {
            ProductError::new(
                "tunnel_unavailable",
                "WebCodex could not install its managed cloudflared binary atomically",
                Some("Check user-state filesystem permissions, then retry webcodex share."),
            )
        })?;
        if !managed_binary_is_valid(&destination, asset).await {
            return Err(ProductError::new(
                "tunnel_unavailable",
                "the installed cloudflared binary failed post-install verification",
                Some("Remove the managed cloudflared file and retry webcodex share, or set WEBCODEX_CLOUDFLARED_BIN."),
            ));
        }
        Ok(destination.clone())
    }
    .await;

    let _ = fs::remove_dir_all(&temporary);
    result
}

async fn managed_binary_is_valid(path: &Path, asset: CloudflaredAsset) -> bool {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    if !metadata.file_type().is_file() {
        return false;
    }
    if sha256_file(path).ok().as_deref() != Some(asset.binary_sha256) {
        return false;
    }
    verify_cloudflared_version(path).await.is_ok()
}

fn create_private_tool_dir(path: &Path) -> Result<(), ProductError> {
    fs::create_dir_all(path).map_err(|_| {
        ProductError::new(
            "tunnel_unavailable",
            "WebCodex could not create its managed cloudflared directory",
            Some("Check user-state filesystem permissions, then retry webcodex share."),
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| {
            ProductError::new(
                "tunnel_unavailable",
                "WebCodex could not protect its managed cloudflared directory",
                Some("Check user-state filesystem permissions, then retry webcodex share."),
            )
        })?;
    }
    Ok(())
}

async fn download_cloudflared_asset(url: &str, destination: &Path) -> Result<(), ProductError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CLOUDFLARED_CONNECT_TIMEOUT)
        .timeout(CLOUDFLARED_DOWNLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.stop();
            }
            if attempt.previous().iter().any(|url| url.scheme() == "https")
                && attempt.url().scheme() != "https"
            {
                return attempt.stop();
            }
            attempt.follow()
        }))
        .build()
        .map_err(|_| download_error("could not initialize the download client"))?;
    let mut response = client
        .get(url)
        .header(
            USER_AGENT,
            format!("webcodex/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|error| download_error(download_request_failure(&error)))?;
    if !response.status().is_success() {
        return Err(download_error(&format!(
            "server returned HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > CLOUDFLARED_MAX_DOWNLOAD_BYTES as u64)
    {
        return Err(download_error("artifact exceeds the download size limit"));
    }

    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or(0)
            .min(CLOUDFLARED_MAX_DOWNLOAD_BYTES as u64) as usize,
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| download_error("response body failed"))?
    {
        if bytes.len().saturating_add(chunk.len()) > CLOUDFLARED_MAX_DOWNLOAD_BYTES {
            return Err(download_error("artifact exceeds the download size limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    write_private_file(destination, &bytes)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), ProductError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| {
        ProductError::new(
            "tunnel_unavailable",
            "WebCodex could not create the temporary cloudflared download",
            Some("Check user-state filesystem permissions, then retry webcodex share."),
        )
    })?;
    file.write_all(bytes).map_err(|_| {
        ProductError::new(
            "tunnel_unavailable",
            "WebCodex could not write the temporary cloudflared download",
            Some("Check user-state filesystem permissions, then retry webcodex share."),
        )
    })
}

async fn extract_cloudflared_archive(archive: &Path, directory: &Path) -> Result<(), ProductError> {
    let output = tokio::time::timeout(
        CLOUDFLARED_VERIFY_TIMEOUT,
        Command::new("tar")
            .arg("-xzf")
            .arg(archive)
            .arg("-C")
            .arg(directory)
            .output(),
    )
    .await
    .map_err(|_| extraction_error("tar timed out"))?
    .map_err(|_| extraction_error("tar is unavailable"))?;
    if !output.status.success() {
        return Err(extraction_error(
            "tar could not extract the verified archive",
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, ProductError> {
    let mut file = File::open(path).map_err(|_| verification_error())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| verification_error())?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_sha256(path: &Path, expected: &str, label: &str) -> Result<(), ProductError> {
    let actual = sha256_file(path)?;
    if actual != expected {
        return Err(ProductError::new(
            "tunnel_unavailable",
            format!("{label} failed SHA-256 verification"),
            Some("Retry webcodex share; if the failure persists, set WEBCODEX_CLOUDFLARED_BIN to a trusted cloudflared binary."),
        ));
    }
    Ok(())
}

fn make_private_executable(path: &Path) -> Result<(), ProductError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| {
            ProductError::new(
                "tunnel_unavailable",
                "WebCodex could not make managed cloudflared executable",
                Some("Check user-state filesystem permissions, then retry webcodex share."),
            )
        })?;
    }
    Ok(())
}

async fn verify_cloudflared_version(path: &Path) -> Result<(), ProductError> {
    let output = tokio::time::timeout(
        CLOUDFLARED_VERIFY_TIMEOUT,
        Command::new(path).arg("--version").output(),
    )
    .await
    .map_err(|_| verification_error())?
    .map_err(|_| verification_error())?;
    let version_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() || !version_text.contains(CLOUDFLARED_VERSION) {
        return Err(verification_error());
    }
    Ok(())
}

fn managed_user_root_error(name: &str) -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        format!("{name} must be an absolute path for managed cloudflared"),
        Some("Fix the user-state environment, set WEBCODEX_CLOUDFLARED_BIN, or use webcodex share --tunnel none."),
    )
}

fn managed_tool_path_error() -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        "WebCodex could not resolve its managed cloudflared path",
        Some("Set WEBCODEX_CLOUDFLARED_BIN or use webcodex share --tunnel none."),
    )
}

fn download_request_failure(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "request timed out"
    } else if error.is_connect() {
        "connection failed"
    } else {
        "request failed"
    }
}

fn download_error(detail: &str) -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        format!("WebCodex could not download verified cloudflared: {detail}"),
        Some("Check network/proxy connectivity and retry webcodex share, set WEBCODEX_CLOUDFLARED_BIN, or use --tunnel none."),
    )
}

fn extraction_error(detail: &str) -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        format!("WebCodex could not unpack verified cloudflared: {detail}"),
        Some("Ensure the system tar command is available, then retry webcodex share; or set WEBCODEX_CLOUDFLARED_BIN."),
    )
}

fn verification_error() -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        "managed cloudflared failed integrity or version verification",
        Some("Retry webcodex share, or set WEBCODEX_CLOUDFLARED_BIN to a trusted cloudflared binary."),
    )
}

#[cfg(test)]
#[path = "project_entry_cloudflared_tests.rs"]
mod tests;
