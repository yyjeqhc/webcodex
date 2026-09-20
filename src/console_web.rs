//! Public static shells for the hosted Runtime Console and Admin UI.
//!
//! The bundles contain no credentials; each browser authenticates separately
//! for protected API requests. Production embeds committed `frontend/dist/`
//! assets. Development mode validates the Runtime Console core files once at
//! startup and re-reads fixed Runtime/Admin assets from a loopback-only directory.

use salvo::http::header::{CACHE_CONTROL, CONTENT_TYPE, PRAGMA};
use salvo::http::{HeaderName, HeaderValue};
use salvo::prelude::*;
use std::fmt;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const CONSOLE_ASSETS_DIR_ENV: &str = "WEBCODEX_CONSOLE_ASSETS_DIR";

const ASSET_MODE_HEADER: HeaderName = HeaderName::from_static("x-webcodex-console-assets");
const EMBEDDED_CACHE_CONTROL: &str = "no-cache, must-revalidate";
const DEVELOPMENT_CACHE_CONTROL: &str = "no-store";
const DEVELOPMENT_ERROR_BODY: &str = "Console development asset is unavailable.\n";

// The committed build stays embedded so production has no runtime filesystem
// dependency.
const ADMIN_HTML: &str = include_str!("../frontend/dist/admin.html");
const ADMIN_APP_JS: &str = include_str!("../frontend/dist/admin.js");
const ADMIN_STYLES_CSS: &str = include_str!("../frontend/dist/admin.css");
const RUNTIME_HTML: &str = include_str!("../frontend/dist/runtime.html");
const RUNTIME_APP_JS: &str = include_str!("../frontend/dist/app.js");
const RUNTIME_STYLES_CSS: &str = include_str!("../frontend/dist/styles.css");

#[derive(Debug, Clone, Copy)]
enum ConsoleAsset {
    AdminHtml,
    AdminJavaScript,
    AdminCss,
    RuntimeHtml,
    RuntimeJavaScript,
    RuntimeCss,
}

impl ConsoleAsset {
    const fn file_name(self) -> &'static str {
        match self {
            Self::AdminHtml => "admin.html",
            Self::AdminJavaScript => "admin.js",
            Self::AdminCss => "admin.css",
            Self::RuntimeHtml => "runtime.html",
            Self::RuntimeJavaScript => "app.js",
            Self::RuntimeCss => "styles.css",
        }
    }

    const fn content_type(self) -> &'static str {
        match self {
            Self::AdminCss | Self::RuntimeCss => "text/css; charset=utf-8",
            Self::AdminHtml | Self::RuntimeHtml => "text/html; charset=utf-8",
            Self::AdminJavaScript | Self::RuntimeJavaScript => {
                "application/javascript; charset=utf-8"
            }
        }
    }

    const fn embedded(self) -> &'static str {
        match self {
            Self::AdminHtml => ADMIN_HTML,
            Self::AdminJavaScript => ADMIN_APP_JS,
            Self::AdminCss => ADMIN_STYLES_CSS,
            Self::RuntimeHtml => RUNTIME_HTML,
            Self::RuntimeJavaScript => RUNTIME_APP_JS,
            Self::RuntimeCss => RUNTIME_STYLES_CSS,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConsoleAssetDirectory {
    canonical_root: PathBuf,
}

/// The single startup-resolved source shared by all three console handlers.
#[derive(Debug, Clone, Default)]
pub(crate) enum ConsoleAssetSource {
    #[default]
    Embedded,
    Directory(ConsoleAssetDirectory),
}

#[derive(Debug)]
pub(crate) struct ConsoleAssetConfigError {
    message: String,
}

impl ConsoleAssetConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConsoleAssetConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConsoleAssetConfigError {}

impl ConsoleAssetSource {
    /// Resolve the private serve-process environment once during startup.
    pub(crate) fn from_env(bind_addr: &str) -> Result<Self, ConsoleAssetConfigError> {
        let Some(directory) = std::env::var_os(CONSOLE_ASSETS_DIR_ENV) else {
            return Ok(Self::Embedded);
        };
        Self::from_directory_for_addr(PathBuf::from(directory), bind_addr)
    }

    /// Validate a development asset source for a specific HTTP bind address.
    pub(crate) fn from_directory_for_addr(
        directory: impl AsRef<Path>,
        bind_addr: &str,
    ) -> Result<Self, ConsoleAssetConfigError> {
        validate_loopback_addr(bind_addr)?;
        Self::from_directory(directory)
    }

    /// Canonicalize the development root and require the three Runtime Console
    /// core files. Admin assets remain optional and fail closed when requested.
    pub(crate) fn from_directory(
        directory: impl AsRef<Path>,
    ) -> Result<Self, ConsoleAssetConfigError> {
        let directory = directory.as_ref();
        if !directory.is_absolute() {
            return Err(ConsoleAssetConfigError::new(
                "console development assets directory must be an absolute path",
            ));
        }
        let canonical_root = fs::canonicalize(directory).map_err(|_| {
            ConsoleAssetConfigError::new(
                "console development assets directory does not exist or is inaccessible",
            )
        })?;
        if !fs::metadata(&canonical_root)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            return Err(ConsoleAssetConfigError::new(
                "console development assets path is not a directory",
            ));
        }

        let source = Self::Directory(ConsoleAssetDirectory { canonical_root });
        for asset in [
            ConsoleAsset::RuntimeHtml,
            ConsoleAsset::RuntimeJavaScript,
            ConsoleAsset::RuntimeCss,
        ] {
            let path = source.validated_path(asset)?;
            fs::File::open(path).map_err(|_| {
                ConsoleAssetConfigError::new(format!(
                    "console development asset {} is not readable",
                    asset.file_name()
                ))
            })?;
        }
        Ok(source)
    }

    pub(crate) fn mode_label(&self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Directory(_) => "local development",
        }
    }

    pub(crate) fn directory(&self) -> Option<&Path> {
        match self {
            Self::Embedded => None,
            Self::Directory(directory) => Some(&directory.canonical_root),
        }
    }

    fn diagnostic_label(&self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Directory(_) => "filesystem",
        }
    }

    fn is_development(&self) -> bool {
        matches!(self, Self::Directory(_))
    }

    fn validated_path(&self, asset: ConsoleAsset) -> Result<PathBuf, ConsoleAssetConfigError> {
        let Self::Directory(directory) = self else {
            return Err(ConsoleAssetConfigError::new(
                "embedded console assets do not have filesystem paths",
            ));
        };
        let candidate = directory.canonical_root.join(asset.file_name());

        // Files are fixed by the server, never by URL input. Re-checking the
        // symlink and canonical-path constraints rejects persistent path
        // substitution. A process with concurrent write access to this directory
        // is already trusted to control the development bundle itself.
        let link_metadata = fs::symlink_metadata(&candidate).map_err(|_| {
            ConsoleAssetConfigError::new(format!(
                "console development asset {} is unavailable",
                asset.file_name()
            ))
        })?;
        if link_metadata.file_type().is_symlink() {
            return Err(ConsoleAssetConfigError::new(format!(
                "console development asset {} must not be a symbolic link",
                asset.file_name()
            )));
        }
        let canonical = fs::canonicalize(&candidate).map_err(|_| {
            ConsoleAssetConfigError::new(format!(
                "console development asset {} is unavailable",
                asset.file_name()
            ))
        })?;
        if !canonical.starts_with(&directory.canonical_root) {
            return Err(ConsoleAssetConfigError::new(format!(
                "console development asset {} escapes its configured directory",
                asset.file_name()
            )));
        }
        if !fs::metadata(&canonical)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
        {
            return Err(ConsoleAssetConfigError::new(format!(
                "console development asset {} is not a regular file",
                asset.file_name()
            )));
        }
        Ok(canonical)
    }

    async fn read(&self, asset: ConsoleAsset) -> Result<String, ConsoleAssetConfigError> {
        match self {
            Self::Embedded => Ok(asset.embedded().to_string()),
            Self::Directory(_) => {
                let path = self.validated_path(asset)?;
                tokio::fs::read_to_string(path).await.map_err(|_| {
                    ConsoleAssetConfigError::new(format!(
                        "console development asset {} could not be read",
                        asset.file_name()
                    ))
                })
            }
        }
    }
}

fn validate_loopback_addr(bind_addr: &str) -> Result<(), ConsoleAssetConfigError> {
    let is_loopback = bind_addr
        .parse::<SocketAddr>()
        .map(|address| address.ip().is_loopback())
        .unwrap_or_else(|_| {
            bind_addr
                .strip_prefix("localhost:")
                .and_then(|port| port.parse::<u16>().ok())
                .is_some()
        });
    if is_loopback {
        Ok(())
    } else {
        Err(ConsoleAssetConfigError::new(format!(
            "{CONSOLE_ASSETS_DIR_ENV} requires WEBCODEX_ADDR to use a loopback address"
        )))
    }
}

fn apply_asset_headers(res: &mut Response, source: &ConsoleAssetSource, asset: ConsoleAsset) {
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(asset.content_type()));
    res.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static(if source.is_development() {
            DEVELOPMENT_CACHE_CONTROL
        } else {
            EMBEDDED_CACHE_CONTROL
        }),
    );
    if source.is_development() {
        res.headers_mut()
            .insert(PRAGMA, HeaderValue::from_static("no-cache"));
    }
    res.headers_mut().insert(
        ASSET_MODE_HEADER,
        HeaderValue::from_static(source.diagnostic_label()),
    );
}

async fn serve_asset(depot: &Depot, res: &mut Response, asset: ConsoleAsset) {
    let Ok(source) = depot.obtain::<Arc<ConsoleAssetSource>>() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Text::Plain("Console asset source is unavailable.\n"));
        return;
    };
    apply_asset_headers(res, source, asset);
    match source.read(asset).await {
        Ok(body) => res.render(Text::Plain(body)),
        Err(error) => {
            tracing::error!(
                asset = asset.file_name(),
                error = %error,
                "Console development asset request failed"
            );
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Text::Plain(DEVELOPMENT_ERROR_BODY));
        }
    }
}

/// Public hosted Runtime Console shell; all data stays behind
/// `/api/runtime-console/*` and ordinary runtime authorization.
#[handler]
pub async fn runtime_html(depot: &Depot, res: &mut Response) {
    serve_asset(depot, res, ConsoleAsset::RuntimeHtml).await;
}

#[handler]
pub async fn runtime_app_js(depot: &Depot, res: &mut Response) {
    serve_asset(depot, res, ConsoleAsset::RuntimeJavaScript).await;
}

#[handler]
pub async fn runtime_styles_css(depot: &Depot, res: &mut Response) {
    serve_asset(depot, res, ConsoleAsset::RuntimeCss).await;
}

/// Public admin shell; all data remains protected by `/api/admin/*`.
#[handler]
pub async fn admin_html(depot: &Depot, res: &mut Response) {
    serve_asset(depot, res, ConsoleAsset::AdminHtml).await;
}

#[handler]
pub async fn admin_app_js(depot: &Depot, res: &mut Response) {
    serve_asset(depot, res, ConsoleAsset::AdminJavaScript).await;
}

#[handler]
pub async fn admin_styles_css(depot: &Depot, res: &mut Response) {
    serve_asset(depot, res, ConsoleAsset::AdminCss).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    fn build_test_router(source: Arc<ConsoleAssetSource>) -> Router {
        Router::new()
            .hoop(affix_state::inject(source))
            .push(Router::with_path("runtime").get(runtime_html))
            .push(Router::with_path("runtime/app.js").get(runtime_app_js))
            .push(Router::with_path("runtime/styles.css").get(runtime_styles_css))
            .push(Router::with_path("admin").get(admin_html))
            .push(Router::with_path("admin/app.js").get(admin_app_js))
            .push(Router::with_path("admin/styles.css").get(admin_styles_css))
    }

    fn embedded_service() -> Service {
        Service::new(build_test_router(Arc::new(ConsoleAssetSource::Embedded)))
    }

    fn header(resp: &Response, name: &str) -> String {
        resp.headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    fn write_runtime_development_assets(directory: &Path) {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join("runtime.html"), "<html>runtime one</html>\n").unwrap();
        fs::write(directory.join("app.js"), "globalThis.runtime = 1;\n").unwrap();
        fs::write(directory.join("styles.css"), ".runtime { color: red; }\n").unwrap();
    }

    fn development_service(directory: &Path) -> Service {
        let source =
            ConsoleAssetSource::from_directory_for_addr(directory, "127.0.0.1:8080").unwrap();
        Service::new(build_test_router(Arc::new(source)))
    }

    #[test]
    fn embedded_bundle_contains_runtime_and_admin_only() {
        assert!(RUNTIME_HTML.contains("WebCodex — Runtime Workspace"));
        assert!(RUNTIME_HTML.contains("/runtime/app.js"));
        assert!(RUNTIME_HTML.contains("/runtime/styles.css"));
        assert!(RUNTIME_APP_JS.contains("/api/runtime-console/"));
        assert!(!RUNTIME_APP_JS.contains("/api/console/"));
        assert!(!RUNTIME_APP_JS.contains("document.cookie"));
        assert!(!RUNTIME_HTML.contains("prototype-v2"));
        assert!(!ADMIN_HTML.is_empty());
        assert!(!ADMIN_APP_JS.contains("/api/console/"));
    }

    #[tokio::test]
    async fn embedded_runtime_and_admin_assets_preserve_bodies_and_headers() {
        let service = embedded_service();
        for (url, expected, mime) in [
            ("http://localhost/runtime", RUNTIME_HTML, "text/html"),
            (
                "http://localhost/runtime/app.js",
                RUNTIME_APP_JS,
                "application/javascript",
            ),
            (
                "http://localhost/runtime/styles.css",
                RUNTIME_STYLES_CSS,
                "text/css",
            ),
            ("http://localhost/admin", ADMIN_HTML, "text/html"),
            (
                "http://localhost/admin/app.js",
                ADMIN_APP_JS,
                "application/javascript",
            ),
            (
                "http://localhost/admin/styles.css",
                ADMIN_STYLES_CSS,
                "text/css",
            ),
        ] {
            let mut response = TestClient::get(url).send(&service).await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert!(header(&response, "content-type").contains(mime));
            assert_eq!(
                header(&response, "cache-control"),
                "no-cache, must-revalidate"
            );
            assert_eq!(response.take_string().await.unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn development_runtime_assets_are_reread_and_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        write_runtime_development_assets(temp.path());
        let service = development_service(temp.path());
        let mut response = TestClient::get("http://localhost/runtime/app.js")
            .send(&service)
            .await;
        assert_eq!(
            response.take_string().await.unwrap(),
            "globalThis.runtime = 1;\n"
        );
        fs::write(temp.path().join("app.js"), "globalThis.runtime = 2;\n").unwrap();
        let mut response = TestClient::get("http://localhost/runtime/app.js")
            .send(&service)
            .await;
        assert_eq!(
            response.take_string().await.unwrap(),
            "globalThis.runtime = 2;\n"
        );
        fs::remove_file(temp.path().join("app.js")).unwrap();
        let mut response = TestClient::get("http://localhost/runtime/app.js")
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert_eq!(header(&response, "cache-control"), "no-store");
        assert_eq!(
            response.take_string().await.unwrap(),
            DEVELOPMENT_ERROR_BODY
        );
    }

    #[test]
    fn development_directory_requires_runtime_core_and_loopback() {
        let temp = tempfile::tempdir().unwrap();
        let missing = ConsoleAssetSource::from_directory(temp.path()).unwrap_err();
        assert!(missing.to_string().contains("runtime.html"));
        write_runtime_development_assets(temp.path());
        for allowed in ["127.0.0.1:8080", "[::1]:8080", "localhost:8080"] {
            ConsoleAssetSource::from_directory_for_addr(temp.path(), allowed).unwrap();
        }
        let error =
            ConsoleAssetSource::from_directory_for_addr(temp.path(), "0.0.0.0:8080").unwrap_err();
        assert!(error.to_string().contains("loopback"));
    }
}
