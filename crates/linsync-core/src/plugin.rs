use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::AppPaths;

pub const PLUGIN_MANIFEST_FILE: &str = "linsync-plugin.json";
pub const CURRENT_PLUGIN_SCHEMA_VERSION: u32 = 1;
pub const CURRENT_PLUGIN_PROTOCOL_VERSION: u32 = 1;

fn current_plugin_schema_version() -> u32 {
    CURRENT_PLUGIN_SCHEMA_VERSION
}

fn current_plugin_protocol_version() -> u32 {
    CURRENT_PLUGIN_PROTOCOL_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    #[serde(default = "current_plugin_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: String,
    pub entry: Vec<String>,
    pub classes: Vec<PluginClass>,
    pub mime_types: Vec<String>,
    pub extensions: Vec<String>,
    pub capabilities: Vec<String>,
    pub deterministic: bool,
    pub sandbox: PluginSandbox,
    /// When `true` the plugin emits a length-prefixed chunk stream instead of a
    /// single JSON response.  See [`run_streaming_plugin`].
    #[serde(default)]
    pub streaming: bool,
    /// Declarative UI schema for per-plugin options.  When non-empty the GUI
    /// renders a settings panel for this plugin.
    #[serde(default)]
    pub options_schema: Vec<PluginOption>,
}

/// A single configurable option declared by a plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOption {
    /// Machine-readable key used when reading/writing the value.
    pub key: String,
    /// Human-readable label shown in the UI.
    pub label: String,
    /// Control kind that determines which widget to render.
    pub kind: PluginOptionKind,
    /// Optional default value.  Serialised as a JSON scalar.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// Allowed choices for `Enum` options.  Ignored for other kinds.
    #[serde(default)]
    pub choices: Vec<String>,
}

/// Widget kind for a [`PluginOption`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginOptionKind {
    String,
    Bool,
    Int,
    Enum,
}

impl PluginManifest {
    pub fn from_manifest_file(path: &Path) -> Result<Self, PluginError> {
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str::<Self>(&text)?)
    }

    pub fn validate(&self, plugin_dir: &Path) -> Result<(), PluginError> {
        if self.schema_version > CURRENT_PLUGIN_SCHEMA_VERSION {
            return Err(PluginError::UnsupportedSchema {
                path: plugin_dir.join(PLUGIN_MANIFEST_FILE),
                version: self.schema_version,
                supported: CURRENT_PLUGIN_SCHEMA_VERSION,
            });
        }

        require_non_empty(plugin_dir, "id", &self.id)?;
        require_non_empty(plugin_dir, "name", &self.name)?;
        require_non_empty(plugin_dir, "version", &self.version)?;
        require_non_empty(plugin_dir, "license", &self.license)?;

        if self.entry.is_empty() {
            return Err(invalid_manifest(plugin_dir, "entry must not be empty"));
        }

        validate_entry_path(plugin_dir, &self.entry[0])?;

        if self.classes.is_empty() {
            return Err(invalid_manifest(plugin_dir, "classes must not be empty"));
        }

        if !is_stable_plugin_id(&self.id) {
            return Err(invalid_manifest(
                plugin_dir,
                "id must contain only ASCII letters, digits, '.', '_', or '-'",
            ));
        }

        Ok(())
    }

    pub fn entry_path(&self, plugin_dir: &Path) -> Result<PathBuf, PluginError> {
        self.validate(plugin_dir)?;
        Ok(plugin_dir.join(&self.entry[0]))
    }

    pub fn supports_extension(&self, extension: &str) -> bool {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        self.extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
    }

    pub fn supports_mime_type(&self, mime_type: &str) -> bool {
        self.mime_types
            .iter()
            .any(|candidate| candidate == mime_type)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginClass {
    Unpacker,
    Prediffer,
    EditorComplement,
    ExternalViewer,
    FolderVirtualizer,
    /// Extracts text from PDF or office documents (used by pdf-to-text and libreoffice-extract plugins).
    DocumentTextExtractor,
    /// Performs OCR to produce text from image or PDF inputs (used by tesseract-ocr plugin).
    OcrEngine,
    /// Renders document pages to images for rendered-document compare (future use).
    PdfRenderer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginSandbox {
    pub network: bool,
    pub writes_input: bool,
    pub requires_home_access: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPlugin {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: PluginManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiscovery {
    pub plugins: Vec<DiscoveredPlugin>,
    pub errors: Vec<PluginDiscoveryError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiscoveryError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PluginExecutionOptions {
    pub timeout: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
    pub text_output_limit: usize,
    /// Maximum total bytes accepted from a streaming plugin before the host
    /// stops reading and returns [`PluginError::StreamTotalBytesExceeded`].
    /// Ignored for non-streaming plugins.
    pub max_total_bytes: usize,
    pub temp_root: Option<PathBuf>,
    pub cancellation: Option<PluginCancellationToken>,
}

impl Default for PluginExecutionOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            stdout_limit: 1024 * 1024,
            stderr_limit: 64 * 1024,
            text_output_limit: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            temp_root: None,
            cancellation: None,
        }
    }
}

/// A single chunk emitted by a streaming plugin.
///
/// Chunks are opaque byte blobs — parse them with [`PluginChunk::parse_json`]
/// into a caller-defined type, or inspect [`PluginChunk::bytes`] directly.
#[derive(Debug, Clone)]
pub struct PluginChunk {
    pub bytes: Vec<u8>,
}

impl PluginChunk {
    /// Deserialize the chunk bytes as JSON into `T`.
    pub fn parse_json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.bytes)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl PluginCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginExecutionResult {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginOperation {
    Probe,
    Prediff,
    UnpackText,
    ListVirtualFolder,
    UnpackFolder,
}

impl PluginOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Prediff => "prediff",
            Self::UnpackText => "unpack_text",
            Self::ListVirtualFolder => "list_virtual_folder",
            Self::UnpackFolder => "unpack_folder",
        }
    }
}

/// A single node in a virtual folder tree returned by the `unpack_folder` operation.
#[derive(Debug, Clone, Deserialize)]
pub struct VirtualNode {
    pub path: String,
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Response produced by the `unpack_folder` operation.
#[derive(Debug, Clone, Deserialize)]
pub struct UnpackFolderResponse {
    pub ok: bool,
    #[serde(default)]
    pub tree: Vec<VirtualNode>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInputDescriptor {
    pub role: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(default)]
    pub read_only: bool,
}

impl PluginInputDescriptor {
    pub fn for_file(role: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            role: role.into(),
            display_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            extension: path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.to_ascii_lowercase()),
            path,
            mime_type: None,
            read_only: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginTextOperationOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<String>,
    /// OCR / text-extraction language hint (e.g. `"eng"`). Passed through to
    /// text-extractor and OCR plugins via `options.language`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOperationRequest {
    #[serde(default = "current_plugin_protocol_version")]
    pub protocol_version: u32,
    pub operation: PluginOperation,
    pub request_id: String,
    pub inputs: Vec<PluginInputDescriptor>,
    pub options: PluginTextOperationOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginOperationStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOperationResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub status: PluginOperationStatus,
    #[serde(default)]
    pub outputs: Vec<PluginOperationOutput>,
    #[serde(default)]
    pub error: Option<PluginOperationError>,
    #[serde(default)]
    pub diagnostics: Vec<PluginDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginOutputKind {
    Text,
    File,
    VirtualFolder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOperationOutput {
    pub role: String,
    pub kind: PluginOutputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiagnostic {
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOperationError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginTextResult {
    pub role: String,
    pub text: String,
    pub encoding: Option<String>,
    pub line_ending: Option<String>,
    pub diagnostics: Vec<PluginDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub enum PluginError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidManifest {
        path: PathBuf,
        message: String,
    },
    UnsupportedSchema {
        path: PathBuf,
        version: u32,
        supported: u32,
    },
    ExecutionFailed {
        status_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    TimedOut {
        timeout: Duration,
        stderr: String,
    },
    Cancelled {
        stderr: String,
    },
    OutputTooLarge {
        stream: PluginOutputStream,
        limit: usize,
        actual: u64,
    },
    UnsupportedOperation {
        plugin_id: String,
        operation: PluginOperation,
    },
    InvalidResponse {
        message: String,
    },
    PluginResponseError {
        code: String,
        message: String,
        diagnostics: Vec<PluginDiagnostic>,
    },
    /// [`run_streaming_plugin`] was called on a manifest that does not declare
    /// `streaming: true`.
    NotStreaming,
    /// The accumulated chunk bytes exceeded [`PluginExecutionOptions::max_total_bytes`].
    StreamTotalBytesExceeded {
        limit: usize,
        actual: usize,
    },
    /// A chunk header declared a length but the process closed stdout before
    /// delivering all the promised bytes.
    TruncatedChunk {
        declared_len: usize,
        actual_len: usize,
    },
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::InvalidManifest { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            Self::UnsupportedSchema {
                path,
                version,
                supported,
            } => write!(
                f,
                "{}: unsupported plugin schema version {version}; supported version is {supported}",
                path.display()
            ),
            Self::ExecutionFailed {
                status_code,
                stderr,
                ..
            } => {
                write!(f, "plugin exited with status {status_code:?}")?;
                if !stderr.trim().is_empty() {
                    write!(f, ": {}", stderr.trim())?;
                }
                Ok(())
            }
            Self::TimedOut { timeout, stderr } => {
                write!(f, "plugin timed out after {} ms", timeout.as_millis())?;
                if !stderr.trim().is_empty() {
                    write!(f, ": {}", stderr.trim())?;
                }
                Ok(())
            }
            Self::Cancelled { stderr } => {
                write!(f, "plugin execution cancelled")?;
                if !stderr.trim().is_empty() {
                    write!(f, ": {}", stderr.trim())?;
                }
                Ok(())
            }
            Self::OutputTooLarge {
                stream,
                limit,
                actual,
            } => write!(
                f,
                "plugin {stream:?} output is too large: {actual} bytes exceeds {limit} byte limit"
            ),
            Self::UnsupportedOperation {
                plugin_id,
                operation,
            } => write!(
                f,
                "plugin '{plugin_id}' does not support {}",
                operation.as_str()
            ),
            Self::InvalidResponse { message } => write!(f, "invalid plugin response: {message}"),
            Self::PluginResponseError { code, message, .. } => {
                write!(f, "plugin response error {code}: {message}")
            }
            Self::NotStreaming => {
                write!(f, "plugin does not declare streaming: true in its manifest")
            }
            Self::StreamTotalBytesExceeded { limit, actual } => write!(
                f,
                "streaming plugin output exceeded {limit} byte cap (received {actual} bytes)"
            ),
            Self::TruncatedChunk {
                declared_len,
                actual_len,
            } => write!(
                f,
                "streaming plugin closed stdout mid-chunk: declared {declared_len} bytes, got {actual_len}"
            ),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<io::Error> for PluginError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub fn discover_plugins(roots: &[PathBuf]) -> PluginDiscovery {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    let mut seen_ids = BTreeSet::new();

    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    errors.push(discovery_error(root, err.to_string()));
                    continue;
                }
            };
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }

            let manifest_path = plugin_dir.join(PLUGIN_MANIFEST_FILE);
            if !manifest_path.exists() {
                continue;
            }

            match load_discovered_plugin(&plugin_dir, &manifest_path) {
                Ok(plugin) => {
                    if seen_ids.insert(plugin.manifest.id.clone()) {
                        plugins.push(plugin);
                    } else {
                        errors.push(discovery_error(
                            &manifest_path,
                            format!("duplicate plugin id '{}'", plugin.manifest.id),
                        ));
                    }
                }
                Err(err) => errors.push(discovery_error(&manifest_path, err.to_string())),
            }
        }
    }

    PluginDiscovery { plugins, errors }
}

pub fn plugin_discovery_roots(paths: &AppPaths) -> Vec<PathBuf> {
    let mut roots = vec![paths.user_plugins_dir()];
    roots.extend(AppPaths::system_plugins_dirs());
    roots
}

pub fn discover_installed_plugins(paths: &AppPaths) -> PluginDiscovery {
    discover_plugins(&plugin_discovery_roots(paths))
}

pub fn run_plugin_helper(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    request_json: &str,
    options: &PluginExecutionOptions,
) -> Result<PluginExecutionResult, PluginError> {
    Ok(run_plugin_helper_with_temp(plugin_dir, manifest, request_json, options)?.result)
}

fn run_plugin_helper_with_temp(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    request_json: &str,
    options: &PluginExecutionOptions,
) -> Result<PluginExecutionWithTemp, PluginError> {
    manifest.validate(plugin_dir)?;
    let executable = manifest.entry_path(plugin_dir)?;
    let temp_dir = TemporaryPluginDir::new(options.temp_root.as_deref())?;
    let stdout_path = temp_dir.path().join("stdout");
    let stderr_path = temp_dir.path().join("stderr");
    let stdout = create_owner_only_file(&stdout_path)?;
    let stderr = create_owner_only_file(&stderr_path)?;

    let mut command = Command::new(&executable);
    command
        .args(manifest.entry.iter().skip(1))
        .current_dir(plugin_dir)
        .env("LINSYNC_PLUGIN_TEMP_DIR", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let mut child = spawn_plugin_helper_sandboxed(
        command,
        plugin_dir,
        manifest,
        request_json,
        temp_dir.path(),
    )?;
    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(request_json.as_bytes()) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {}
            Err(err) => return Err(PluginError::Io(err)),
        }
    }

    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }

        if options
            .cancellation
            .as_ref()
            .is_some_and(PluginCancellationToken::is_cancelled)
        {
            kill_plugin_helper(&mut child);
            let stderr = read_limited_text(
                &stderr_path,
                PluginOutputStream::Stderr,
                options.stderr_limit,
            )
            .unwrap_or_default();
            return Err(PluginError::Cancelled { stderr });
        }

        if started.elapsed() >= options.timeout {
            kill_plugin_helper(&mut child);
            let stderr = read_limited_text(
                &stderr_path,
                PluginOutputStream::Stderr,
                options.stderr_limit,
            )
            .unwrap_or_default();
            return Err(PluginError::TimedOut {
                timeout: options.timeout,
                stderr,
            });
        }

        // Enforce the output limits *during* the run, not just after exit, so a
        // helper that floods stdout/stderr cannot fill the disk/RAM before the
        // post-exit check fires. A flooding helper is killed as soon as either
        // stream crosses its cap.
        let oversize = output_limit_exceeded(
            &stdout_path,
            PluginOutputStream::Stdout,
            options.stdout_limit,
        )
        .or_else(|| {
            output_limit_exceeded(
                &stderr_path,
                PluginOutputStream::Stderr,
                options.stderr_limit,
            )
        });
        if let Some(err) = oversize {
            kill_plugin_helper(&mut child);
            return Err(err);
        }

        std::thread::sleep(Duration::from_millis(10));
    };

    let stdout = read_limited_text(
        &stdout_path,
        PluginOutputStream::Stdout,
        options.stdout_limit,
    )?;
    let stderr = read_limited_text(
        &stderr_path,
        PluginOutputStream::Stderr,
        options.stderr_limit,
    )?;

    if !status.success() {
        return Err(PluginError::ExecutionFailed {
            status_code: status.code(),
            stdout,
            stderr,
        });
    }

    Ok(PluginExecutionWithTemp {
        result: PluginExecutionResult { stdout, stderr },
        temp_dir,
    })
}

/// Spawn a streaming plugin and collect all length-prefixed chunks from its
/// stdout.
///
/// The manifest must declare `streaming: true`; otherwise
/// [`PluginError::NotStreaming`] is returned immediately.
///
/// ## Wire format
///
/// Each chunk is framed as a 4-byte little-endian `u32` length header followed
/// by exactly that many bytes of payload.  The host reads frames until EOF,
/// a timeout, a cancellation, or until the accumulated byte count exceeds
/// [`PluginExecutionOptions::max_total_bytes`].
///
/// ## Back-pressure / cap behaviour
///
/// When the cumulative payload size would exceed `max_total_bytes` the host
/// kills the child, discards any partial frame, and returns
/// [`PluginError::StreamTotalBytesExceeded`].
pub fn run_streaming_plugin(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    request_json: &str,
    options: &PluginExecutionOptions,
) -> Result<Vec<PluginChunk>, PluginError> {
    if !manifest.streaming {
        return Err(PluginError::NotStreaming);
    }

    manifest.validate(plugin_dir)?;
    let executable = manifest.entry_path(plugin_dir)?;
    let temp_dir = TemporaryPluginDir::new(options.temp_root.as_deref())?;
    let stderr_path = temp_dir.path().join("stderr");
    let stderr_file = create_owner_only_file(&stderr_path)?;

    let mut command = Command::new(&executable);
    command
        .args(manifest.entry.iter().skip(1))
        .current_dir(plugin_dir)
        .env("LINSYNC_PLUGIN_TEMP_DIR", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr_file));

    // Streaming helpers run under the same sandbox policy as request/response
    // helpers — never unconfined.
    let mut child = spawn_plugin_helper_sandboxed(
        command,
        plugin_dir,
        manifest,
        request_json,
        temp_dir.path(),
    )?;

    // Write the request on stdin then close it so the plugin knows the request
    // is complete.
    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(request_json.as_bytes()) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {}
            Err(err) => return Err(PluginError::Io(err)),
        }
        // stdin is dropped here, which closes the write end
    }

    // Take the piped stdout handle.
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("streaming plugin stdout pipe missing"))?;

    let started = std::time::Instant::now();
    let mut chunks: Vec<PluginChunk> = Vec::new();
    let mut total_bytes: usize = 0;

    loop {
        // Honour timeout / cancellation between frames.
        if options
            .cancellation
            .as_ref()
            .is_some_and(PluginCancellationToken::is_cancelled)
        {
            kill_plugin_helper(&mut child);
            let stderr = read_limited_text(
                &stderr_path,
                PluginOutputStream::Stderr,
                options.stderr_limit,
            )
            .unwrap_or_default();
            return Err(PluginError::Cancelled { stderr });
        }

        if started.elapsed() >= options.timeout {
            kill_plugin_helper(&mut child);
            let stderr = read_limited_text(
                &stderr_path,
                PluginOutputStream::Stderr,
                options.stderr_limit,
            )
            .unwrap_or_default();
            return Err(PluginError::TimedOut {
                timeout: options.timeout,
                stderr,
            });
        }

        // Read the 4-byte length header.
        let mut header = [0u8; 4];
        match read_exact_or_eof(&mut stdout, &mut header)? {
            0 => break, // clean EOF — plugin finished
            4 => {}     // full header received
            n => {
                // Partial header — treat as truncated chunk.
                kill_plugin_helper(&mut child);
                return Err(PluginError::TruncatedChunk {
                    declared_len: 4,
                    actual_len: n,
                });
            }
        }

        let chunk_len = u32::from_le_bytes(header) as usize;

        // Cap check: would accepting this chunk push us over the limit?
        //
        // Count the 4-byte frame header toward the total as well, so a flood of
        // zero-length chunks still consumes budget and cannot grow the
        // accumulated `chunks` Vec without bound (memory DoS).
        let new_total = total_bytes
            .saturating_add(header.len())
            .saturating_add(chunk_len);
        if new_total > options.max_total_bytes {
            kill_plugin_helper(&mut child);
            return Err(PluginError::StreamTotalBytesExceeded {
                limit: options.max_total_bytes,
                actual: new_total,
            });
        }

        // Read the chunk payload.
        let mut payload = vec![0u8; chunk_len];
        let n = read_exact_or_eof(&mut stdout, &mut payload)?;
        if n != chunk_len {
            kill_plugin_helper(&mut child);
            return Err(PluginError::TruncatedChunk {
                declared_len: chunk_len,
                actual_len: n,
            });
        }

        total_bytes = new_total;
        chunks.push(PluginChunk { bytes: payload });
    }

    // Wait for the child to exit and check the status.
    let stderr_text = read_limited_text(
        &stderr_path,
        PluginOutputStream::Stderr,
        options.stderr_limit,
    )
    .unwrap_or_default();

    let status = child.wait()?;
    if !status.success() {
        return Err(PluginError::ExecutionFailed {
            status_code: status.code(),
            stdout: String::new(),
            stderr: stderr_text,
        });
    }

    Ok(chunks)
}

/// Read bytes into `buf` from `reader`, returning the number of bytes read.
///
/// Returns `Ok(0)` only if the very first byte read results in EOF.  Returns
/// `Ok(buf.len())` on a complete fill.  Returns a short count if EOF arrives
/// after at least one byte — the caller treats that as a truncated frame.
fn read_exact_or_eof(reader: &mut impl io::Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(filled)
}

/// Extract the source path from a plugin request JSON so the sandbox can grant
/// the helper read access to the file being processed. Checks both `source`
/// (legacy unpack_folder protocol) and `inputs[0].path` (unpack_text / prediff
/// protocol); falls back to `plugin_dir` so the helper can at least read its
/// own binary.
#[cfg(feature = "sandbox")]
fn plugin_source_path_from_request(request_json: &str, plugin_dir: &Path) -> std::path::PathBuf {
    serde_json::from_str::<serde_json::Value>(request_json)
        .ok()
        .and_then(|v| {
            if let Some(s) = v.get("source").and_then(|s| s.as_str()) {
                return Some(std::path::PathBuf::from(s));
            }
            v.get("inputs")
                .and_then(|inputs| inputs.get(0))
                .and_then(|first| first.get("path"))
                .and_then(|p| p.as_str())
                .map(std::path::PathBuf::from)
        })
        .unwrap_or_else(|| plugin_dir.to_path_buf())
}

/// Spawn a configured plugin-helper `Command` under the sandbox policy derived
/// from the manifest. Both the request/response runner
/// ([`run_plugin_helper_with_temp`]) and the streaming runner
/// ([`run_streaming_plugin`]) go through here so neither can spawn a helper
/// unconfined. When the `sandbox` feature is disabled it falls back to a bare,
/// process-group-isolated spawn.
fn spawn_plugin_helper_sandboxed(
    mut command: Command,
    plugin_dir: &Path,
    manifest: &PluginManifest,
    request_json: &str,
    temp_dir: &Path,
) -> Result<Child, PluginError> {
    // Put the helper in its own process group so kill_plugin_helper can SIGKILL
    // the entire descendant tree (helper + grandchildren like soffice.bin).
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(feature = "sandbox")]
    {
        use linsync_sandbox::{PluginSandboxFields, SandboxedCommand, policy_for_plugin};

        let source_path = plugin_source_path_from_request(request_json, plugin_dir);
        let sandbox_fields = PluginSandboxFields {
            network: manifest.sandbox.network,
            requires_home_access: manifest.sandbox.requires_home_access,
        };
        let policy = policy_for_plugin(&sandbox_fields, plugin_dir, &source_path, temp_dir);

        SandboxedCommand::new(command, policy)
            .spawn()
            .map_err(|e| PluginError::Io(std::io::Error::other(e.to_string())))
    }

    #[cfg(not(feature = "sandbox"))]
    {
        let _ = (plugin_dir, manifest, request_json, temp_dir);
        Ok(spawn_plugin_helper(&mut command)?)
    }
}

#[cfg(not(feature = "sandbox"))]
fn spawn_plugin_helper(command: &mut Command) -> io::Result<Child> {
    // Spawn the helper in its own process group so that on timeout or
    // cancellation we can SIGKILL the entire descendant tree (helper script
    // + any grandchildren it spawned, e.g. soffice.bin under
    // libreoffice-extract). Without this the host only kills the immediate
    // child; grandchildren leak as orphans reparented to init.
    use std::os::unix::process::CommandExt;
    command.process_group(0);

    let mut last_error = None;
    for _ in 0..10 {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(err) if err.raw_os_error() == Some(libc::ETXTBSY) => {
                last_error = Some(err);
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_error.unwrap_or_else(|| io::Error::other("failed to spawn plugin helper")))
}

/// SIGKILL the helper's process group, then reap. Must be called instead of
/// `child.kill()` whenever we need to stop a plugin: it kills the helper plus
/// every grandchild (e.g. LibreOffice / Tesseract / poppler tools) the helper
/// may have spawned.
fn kill_plugin_helper(child: &mut Child) {
    let pid = child.id() as libc::pid_t;
    // killpg(pgid, sig) where pgid is the process-group id. We spawned the
    // child with process_group(0) so its pid == its pgid.
    // SAFETY: killpg is async-signal-safe; pid is a valid process id we own.
    unsafe {
        libc::killpg(pid, libc::SIGKILL);
    }
    let _ = child.wait();
}

pub fn run_prediffer_plugin(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    input: PluginInputDescriptor,
    execution_options: &PluginExecutionOptions,
) -> Result<PluginTextResult, PluginError> {
    run_prediffer_plugin_with_options(
        plugin_dir,
        manifest,
        input,
        &PluginTextOperationOptions::default(),
        execution_options,
    )
}

pub fn run_prediffer_plugin_with_options(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    input: PluginInputDescriptor,
    operation_options: &PluginTextOperationOptions,
    execution_options: &PluginExecutionOptions,
) -> Result<PluginTextResult, PluginError> {
    run_text_operation(
        PluginOperation::Prediff,
        PluginClass::Prediffer,
        plugin_dir,
        manifest,
        vec![input],
        operation_options,
        execution_options,
    )
}

pub fn run_unpack_text_plugin(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    input: PluginInputDescriptor,
    execution_options: &PluginExecutionOptions,
) -> Result<PluginTextResult, PluginError> {
    run_unpack_text_plugin_with_options(
        plugin_dir,
        manifest,
        input,
        &PluginTextOperationOptions::default(),
        execution_options,
    )
}

pub fn run_unpack_text_plugin_with_options(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    input: PluginInputDescriptor,
    operation_options: &PluginTextOperationOptions,
    execution_options: &PluginExecutionOptions,
) -> Result<PluginTextResult, PluginError> {
    run_text_operation(
        PluginOperation::UnpackText,
        PluginClass::Unpacker,
        plugin_dir,
        manifest,
        vec![input],
        operation_options,
        execution_options,
    )
}

/// Invoke an `unpack_folder` plugin and return the virtual folder tree.
///
/// The request sent to the plugin is `{"op":"unpack_folder","source":<source>}`.
/// The plugin must respond with `{"ok":true,"tree":[...]}` or
/// `{"ok":false,"error":"..."}`.
pub fn run_unpack_folder_plugin(
    plugin_dir: &Path,
    manifest: &PluginManifest,
    source: &str,
    options: &PluginExecutionOptions,
) -> Result<UnpackFolderResponse, PluginError> {
    let req = serde_json::json!({"op": "unpack_folder", "source": source});
    let raw = run_plugin_helper(plugin_dir, manifest, &req.to_string(), options)?;
    Ok(serde_json::from_str(&raw.stdout)?)
}

fn run_text_operation(
    operation: PluginOperation,
    required_class: PluginClass,
    plugin_dir: &Path,
    manifest: &PluginManifest,
    inputs: Vec<PluginInputDescriptor>,
    operation_options: &PluginTextOperationOptions,
    execution_options: &PluginExecutionOptions,
) -> Result<PluginTextResult, PluginError> {
    if !manifest.classes.contains(&required_class) {
        return Err(PluginError::UnsupportedOperation {
            plugin_id: manifest.id.clone(),
            operation,
        });
    }

    let request = PluginOperationRequest {
        protocol_version: CURRENT_PLUGIN_PROTOCOL_VERSION,
        operation,
        request_id: plugin_request_id(operation),
        inputs,
        options: operation_options.clone(),
    };
    let request_json = serde_json::to_string(&request)?;
    let execution = run_plugin_helper_with_temp(
        plugin_dir,
        manifest,
        &format!("{request_json}\n"),
        execution_options,
    )?;

    parse_text_operation_response(
        &request,
        &execution.result.stdout,
        execution.temp_dir.path(),
        execution_options.text_output_limit,
    )
}

fn plugin_request_id(operation: PluginOperation) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "linsync-{}-{}-{now}",
        operation.as_str(),
        std::process::id()
    )
}

fn parse_text_operation_response(
    request: &PluginOperationRequest,
    stdout: &str,
    temp_dir: &Path,
    text_output_limit: usize,
) -> Result<PluginTextResult, PluginError> {
    let response: PluginOperationResponse = serde_json::from_str(stdout.trim())?;

    if response.protocol_version != CURRENT_PLUGIN_PROTOCOL_VERSION {
        return Err(PluginError::InvalidResponse {
            message: format!(
                "protocol version {} is not supported",
                response.protocol_version
            ),
        });
    }

    if response.request_id != request.request_id {
        return Err(PluginError::InvalidResponse {
            message: format!(
                "request id mismatch: expected {}, got {}",
                request.request_id, response.request_id
            ),
        });
    }

    match response.status {
        PluginOperationStatus::Error => {
            let Some(error) = response.error else {
                return Err(PluginError::InvalidResponse {
                    message: "error response did not include an error object".to_owned(),
                });
            };
            Err(PluginError::PluginResponseError {
                code: error.code,
                message: error.message,
                diagnostics: response.diagnostics,
            })
        }
        PluginOperationStatus::Ok => {
            let output = response
                .outputs
                .into_iter()
                .find(|output| output.kind == PluginOutputKind::Text)
                .ok_or_else(|| PluginError::InvalidResponse {
                    message: "ok response did not include a text output".to_owned(),
                })?;

            let text = match (output.inline_text, output.path) {
                (Some(text), None) => text,
                (None, Some(path)) => read_plugin_text_output(&path, temp_dir, text_output_limit)?,
                (Some(_), Some(_)) => {
                    return Err(PluginError::InvalidResponse {
                        message: "text output must include either inline_text or path, not both"
                            .to_owned(),
                    });
                }
                (None, None) => {
                    return Err(PluginError::InvalidResponse {
                        message: "text output must include inline_text or path".to_owned(),
                    });
                }
            };

            Ok(PluginTextResult {
                role: output.role,
                text,
                encoding: output.encoding,
                line_ending: output.line_ending,
                diagnostics: response.diagnostics,
            })
        }
    }
}

fn read_plugin_text_output(
    path: &Path,
    temp_dir: &Path,
    limit: usize,
) -> Result<String, PluginError> {
    let output_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        temp_dir.join(path)
    };
    let temp_root = temp_dir.canonicalize()?;
    let output_path = output_path.canonicalize()?;

    if !output_path.starts_with(&temp_root) {
        return Err(PluginError::InvalidResponse {
            message: "text output path must stay under the plugin temp directory".to_owned(),
        });
    }

    if !output_path.is_file() {
        return Err(PluginError::InvalidResponse {
            message: "text output path must reference a regular file".to_owned(),
        });
    }

    read_limited_text(&output_path, PluginOutputStream::Stdout, limit)
}

fn load_discovered_plugin(
    plugin_dir: &Path,
    manifest_path: &Path,
) -> Result<DiscoveredPlugin, PluginError> {
    let manifest = PluginManifest::from_manifest_file(manifest_path)?;
    manifest.validate(plugin_dir)?;
    Ok(DiscoveredPlugin {
        root: plugin_dir.to_path_buf(),
        manifest_path: manifest_path.to_path_buf(),
        manifest,
    })
}

fn require_non_empty(
    plugin_dir: &Path,
    field: &'static str,
    value: &str,
) -> Result<(), PluginError> {
    if value.trim().is_empty() {
        return Err(invalid_manifest(
            plugin_dir,
            format!("{field} must not be empty"),
        ));
    }

    Ok(())
}

fn validate_entry_path(plugin_dir: &Path, entry: &str) -> Result<(), PluginError> {
    if entry.trim().is_empty() {
        return Err(invalid_manifest(
            plugin_dir,
            "entry command must not be empty",
        ));
    }

    let path = Path::new(entry);
    if path.is_absolute() {
        return Err(invalid_manifest(
            plugin_dir,
            "entry command must be relative to the plugin directory",
        ));
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(invalid_manifest(
                plugin_dir,
                "entry command must not escape the plugin directory",
            ));
        }
    }

    Ok(())
}

fn is_stable_plugin_id(id: &str) -> bool {
    id.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn invalid_manifest(plugin_dir: &Path, message: impl Into<String>) -> PluginError {
    PluginError::InvalidManifest {
        path: plugin_dir.join(PLUGIN_MANIFEST_FILE),
        message: message.into(),
    }
}

fn discovery_error(path: &Path, message: String) -> PluginDiscoveryError {
    PluginDiscoveryError {
        path: path.to_path_buf(),
        message,
    }
}

/// Return [`PluginError::OutputTooLarge`] if the file at `path` already exceeds
/// `limit` bytes, otherwise `None`. Used to enforce stdout/stderr caps while the
/// helper is still running (a missing file simply means nothing written yet).
fn output_limit_exceeded(
    path: &Path,
    stream: PluginOutputStream,
    limit: usize,
) -> Option<PluginError> {
    let actual = fs::metadata(path).ok()?.len();
    (actual > limit as u64).then_some(PluginError::OutputTooLarge {
        stream,
        limit,
        actual,
    })
}

fn read_limited_text(
    path: &Path,
    stream: PluginOutputStream,
    limit: usize,
) -> Result<String, PluginError> {
    let actual = fs::metadata(path)?.len();
    if actual > limit as u64 {
        return Err(PluginError::OutputTooLarge {
            stream,
            limit,
            actual,
        });
    }

    let bytes = fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn create_owner_only_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

struct TemporaryPluginDir {
    path: PathBuf,
}

struct PluginExecutionWithTemp {
    result: PluginExecutionResult,
    temp_dir: TemporaryPluginDir,
}

impl TemporaryPluginDir {
    fn new(root: Option<&Path>) -> io::Result<Self> {
        let root = root
            .map(Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root)?;

        for attempt in 0..100 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = root.join(format!(
                "linsync-plugin-{}-{now}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                    }
                    return Ok(Self { path });
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not create unique plugin temporary directory",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryPluginDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validates_manifest_and_entry_path() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/example");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("normalize-text"), "").unwrap();
        let manifest = sample_manifest("example.normalizer");

        assert!(manifest.validate(&plugin_dir).is_ok());
        assert_eq!(
            manifest.entry_path(&plugin_dir).unwrap(),
            plugin_dir.join("normalize-text")
        );
        assert!(manifest.supports_extension(".TXT"));
        assert!(manifest.supports_mime_type("text/plain"));
    }

    #[test]
    fn rejects_invalid_manifest_shapes() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/bad");
        fs::create_dir_all(&plugin_dir).unwrap();

        let mut missing_entry = sample_manifest("bad.missing-entry");
        missing_entry.entry.clear();
        assert!(matches!(
            missing_entry.validate(&plugin_dir),
            Err(PluginError::InvalidManifest { .. })
        ));

        let mut escaping_entry = sample_manifest("bad.escaping-entry");
        escaping_entry.entry = vec!["../tool".to_owned()];
        assert!(matches!(
            escaping_entry.validate(&plugin_dir),
            Err(PluginError::InvalidManifest { .. })
        ));

        let mut absolute_entry = sample_manifest("bad.absolute-entry");
        absolute_entry.entry = vec!["/tmp/tool".to_owned()];
        assert!(matches!(
            absolute_entry.validate(&plugin_dir),
            Err(PluginError::InvalidManifest { .. })
        ));

        let mut bad_id = sample_manifest("bad id");
        assert!(matches!(
            bad_id.validate(&plugin_dir),
            Err(PluginError::InvalidManifest { .. })
        ));

        bad_id.id = "bad.future".to_owned();
        bad_id.schema_version = CURRENT_PLUGIN_SCHEMA_VERSION + 1;
        assert!(matches!(
            bad_id.validate(&plugin_dir),
            Err(PluginError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn manifest_requires_support_and_sandbox_declarations() {
        let missing_declarations = r#"{
  "schema_version": 1,
  "id": "example.incomplete",
  "name": "Incomplete",
  "version": "1.0.0",
  "license": "MIT",
  "entry": ["helper"],
  "classes": ["prediffer"]
}"#;

        let err = serde_json::from_str::<PluginManifest>(missing_declarations).unwrap_err();

        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn discovers_valid_plugins_and_reports_errors() {
        let fixture = TempFixture::new();
        let root = fixture.path.join("plugins");
        let valid_dir = root.join("valid");
        let duplicate_dir = root.join("duplicate");
        let invalid_dir = root.join("invalid");
        fs::create_dir_all(&valid_dir).unwrap();
        fs::create_dir_all(&duplicate_dir).unwrap();
        fs::create_dir_all(&invalid_dir).unwrap();

        write_manifest(&valid_dir, &sample_manifest("example.normalizer"));
        write_manifest(&duplicate_dir, &sample_manifest("example.normalizer"));
        let mut invalid = sample_manifest("example.invalid");
        invalid.entry = vec!["../escape".to_owned()];
        write_manifest(&invalid_dir, &invalid);

        let discovery = discover_plugins(&[root]);

        assert_eq!(discovery.plugins.len(), 1);
        assert_eq!(discovery.plugins[0].manifest.id, "example.normalizer");
        assert_eq!(discovery.errors.len(), 2);
        assert!(
            discovery
                .errors
                .iter()
                .any(|err| err.message.contains("duplicate plugin id"))
        );
        assert!(
            discovery
                .errors
                .iter()
                .any(|err| err.message.contains("must not escape"))
        );
    }

    #[test]
    fn discovers_installed_plugins_from_xdg_user_root_before_system_roots() {
        let fixture = TempFixture::new();
        let paths = AppPaths::from_base_dirs(
            fixture.path.join("config"),
            fixture.path.join("data"),
            fixture.path.join("cache"),
            fixture.path.join("state"),
        );
        let plugin_dir = paths.user_plugins_dir().join("normalizer");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(&plugin_dir, "normalize-text", "#!/bin/sh\n");
        write_manifest(&plugin_dir, &sample_manifest("example.installed"));

        let roots = plugin_discovery_roots(&paths);
        assert_eq!(roots[0], paths.user_plugins_dir());
        assert!(roots.contains(&PathBuf::from("/usr/local/share/linsync/plugins")));
        assert!(roots.contains(&PathBuf::from("/usr/share/linsync/plugins")));

        let discovery = discover_installed_plugins(&paths);

        assert_eq!(discovery.errors, Vec::new());
        assert_eq!(discovery.plugins.len(), 1);
        assert_eq!(discovery.plugins[0].manifest.id, "example.installed");
        assert_eq!(discovery.plugins[0].root, plugin_dir);
    }

    #[test]
    fn runs_helper_with_stdin_capture_and_temp_cleanup() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/runner");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "helper.sh",
            r#"#!/bin/sh
read request
echo "request=$request"
echo "temp=$LINSYNC_PLUGIN_TEMP_DIR"
touch "$LINSYNC_PLUGIN_TEMP_DIR/touched"
echo "warning" >&2
"#,
        );
        let mut manifest = sample_manifest("example.runner");
        manifest.entry = vec!["helper.sh".to_owned()];

        let result = run_plugin_helper(
            &plugin_dir,
            &manifest,
            "{\"ping\":true}\n",
            &PluginExecutionOptions::default(),
        )
        .unwrap();

        assert!(result.stdout.contains("request={\"ping\":true}"));
        assert!(result.stderr.contains("warning"));
        let temp_path = result
            .stdout
            .lines()
            .find_map(|line| line.strip_prefix("temp="))
            .expect("helper reports temp path");
        assert!(!Path::new(temp_path).exists());
    }

    #[test]
    fn helper_nonzero_status_is_error_with_captured_output() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/failing");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "fail.sh",
            r#"#!/bin/sh
echo '{"partial":true}'
echo "failure details" >&2
exit 7
"#,
        );
        let mut manifest = sample_manifest("example.failing");
        manifest.entry = vec!["fail.sh".to_owned()];

        let err = run_plugin_helper(
            &plugin_dir,
            &manifest,
            "{}",
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        match err {
            PluginError::ExecutionFailed {
                status_code,
                stdout,
                stderr,
            } => {
                assert_eq!(status_code, Some(7));
                assert!(stdout.contains("partial"));
                assert!(stderr.contains("failure details"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn helper_timeout_and_cancellation_stop_process() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/slow");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "sleep.sh",
            r#"#!/bin/sh
echo "started" >&2
sleep 2
"#,
        );
        let mut manifest = sample_manifest("example.slow");
        manifest.entry = vec!["sleep.sh".to_owned()];

        let timeout_err = run_plugin_helper(
            &plugin_dir,
            &manifest,
            "{}",
            &PluginExecutionOptions {
                timeout: Duration::from_millis(50),
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(timeout_err, PluginError::TimedOut { .. }));

        let token = PluginCancellationToken::new();
        let cancellation = token.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancellation.cancel();
        });
        let cancel_err = run_plugin_helper(
            &plugin_dir,
            &manifest,
            "{}",
            &PluginExecutionOptions {
                timeout: Duration::from_secs(5),
                cancellation: Some(token),
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();
        handle.join().unwrap();
        assert!(matches!(cancel_err, PluginError::Cancelled { .. }));
    }

    #[test]
    fn helper_output_limits_are_enforced() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/noisy");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "noisy.sh",
            r#"#!/bin/sh
printf 'abcdef'
"#,
        );
        let mut manifest = sample_manifest("example.noisy");
        manifest.entry = vec!["noisy.sh".to_owned()];

        let err = run_plugin_helper(
            &plugin_dir,
            &manifest,
            "{}",
            &PluginExecutionOptions {
                stdout_limit: 3,
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PluginError::OutputTooLarge {
                stream: PluginOutputStream::Stdout,
                limit: 3,
                actual: 6
            }
        ));
    }

    #[test]
    fn runs_prediffer_plugin_for_inline_text() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/prediffer");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "prediff.sh",
            r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
cat <<JSON
{"protocol_version":1,"request_id":"$request_id","status":"ok","outputs":[{"role":"left","kind":"text","inline_text":"normalized text","encoding":"utf-8","line_ending":"lf"}],"diagnostics":[{"severity":"info","message":"normalized"}]}
JSON
"#,
        );
        let mut manifest = sample_manifest("example.prediffer");
        manifest.entry = vec!["prediff.sh".to_owned()];
        let input_path = plugin_dir.join("left.txt");
        fs::write(&input_path, "Normalized   Text").unwrap();

        let result = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", &input_path),
            &PluginExecutionOptions::default(),
        )
        .unwrap();

        assert_eq!(result.role, "left");
        assert_eq!(result.text, "normalized text");
        assert_eq!(result.encoding.as_deref(), Some("utf-8"));
        assert_eq!(result.line_ending.as_deref(), Some("lf"));
        assert_eq!(result.diagnostics[0].message, "normalized");
    }

    #[test]
    fn runs_unpacker_plugin_for_inline_text() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/unpacker");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "unpack.sh",
            r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
cat <<JSON
{"protocol_version":1,"request_id":"$request_id","status":"ok","outputs":[{"role":"source","kind":"text","inline_text":"extracted text","encoding":"utf-8"}],"diagnostics":[]}
JSON
"#,
        );
        let mut manifest = sample_manifest("example.unpacker");
        manifest.entry = vec!["unpack.sh".to_owned()];
        manifest.classes = vec![PluginClass::Unpacker];
        manifest.extensions = vec!["pdf".to_owned()];
        let input_path = plugin_dir.join("document.pdf");
        fs::write(&input_path, b"%PDF sample").unwrap();

        let result = run_unpack_text_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("source", &input_path),
            &PluginExecutionOptions::default(),
        )
        .unwrap();

        assert_eq!(result.role, "source");
        assert_eq!(result.text, "extracted text");
        assert_eq!(result.encoding.as_deref(), Some("utf-8"));
    }

    #[test]
    fn plugin_operation_error_response_is_reported() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/erroring");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "error.sh",
            r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
cat <<JSON
{"protocol_version":1,"request_id":"$request_id","status":"error","error":{"code":"unsupported-input","message":"cannot normalize this input"},"diagnostics":[{"severity":"warning","message":"skipped"}]}
JSON
"#,
        );
        let mut manifest = sample_manifest("example.erroring");
        manifest.entry = vec!["error.sh".to_owned()];

        let err = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        match err {
            PluginError::PluginResponseError {
                code,
                message,
                diagnostics,
            } => {
                assert_eq!(code, "unsupported-input");
                assert_eq!(message, "cannot normalize this input");
                assert_eq!(diagnostics[0].message, "skipped");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn malformed_plugin_json_response_is_rejected() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/malformed-json");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "malformed.sh",
            r#"#!/bin/sh
cat >/dev/null
echo '{not-json'
"#,
        );
        let mut manifest = sample_manifest("example.malformed-json");
        manifest.entry = vec!["malformed.sh".to_owned()];

        let err = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        assert!(matches!(err, PluginError::Json(_)));
    }

    #[test]
    fn plugin_protocol_mismatch_is_rejected() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/protocol-mismatch");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "mismatch.sh",
            r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
cat <<JSON
{"protocol_version":999,"request_id":"$request_id","status":"ok","outputs":[{"role":"left","kind":"text","inline_text":"normalized"}],"diagnostics":[]}
JSON
"#,
        );
        let mut manifest = sample_manifest("example.protocol-mismatch");
        manifest.entry = vec!["mismatch.sh".to_owned()];

        let err = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        assert!(
            matches!(err, PluginError::InvalidResponse { message } if message.contains("protocol version"))
        );
    }

    #[test]
    fn plugin_text_output_can_be_file_backed_under_temp_dir() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/file-output");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "file-output.sh",
            r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
output="$LINSYNC_PLUGIN_TEMP_DIR/output.txt"
printf 'file-backed normalized text' > "$output"
cat <<JSON
{"protocol_version":1,"request_id":"$request_id","status":"ok","outputs":[{"role":"left","kind":"text","path":"$output","encoding":"utf-8"}],"diagnostics":[]}
JSON
"#,
        );
        let mut manifest = sample_manifest("example.file-output");
        manifest.entry = vec!["file-output.sh".to_owned()];

        let result = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions::default(),
        )
        .unwrap();

        assert_eq!(result.role, "left");
        assert_eq!(result.text, "file-backed normalized text");
        assert_eq!(result.encoding.as_deref(), Some("utf-8"));
    }

    #[test]
    fn plugin_text_output_rejects_paths_outside_temp_dir() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/file-output-escape");
        fs::create_dir_all(&plugin_dir).unwrap();
        let escaped_output = fixture.path.join("escaped-output.txt");
        fs::write(&escaped_output, "escaped").unwrap();
        write_helper(
            &plugin_dir,
            "file-output.sh",
            &format!(
                r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
cat <<JSON
{{"protocol_version":1,"request_id":"$request_id","status":"ok","outputs":[{{"role":"left","kind":"text","path":"{}","encoding":"utf-8"}}],"diagnostics":[]}}
JSON
"#,
                escaped_output.display()
            ),
        );
        let mut manifest = sample_manifest("example.file-output-escape");
        manifest.entry = vec!["file-output.sh".to_owned()];

        let err = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        assert!(
            matches!(err, PluginError::InvalidResponse { message } if message.contains("temp directory"))
        );
    }

    #[test]
    fn plugin_text_output_file_limit_is_enforced() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/file-output-large");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "file-output.sh",
            r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
output="$LINSYNC_PLUGIN_TEMP_DIR/output.txt"
printf 'abcdef' > "$output"
cat <<JSON
{"protocol_version":1,"request_id":"$request_id","status":"ok","outputs":[{"role":"left","kind":"text","path":"$output","encoding":"utf-8"}],"diagnostics":[]}
JSON
"#,
        );
        let mut manifest = sample_manifest("example.file-output-large");
        manifest.entry = vec!["file-output.sh".to_owned()];

        let err = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions {
                text_output_limit: 3,
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PluginError::OutputTooLarge {
                stream: PluginOutputStream::Stdout,
                limit: 3,
                actual: 6
            }
        ));
    }

    #[test]
    fn text_operation_rejects_missing_plugin_class() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/wrong-class");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "unpack.sh",
            r#"#!/bin/sh
cat >/dev/null
"#,
        );
        let mut manifest = sample_manifest("example.wrong-class");
        manifest.entry = vec!["unpack.sh".to_owned()];
        manifest.classes = vec![PluginClass::Unpacker];

        let err = run_prediffer_plugin(
            &plugin_dir,
            &manifest,
            PluginInputDescriptor::for_file("left", plugin_dir.join("left.txt")),
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PluginError::UnsupportedOperation {
                plugin_id,
                operation: PluginOperation::Prediff
            } if plugin_id == "example.wrong-class"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn plugin_temporary_artifacts_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempFixture::new();
        let temp_dir = TemporaryPluginDir::new(Some(&fixture.path)).unwrap();
        let output_path = temp_dir.path().join("stdout.txt");
        drop(create_owner_only_file(&output_path).unwrap());

        assert_eq!(
            fs::metadata(temp_dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(output_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    // ---- Streaming protocol tests -----------------------------------------

    #[test]
    fn streaming_plugin_emits_chunks_in_order() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/streaming");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(
            &plugin_dir,
            "stream.sh",
            r#"#!/usr/bin/env bash
read REQ
emit() {
    local json="$1"
    local len=${#json}
    printf '%b' "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
        $(( len        & 0xff )) \
        $(( (len >> 8) & 0xff )) \
        $(( (len >> 16) & 0xff )) \
        $(( (len >> 24) & 0xff )))"
    printf '%s' "$json"
}
emit '{"index":0,"msg":"first"}'
emit '{"index":1,"msg":"second"}'
emit '{"index":2,"msg":"third"}'
"#,
        );
        let mut manifest = sample_manifest("example.streaming");
        manifest.entry = vec!["stream.sh".to_owned()];
        manifest.streaming = true;

        let chunks = run_streaming_plugin(
            &plugin_dir,
            &manifest,
            "{}\n",
            &PluginExecutionOptions::default(),
        )
        .unwrap();

        assert_eq!(chunks.len(), 3);

        #[derive(serde::Deserialize)]
        struct Chunk {
            index: u32,
            msg: String,
        }

        let decoded: Vec<Chunk> = chunks
            .iter()
            .map(|c| c.parse_json::<Chunk>().unwrap())
            .collect();

        assert_eq!(decoded[0].index, 0);
        assert_eq!(decoded[0].msg, "first");
        assert_eq!(decoded[1].index, 1);
        assert_eq!(decoded[1].msg, "second");
        assert_eq!(decoded[2].index, 2);
        assert_eq!(decoded[2].msg, "third");
    }

    #[test]
    fn streaming_plugin_respects_max_total_bytes() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/streaming-large");
        fs::create_dir_all(&plugin_dir).unwrap();
        // Emit two chunks of 20 bytes each (40 total); cap is set below 40.
        write_helper(
            &plugin_dir,
            "stream-large.sh",
            r#"#!/usr/bin/env bash
read REQ
emit() {
    local json="$1"
    local len=${#json}
    printf '%b' "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
        $(( len        & 0xff )) \
        $(( (len >> 8) & 0xff )) \
        $(( (len >> 16) & 0xff )) \
        $(( (len >> 24) & 0xff )))"
    printf '%s' "$json"
}
emit '{"index":0,"msg":"aaaaaaaaaa"}'
emit '{"index":1,"msg":"bbbbbbbbbb"}'
"#,
        );
        let mut manifest = sample_manifest("example.streaming-large");
        manifest.entry = vec!["stream-large.sh".to_owned()];
        manifest.streaming = true;

        // Each chunk JSON is ~28 bytes; set cap to 30 so the second chunk
        // pushes us over.
        let err = run_streaming_plugin(
            &plugin_dir,
            &manifest,
            "{}\n",
            &PluginExecutionOptions {
                max_total_bytes: 30,
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();

        assert!(
            matches!(err, PluginError::StreamTotalBytesExceeded { limit: 30, .. }),
            "expected StreamTotalBytesExceeded, got: {err}"
        );
    }

    #[test]
    fn run_streaming_plugin_rejects_non_streaming_manifest() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/non-streaming");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_helper(&plugin_dir, "normalize-text", "#!/bin/sh\n");
        let manifest = sample_manifest("example.non-streaming"); // streaming=false by default

        let err = run_streaming_plugin(
            &plugin_dir,
            &manifest,
            "{}",
            &PluginExecutionOptions::default(),
        )
        .unwrap_err();

        assert!(matches!(err, PluginError::NotStreaming));
    }

    // ---- end streaming tests -----------------------------------------------

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            schema_version: CURRENT_PLUGIN_SCHEMA_VERSION,
            id: id.to_owned(),
            name: "Example Normalizer".to_owned(),
            version: "1.0.0".to_owned(),
            license: "MIT".to_owned(),
            entry: vec!["normalize-text".to_owned()],
            classes: vec![PluginClass::Prediffer],
            mime_types: vec!["text/plain".to_owned()],
            extensions: vec!["txt".to_owned(), "log".to_owned()],
            capabilities: vec!["deterministic-output".to_owned()],
            deterministic: true,
            sandbox: PluginSandbox::default(),
            streaming: false,
            options_schema: vec![],
        }
    }

    fn write_manifest(plugin_dir: &Path, manifest: &PluginManifest) {
        let text = serde_json::to_string_pretty(manifest).unwrap();
        fs::write(plugin_dir.join(PLUGIN_MANIFEST_FILE), text).unwrap();
    }

    fn write_helper(plugin_dir: &Path, name: &str, script: &str) {
        let path = plugin_dir.join(name);
        fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
        }
    }

    struct TempFixture {
        path: PathBuf,
    }

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    impl TempFixture {
        fn new() -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_FIXTURE_ID.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "linsync-plugin-test-{}-{suffix}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn plugin_class_deserializes_document_classes() {
        let json = r#"["document_text_extractor","ocr_engine","pdf_renderer"]"#;
        let classes: Vec<PluginClass> = serde_json::from_str(json).unwrap();
        assert_eq!(classes[0], PluginClass::DocumentTextExtractor);
        assert_eq!(classes[1], PluginClass::OcrEngine);
        assert_eq!(classes[2], PluginClass::PdfRenderer);
    }

    #[test]
    fn text_operation_options_serialize_language() {
        let options = PluginTextOperationOptions {
            language: Some("deu".to_owned()),
            ..PluginTextOperationOptions::default()
        };
        let json = serde_json::to_string(&options).unwrap();
        assert!(
            json.contains("\"language\":\"deu\""),
            "expected language in serialized options, got: {json}"
        );

        // Absent language must be omitted (skip_serializing_if).
        let empty = serde_json::to_string(&PluginTextOperationOptions::default()).unwrap();
        assert!(
            !empty.contains("language"),
            "expected no language key when unset, got: {empty}"
        );
    }

    #[test]
    fn operation_request_serializes_language_option() {
        // Mirror the request `run_text_operation` builds and assert the language
        // survives serialization into `PluginOperationRequest.options` — the wire
        // contract the plugin reads as `options.language`.
        let request = PluginOperationRequest {
            protocol_version: CURRENT_PLUGIN_PROTOCOL_VERSION,
            operation: PluginOperation::UnpackText,
            request_id: "linsync-unpack_text-test".to_owned(),
            inputs: vec![PluginInputDescriptor::for_file(
                "source",
                PathBuf::from("/tmp/document.pdf"),
            )],
            options: PluginTextOperationOptions {
                language: Some("fra".to_owned()),
                ..PluginTextOperationOptions::default()
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(
            json.contains("\"options\":{\"language\":\"fra\"}"),
            "expected language inside request options, got: {json}"
        );
    }

    #[test]
    fn streaming_plugin_caps_flood_of_empty_chunks() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/streaming-empty");
        fs::create_dir_all(&plugin_dir).unwrap();
        // Emit many zero-length chunks (header only, no payload). Without
        // counting header overhead these would grow the chunk Vec unboundedly.
        write_helper(
            &plugin_dir,
            "stream-empty.sh",
            r#"#!/usr/bin/env bash
read REQ
i=0
while [ "$i" -lt 1000 ]; do
    printf '\x00\x00\x00\x00'
    i=$((i + 1))
done
"#,
        );
        let mut manifest = sample_manifest("example.streaming-empty");
        manifest.entry = vec!["stream-empty.sh".to_owned()];
        manifest.streaming = true;

        // Cap of 20 bytes allows only ~5 four-byte headers before the cap trips.
        let err = run_streaming_plugin(
            &plugin_dir,
            &manifest,
            "{}\n",
            &PluginExecutionOptions {
                max_total_bytes: 20,
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();

        assert!(
            matches!(err, PluginError::StreamTotalBytesExceeded { limit: 20, .. }),
            "expected StreamTotalBytesExceeded from empty-chunk flood, got: {err}"
        );
    }

    #[test]
    fn helper_output_limit_enforced_during_run() {
        let fixture = TempFixture::new();
        let plugin_dir = fixture.path.join("plugins/flooding");
        fs::create_dir_all(&plugin_dir).unwrap();
        // A long-running helper that floods stdout then sleeps. The host must
        // kill it once the cap is crossed rather than waiting for it to exit.
        write_helper(
            &plugin_dir,
            "flood.sh",
            r#"#!/bin/sh
i=0
while [ "$i" -lt 200 ]; do
    printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'
    i=$((i + 1))
done
sleep 30
"#,
        );
        let mut manifest = sample_manifest("example.flooding");
        manifest.entry = vec!["flood.sh".to_owned()];

        let started = std::time::Instant::now();
        let err = run_plugin_helper(
            &plugin_dir,
            &manifest,
            "{}",
            &PluginExecutionOptions {
                stdout_limit: 64,
                timeout: Duration::from_secs(20),
                ..PluginExecutionOptions::default()
            },
        )
        .unwrap_err();

        // The helper sleeps 30s and the timeout is 20s, so finishing quickly
        // proves the limit was enforced mid-run (not via timeout or exit).
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "expected mid-run kill, but the call took {:?}",
            started.elapsed()
        );
        assert!(
            matches!(
                err,
                PluginError::OutputTooLarge {
                    stream: PluginOutputStream::Stdout,
                    limit: 64,
                    ..
                }
            ),
            "expected OutputTooLarge during run, got: {err}"
        );
    }
}
