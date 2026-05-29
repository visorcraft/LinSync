//! Qt WebEngine wrapper for LinSync webpage compare (rendered + screenshot sub-modes).
//!
//! This crate is only available when the `web-engine` feature is enabled on
//! `linsync-core`.  In the default build, this crate is not compiled and the
//! rendered/screenshot sub-modes are absent from the public API.
//!
//! # Phase 9.7-bis
//!
//! The real implementation (cxx-qt bindings to `QWebEngineView` and
//! `QWebEngineProfile`) is deferred to Phase 9.7-bis.  This stub defines the
//! public API surface and returns `Err(WebEngineError::NotImplemented)` for all
//! operations so that Tasks 9.8, 9.9, and 9.10 can compile and be wired end-to-end.

/// Errors from the web-engine wrapper.
#[derive(Debug)]
pub enum WebEngineError {
    NotImplemented,
    InitFailed(String),
    PageLoadFailed { url: String, reason: String },
    CaptureFailed(String),
}

impl std::fmt::Display for WebEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => {
                write!(
                    f,
                    "Qt WebEngine bindings not yet implemented (Phase 9.7-bis)"
                )
            }
            Self::InitFailed(s) => write!(f, "Qt WebEngine initialization failed: {s}"),
            Self::PageLoadFailed { url, reason } => {
                write!(f, "page load failed for {url}: {reason}")
            }
            Self::CaptureFailed(s) => write!(f, "screenshot capture failed: {s}"),
        }
    }
}

impl std::error::Error for WebEngineError {}

/// Options for a web-engine rendering session.
#[derive(Debug, Clone)]
pub struct WebEngineOptions {
    /// Directory for the isolated `QWebEngineProfile` storage.
    /// Typically `$XDG_CACHE_HOME/linsync/webcompare/profile/`.
    pub profile_storage_dir: std::path::PathBuf,
    /// Viewport width in logical pixels.  Default 1280.
    pub viewport_width: u32,
    /// Viewport height in logical pixels.  Default 900.
    pub viewport_height: u32,
    /// Page-load timeout in seconds.  Default 30.
    pub timeout_secs: u32,
}

impl Default for WebEngineOptions {
    fn default() -> Self {
        Self {
            profile_storage_dir: std::path::PathBuf::from("/tmp/linsync-webengine-profile"),
            viewport_width: 1280,
            viewport_height: 900,
            timeout_secs: 30,
        }
    }
}

/// Render `url` in an isolated Qt WebEngine profile and return the path to a PNG screenshot.
///
/// The PNG is written to `output_dir/<url_hash>.png`.
///
/// # Errors
///
/// Returns [`WebEngineError::NotImplemented`] in the current stub.
/// The real implementation (Phase 9.7-bis) will initialize a `QApplication` /
/// `QWebEngineView` in the calling thread, navigate to the URL, wait for
/// `loadFinished`, then call `QWebEngineView::grab()` and save the result as PNG.
pub fn render_url(
    url: &str,
    output_dir: &std::path::Path,
    options: &WebEngineOptions,
) -> Result<std::path::PathBuf, WebEngineError> {
    let _ = (url, output_dir, options);
    Err(WebEngineError::NotImplemented)
}

/// Delete all Qt WebEngine profile data under `profile_storage_dir`.
///
/// Calls `QWebEngineProfile::deleteAllCookies()` before removing the directory.
/// In this stub, simply removes the directory if it exists.
pub fn clear_profile(options: &WebEngineOptions) -> Result<(), WebEngineError> {
    let dir = &options.profile_storage_dir;
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_url_returns_not_implemented() {
        let opts = WebEngineOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let err = render_url("http://example.com/", tmp.path(), &opts).unwrap_err();
        assert!(matches!(err, WebEngineError::NotImplemented));
    }

    #[test]
    fn clear_profile_is_idempotent_when_dir_missing() {
        let opts = WebEngineOptions {
            profile_storage_dir: std::path::PathBuf::from(
                "/tmp/linsync-test-nonexistent-profile-dir",
            ),
            ..Default::default()
        };
        // Should not error even though directory doesn't exist.
        clear_profile(&opts).unwrap();
    }
}
