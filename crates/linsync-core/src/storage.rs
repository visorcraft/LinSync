use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::{DeserializeOwned, Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_repr::Serialize_repr;

use crate::filter::FileFilter;
use crate::paths::AppPaths;
use crate::text::{CompareSession, CompareSide};
use crate::trash::DeletePreference;

const CURRENT_STORAGE_SCHEMA_VERSION: u32 = 1;

fn current_storage_schema_version() -> u32 {
    CURRENT_STORAGE_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    #[serde(rename = "theme_preference", alias = "theme")]
    pub theme_preference: ThemePreference,
    pub pane_font_size: u8,
    pub pane_font_family: String,
    pub pane_tab_width: u8,
    pub show_line_numbers: bool,
    pub show_whitespace: bool,
    pub word_wrap: bool,
    pub ignore_case: bool,
    pub ignore_whitespace: bool,
    pub ignore_blank_lines: bool,
    pub ignore_eol: bool,
    pub eol_normalization: String,
    pub default_compare_mode: String,
    pub open_last_session: bool,
    pub confirm_on_close: bool,
    pub persist_recent_paths: bool,
    pub recent_limit: usize,
    #[serde(default)]
    pub reduce_motion: bool,
    pub default_recursive_folder_compare: bool,
    #[serde(default)]
    pub detect_moves: bool,
    pub delete_preference: DeletePreference,
    pub confirm_permanent_delete: bool,
    pub window_size: Option<WindowSize>,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default)]
    pub max_walk_depth: u32,
    #[serde(default)]
    pub session_includes: Vec<String>,
    #[serde(default)]
    pub session_excludes: Vec<String>,
    /// Unknown keys from a richer/newer build, preserved verbatim across a
    /// load→save round-trip so an older build never silently drops settings it
    /// does not understand.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme_preference: ThemePreference::System,
            pane_font_size: 12,
            pane_font_family: "monospace".to_owned(),
            pane_tab_width: 4,
            show_line_numbers: true,
            show_whitespace: false,
            word_wrap: false,
            ignore_case: false,
            ignore_whitespace: false,
            ignore_blank_lines: false,
            ignore_eol: true,
            eol_normalization: "auto".to_owned(),
            default_compare_mode: "Text".to_owned(),
            open_last_session: true,
            confirm_on_close: true,
            persist_recent_paths: true,
            recent_limit: 20,
            reduce_motion: false,
            default_recursive_folder_compare: true,
            detect_moves: false,
            delete_preference: DeletePreference::MoveToTrash,
            confirm_permanent_delete: true,
            window_size: None,
            respect_gitignore: true,
            follow_symlinks: false,
            max_walk_depth: 0,
            session_includes: Vec::new(),
            session_excludes: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Settings {
    pub const CURRENT_SCHEMA_VERSION: u32 = CURRENT_STORAGE_SCHEMA_VERSION;

    pub fn validate_current_schema(mut self) -> Result<Self, StoreError> {
        ensure_supported_schema(
            "settings",
            self.schema_version,
            Self::CURRENT_SCHEMA_VERSION,
        )?;
        if let Some(size) = self.window_size
            && (size.width == 0 || size.height == 0)
        {
            return Err(StoreError::InvalidData(
                "settings window_size dimensions must be greater than zero".to_owned(),
            ));
        }
        self.schema_version = Self::CURRENT_SCHEMA_VERSION;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize_repr)]
#[repr(u8)]
pub enum ThemePreference {
    #[default]
    System = 0,
    Light = 1,
    Dark = 2,
    GentleGecko = 3,
    BlackKnight = 4,
    Diamond = 5,
    Dreams = 6,
    Paranoid = 7,
    RedVelvet = 8,
    Subspace = 9,
    Tiefling = 10,
    Vibes = 11,
    OledBlack = 12,
}

impl ThemePreference {
    pub fn from_grex_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::System),
            1 => Some(Self::Light),
            2 => Some(Self::Dark),
            3 => Some(Self::GentleGecko),
            4 => Some(Self::BlackKnight),
            5 => Some(Self::Diamond),
            6 => Some(Self::Dreams),
            7 => Some(Self::Paranoid),
            8 => Some(Self::RedVelvet),
            9 => Some(Self::Subspace),
            10 => Some(Self::Tiefling),
            11 => Some(Self::Vibes),
            12 => Some(Self::OledBlack),
            _ => None,
        }
    }

    pub fn grex_value(self) -> u8 {
        self as u8
    }

    pub fn from_legacy_key(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "oled-black" | "oled_black" => Some(Self::OledBlack),
            "gentle-gecko" | "gentle_gecko" => Some(Self::GentleGecko),
            "black-knight" | "black_knight" | "high-contrast" | "high_contrast" => {
                Some(Self::BlackKnight)
            }
            "diamond" => Some(Self::Diamond),
            "dreams" => Some(Self::Dreams),
            "paranoid" => Some(Self::Paranoid),
            "red-velvet" | "red_velvet" => Some(Self::RedVelvet),
            "subspace" => Some(Self::Subspace),
            "tiefling" => Some(Self::Tiefling),
            "vibes" => Some(Self::Vibes),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for ThemePreference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ThemePreferenceVisitor;

        impl Visitor<'_> for ThemePreferenceVisitor {
            type Value = ThemePreference;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Grex/Grexa theme integer 0..12 or a legacy theme key")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                let value = u8::try_from(value)
                    .map_err(|_| E::custom(format!("unsupported theme preference {value}")))?;
                ThemePreference::from_grex_value(value)
                    .ok_or_else(|| E::custom(format!("unsupported theme preference {value}")))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                let value = u8::try_from(value)
                    .map_err(|_| E::custom(format!("unsupported theme preference {value}")))?;
                ThemePreference::from_grex_value(value)
                    .ok_or_else(|| E::custom(format!("unsupported theme preference {value}")))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                ThemePreference::from_legacy_key(value)
                    .ok_or_else(|| E::custom(format!("unsupported theme preference '{value}'")))
            }
        }

        deserializer.deserialize_any(ThemePreferenceVisitor)
    }
}

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidData(String),
    UnsupportedSchema {
        artifact: &'static str,
        version: u32,
        supported: u32,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::InvalidData(message) => write!(f, "{message}"),
            Self::UnsupportedSchema {
                artifact,
                version,
                supported,
            } => write!(
                f,
                "unsupported {artifact} schema version {version}; supported version is {supported}"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_or_default(&self) -> Result<Settings, StoreError> {
        if !self.path.exists() {
            return Ok(Settings::default());
        }

        let text = fs::read_to_string(&self.path)?;
        let raw: serde_json::Value = serde_json::from_str(&text)?;
        let should_rewrite_theme = raw.get("theme").is_some()
            || raw
                .get("theme_preference")
                .is_some_and(serde_json::Value::is_string);
        let settings = serde_json::from_value::<Settings>(raw)?;
        let original_schema_version = settings.schema_version;
        let settings = settings.validate_current_schema()?;
        if original_schema_version != Settings::CURRENT_SCHEMA_VERSION || should_rewrite_theme {
            self.save(&settings)?;
        }
        Ok(settings)
    }

    pub fn save(&self, settings: &Settings) -> Result<(), StoreError> {
        write_json(&self.path, settings)
    }

    pub fn import_from(&self, source: &Path) -> Result<Settings, StoreError> {
        let settings = read_json::<Settings>(source)?.validate_current_schema()?;
        self.save(&settings)?;
        Ok(settings)
    }

    pub fn export_to(&self, destination: &Path) -> Result<(), StoreError> {
        let settings = self.load_or_default()?;
        write_json(destination, &settings)
    }

    pub fn backup_to(&self, destination: &Path) -> Result<(), StoreError> {
        self.export_to(destination)
    }

    pub fn reset_to_default(&self) -> Result<Settings, StoreError> {
        let settings = Settings::default();
        self.save(&settings)?;
        Ok(settings)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentPaths {
    pub schema_version: u32,
    pub paths: Vec<PathBuf>,
    /// Unknown keys preserved across a load→save round-trip (forward-compat).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for RecentPaths {
    fn default() -> Self {
        Self {
            schema_version: 1,
            paths: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl RecentPaths {
    pub fn validate_current_schema(mut self) -> Result<Self, StoreError> {
        ensure_supported_schema(
            "recent paths",
            self.schema_version,
            CURRENT_STORAGE_SCHEMA_VERSION,
        )?;
        self.schema_version = CURRENT_STORAGE_SCHEMA_VERSION;
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct RecentPathStore {
    path: PathBuf,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamedFilters {
    pub schema_version: u32,
    pub filters: Vec<FileFilter>,
}

impl Default for NamedFilters {
    fn default() -> Self {
        Self {
            schema_version: 1,
            filters: Vec::new(),
        }
    }
}

impl NamedFilters {
    pub fn validate_current_schema(mut self) -> Result<Self, StoreError> {
        ensure_supported_schema(
            "named filters",
            self.schema_version,
            CURRENT_STORAGE_SCHEMA_VERSION,
        )?;
        self.schema_version = CURRENT_STORAGE_SCHEMA_VERSION;
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct FilterStore {
    path: PathBuf,
}

impl FilterStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_or_default(&self) -> Result<NamedFilters, StoreError> {
        if !self.path.exists() {
            return Ok(NamedFilters::default());
        }

        read_json::<NamedFilters>(&self.path)?.validate_current_schema()
    }

    pub fn save(&self, filters: &NamedFilters) -> Result<(), StoreError> {
        write_json(&self.path, filters)
    }

    pub fn upsert(&self, filter: FileFilter) -> Result<NamedFilters, StoreError> {
        let mut filters = self.load_or_default()?;
        if let Some(name) = filter.name.as_deref() {
            filters
                .filters
                .retain(|existing| existing.name.as_deref() != Some(name));
        }
        filters.filters.push(filter);
        self.save(&filters)?;
        Ok(filters)
    }
}

impl RecentPathStore {
    pub fn new(path: PathBuf, limit: usize) -> Self {
        Self { path, limit }
    }

    pub fn load_or_default(&self) -> Result<RecentPaths, StoreError> {
        if !self.path.exists() {
            return Ok(RecentPaths::default());
        }

        read_json::<RecentPaths>(&self.path)?.validate_current_schema()
    }

    pub fn add(&self, path: PathBuf) -> Result<RecentPaths, StoreError> {
        let mut recent = self.load_or_default()?;
        recent.paths.retain(|existing| existing != &path);
        recent.paths.insert(0, path);
        // `max(1)`: a limit of 0 would truncate away the entry we just added,
        // silently discarding the caller's data.
        recent.paths.truncate(self.limit.max(1));
        self.save(&recent)?;
        Ok(recent)
    }

    pub fn remove(&self, path: &Path) -> Result<RecentPaths, StoreError> {
        let mut recent = self.load_or_default()?;
        recent.paths.retain(|existing| existing != path);
        self.save(&recent)?;
        Ok(recent)
    }

    pub fn search(&self, query: &str) -> Result<Vec<PathBuf>, StoreError> {
        let recent = self.load_or_default()?;
        let query = query.to_lowercase();

        Ok(recent
            .paths
            .into_iter()
            .filter(|path| {
                query.is_empty()
                    || path
                        .to_string_lossy()
                        .to_lowercase()
                        .split(['/', '\\'])
                        .any(|segment| segment.starts_with(&query))
                    || path.to_string_lossy().to_lowercase().contains(&query)
            })
            .collect())
    }

    pub fn save(&self, recent: &RecentPaths) -> Result<(), StoreError> {
        write_json(&self.path, recent)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionFile {
    #[serde(default = "current_storage_schema_version")]
    pub schema_version: u32,
    pub session: CompareSession,
    #[serde(default)]
    pub selected_view: CompareViewMode,
    #[serde(default)]
    pub filter_names: Vec<String>,
    /// Compare profile id (built-in or user) this entry was created with, if
    /// any. Clients (e.g. the CLI `project run`/`report`) resolve and apply it
    /// to drive the comparison; `None` means use default options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub layout: SessionLayout,
    /// Unknown keys preserved across a load→save round-trip (forward-compat).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl SessionFile {
    pub fn new(session: CompareSession) -> Self {
        Self {
            schema_version: CURRENT_STORAGE_SCHEMA_VERSION,
            session,
            selected_view: CompareViewMode::default(),
            filter_names: Vec::new(),
            profile: None,
            layout: SessionLayout::default(),
            extra: serde_json::Map::new(),
        }
    }

    pub fn validate_current_schema(mut self) -> Result<Self, StoreError> {
        ensure_supported_schema(
            "session",
            self.schema_version,
            CURRENT_STORAGE_SCHEMA_VERSION,
        )?;
        self.schema_version = CURRENT_STORAGE_SCHEMA_VERSION;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareViewMode {
    #[default]
    Text,
    Folder,
    Binary,
    Table,
    Image,
    Document,
    Archive,
    Webpage,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionLayout {
    pub active_side: Option<CompareSide>,
    pub visible_columns: Vec<String>,
    pub sort_column: Option<String>,
    pub selected_view_state: Option<String>,
    /// Unknown keys preserved across a load→save round-trip (forward-compat).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SessionFileStore {
    path: PathBuf,
}

impl SessionFileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<SessionFile, StoreError> {
        read_json::<SessionFile>(&self.path)?.validate_current_schema()
    }

    pub fn save(&self, session: &SessionFile) -> Result<(), StoreError> {
        write_json(&self.path, session)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    #[serde(default = "current_storage_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub sessions: Vec<SessionFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session_index: Option<usize>,
}

impl ProjectFile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_STORAGE_SCHEMA_VERSION,
            name: name.into(),
            sessions: Vec::new(),
            active_session_index: None,
        }
    }

    pub fn validate_current_schema(mut self) -> Result<Self, StoreError> {
        ensure_supported_schema(
            "project",
            self.schema_version,
            CURRENT_STORAGE_SCHEMA_VERSION,
        )?;

        if let Some(index) = self.active_session_index
            && index >= self.sessions.len()
        {
            return Err(StoreError::InvalidData(format!(
                "project active_session_index {index} is out of range for {} sessions",
                self.sessions.len()
            )));
        }

        self.schema_version = CURRENT_STORAGE_SCHEMA_VERSION;
        self.sessions = self
            .sessions
            .into_iter()
            .map(SessionFile::validate_current_schema)
            .collect::<Result<_, _>>()?;
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct ProjectFileStore {
    path: PathBuf,
}

impl ProjectFileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<ProjectFile, StoreError> {
        let mut project = read_json::<ProjectFile>(&self.path)?.validate_current_schema()?;
        // Resolve any relative session paths against the project file's own
        // directory so a project (saved with relative paths) stays valid after
        // being moved together with its files. Absolute paths are unchanged
        // (Path::join returns an absolute right-hand side as-is).
        if let Some(base) = self.path.parent() {
            for session in &mut project.sessions {
                resolve_session_paths_against(&mut session.session, base);
            }
        }
        Ok(project)
    }

    pub fn save(&self, project: &ProjectFile) -> Result<(), StoreError> {
        write_json(&self.path, project)
    }
}

/// Rewrite a comparison's `left`/`base`/`right` to be absolute by joining them
/// against `base` when they are relative. A no-op for already-absolute paths.
fn resolve_session_paths_against(session: &mut CompareSession, base: &Path) {
    let resolve = |p: &PathBuf| -> PathBuf {
        if p.as_os_str().is_empty() {
            p.clone()
        } else {
            base.join(p)
        }
    };
    session.left = resolve(&session.left);
    session.right = resolve(&session.right);
    if let Some(b) = session.base.as_ref() {
        session.base = Some(resolve(b));
    }
}

/// Rewrite a comparison's `left`/`base`/`right` to be relative to `base` when
/// they currently live under it; paths outside `base` are left absolute.
/// Used when saving a project so it can travel with its directory.
pub fn relativize_session_paths_against(session: &mut CompareSession, base: &Path) {
    let relativize = |p: &PathBuf| -> PathBuf {
        p.strip_prefix(base)
            .map(|rel| rel.to_path_buf())
            .unwrap_or_else(|_| p.clone())
    };
    session.left = relativize(&session.left);
    session.right = relativize(&session.right);
    if let Some(b) = session.base.as_ref() {
        session.base = Some(relativize(b));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentSessions {
    pub schema_version: u32,
    pub sessions: Vec<SessionFile>,
    /// Unknown keys preserved across a load→save round-trip (forward-compat).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for RecentSessions {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_STORAGE_SCHEMA_VERSION,
            sessions: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl RecentSessions {
    pub fn validate_current_schema(mut self) -> Result<Self, StoreError> {
        ensure_supported_schema(
            "recent sessions",
            self.schema_version,
            CURRENT_STORAGE_SCHEMA_VERSION,
        )?;
        self.schema_version = CURRENT_STORAGE_SCHEMA_VERSION;
        self.sessions = self
            .sessions
            .into_iter()
            .map(SessionFile::validate_current_schema)
            .collect::<Result<_, _>>()?;
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct RecentSessionStore {
    path: PathBuf,
    limit: usize,
}

impl RecentSessionStore {
    pub fn new(path: PathBuf, limit: usize) -> Self {
        Self { path, limit }
    }

    pub fn load_or_default(&self) -> Result<RecentSessions, StoreError> {
        if !self.path.exists() {
            return Ok(RecentSessions::default());
        }

        read_json::<RecentSessions>(&self.path)?.validate_current_schema()
    }

    pub fn add(&self, session: SessionFile) -> Result<RecentSessions, StoreError> {
        let mut recent = self.load_or_default()?;
        recent
            .sessions
            .retain(|existing| existing.session != session.session);
        recent.sessions.insert(0, session);
        // `max(1)`: a limit of 0 would truncate away the entry we just added.
        recent.sessions.truncate(self.limit.max(1));
        self.save(&recent)?;
        Ok(recent)
    }

    pub fn save(&self, recent: &RecentSessions) -> Result<(), StoreError> {
        write_json(&self.path, recent)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompareArtifact {
    ImageOverlay {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    ExtractedText {
        path: PathBuf,
        side: String,
        format: String,
    },
    HexDump {
        path: PathBuf,
        side: String,
    },
    ReportFile {
        path: PathBuf,
        format: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtifactManifest {
    pub session_id: Option<String>,
    pub created_at: Option<String>,
    pub artifacts: Vec<CompareArtifact>,
}

static ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn artifact_dir(paths: &AppPaths) -> PathBuf {
    let dir = paths.cache_dir.join("artifacts");
    let _ = fs::create_dir_all(&dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    dir
}

pub fn save_artifact(paths: &AppPaths, name: &str, data: &[u8]) -> io::Result<PathBuf> {
    let dir = artifact_dir(paths);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = format!("{}-{ts}-{counter}", sanitize_artifact_name(name));
    let file_path = dir.join(&file_name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&file_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(data)?;
    file.sync_all()?;
    Ok(file_path)
}

fn sanitize_artifact_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len().max(1));
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('.').trim_matches('_').trim_matches('-');
    if trimmed.is_empty() {
        "artifact".to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub fn cleanup_artifacts(paths: &AppPaths, max_age: Duration) -> io::Result<u64> {
    let dir = artifact_dir(paths);
    let cutoff = SystemTime::now() - max_age;
    let mut removed: u64 = 0;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(0),
    };
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata()
            && metadata.is_file()
            && let Ok(modified) = metadata.modified()
            && modified < cutoff
            && fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

fn ensure_supported_schema(
    artifact: &'static str,
    version: u32,
    supported: u32,
) -> Result<(), StoreError> {
    if version > supported {
        return Err(StoreError::UnsupportedSchema {
            artifact,
            version,
            supported,
        });
    }

    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StoreError> {
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let text = serde_json::to_string_pretty(value)?;

    for attempt in 0..100 {
        let temporary = temporary_json_path(path, attempt);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temporary) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(StoreError::Io(err)),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(err) = file.set_permissions(fs::Permissions::from_mode(0o600)) {
                let _ = fs::remove_file(&temporary);
                return Err(StoreError::Io(err));
            }
        }

        if let Err(err) = file.write_all(text.as_bytes()) {
            let _ = fs::remove_file(&temporary);
            return Err(StoreError::Io(err));
        }
        if let Err(err) = file.sync_all() {
            let _ = fs::remove_file(&temporary);
            return Err(StoreError::Io(err));
        }
        drop(file);

        if let Err(err) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(StoreError::Io(err));
        }
        return Ok(());
    }

    Err(StoreError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create unique temporary settings file",
    )))
}

fn temporary_json_path(path: &Path, attempt: u32) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "settings".into());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(
        ".{file_name}.{}-{now}-{attempt}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn settings_round_trip() {
        let fixture = TempFixture::new();
        let store = SettingsStore::new(fixture.path.join("settings.json"));
        let settings = Settings {
            theme_preference: ThemePreference::Dark,
            reduce_motion: true,
            window_size: Some(WindowSize {
                width: 1280,
                height: 720,
            }),
            ..Settings::default()
        };

        store.save(&settings).unwrap();
        assert_eq!(store.load_or_default().unwrap(), settings);
        let persisted = fs::read_to_string(&store.path).unwrap();
        assert!(persisted.contains(r#""theme_preference": 2"#));
        assert!(persisted.contains(r#""reduce_motion": true"#));
        assert!(persisted.contains(r#""window_size""#));
        assert!(persisted.contains(r#""width": 1280"#));
        assert!(!persisted.contains(r#""x""#));
        assert!(!persisted.contains(r#""y""#));
    }

    #[cfg(unix)]
    #[test]
    fn stored_json_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempFixture::new();
        let settings_path = fixture.path.join("settings.json");
        let recent_path = fixture.path.join("recent-paths.json");

        SettingsStore::new(settings_path.clone())
            .save(&Settings::default())
            .unwrap();
        RecentPathStore::new(recent_path.clone(), 10)
            .add(PathBuf::from("/home/alice/private.txt"))
            .unwrap();

        assert_eq!(
            fs::metadata(settings_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(recent_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn settings_load_migrates_legacy_schema_and_persists_current_schema() {
        let fixture = TempFixture::new();
        let store = SettingsStore::new(fixture.path.join("settings.json"));
        fs::write(
            &store.path,
            r#"{"schema_version":0,"theme":"light","recent_limit":4}"#,
        )
        .unwrap();

        let settings = store.load_or_default().unwrap();

        assert_eq!(settings.schema_version, Settings::CURRENT_SCHEMA_VERSION);
        assert_eq!(settings.theme_preference, ThemePreference::Light);
        assert_eq!(settings.recent_limit, 4);
        assert!(settings.default_recursive_folder_compare);
        let persisted = fs::read_to_string(&store.path).unwrap();
        assert!(persisted.contains(r#""schema_version": 1"#));
        assert!(persisted.contains(r#""theme_preference": 1"#));
        assert!(persisted.contains(r#""default_recursive_folder_compare": true"#));
    }

    #[test]
    fn settings_load_rewrites_legacy_theme_field_at_current_schema() {
        let fixture = TempFixture::new();
        let store = SettingsStore::new(fixture.path.join("settings.json"));
        fs::write(
            &store.path,
            r#"{"schema_version":1,"theme":"oled-black","recent_limit":8}"#,
        )
        .unwrap();

        let settings = store.load_or_default().unwrap();

        assert_eq!(settings.theme_preference, ThemePreference::OledBlack);
        assert_eq!(settings.recent_limit, 8);
        let persisted = fs::read_to_string(&store.path).unwrap();
        assert!(persisted.contains(r#""theme_preference": 12"#));
        assert!(!persisted.contains(r#""theme""#));
    }

    #[test]
    fn settings_import_export_backup_and_reset_defaults() {
        let fixture = TempFixture::new();
        let store = SettingsStore::new(fixture.path.join("settings.json"));
        let backup = fixture.path.join("backup/settings.json");
        let imported = fixture.path.join("imported.json");
        let settings = Settings {
            theme_preference: ThemePreference::BlackKnight,
            recent_limit: 7,
            ..Settings::default()
        };

        store.save(&settings).unwrap();
        store.backup_to(&backup).unwrap();
        store.reset_to_default().unwrap();
        assert_eq!(store.load_or_default().unwrap(), Settings::default());

        fs::copy(&backup, &imported).unwrap();
        assert_eq!(store.import_from(&imported).unwrap(), settings);
        assert_eq!(store.load_or_default().unwrap(), settings);
    }

    #[test]
    fn settings_theme_preferences_cover_grexa_palette_keys() {
        for (theme, value) in [
            (ThemePreference::System, 0),
            (ThemePreference::Light, 1),
            (ThemePreference::Dark, 2),
            (ThemePreference::GentleGecko, 3),
            (ThemePreference::BlackKnight, 4),
            (ThemePreference::Diamond, 5),
            (ThemePreference::Dreams, 6),
            (ThemePreference::Paranoid, 7),
            (ThemePreference::RedVelvet, 8),
            (ThemePreference::Subspace, 9),
            (ThemePreference::Tiefling, 10),
            (ThemePreference::Vibes, 11),
            (ThemePreference::OledBlack, 12),
        ] {
            let json = serde_json::to_string(&theme).unwrap();
            assert_eq!(json, value.to_string());
            assert_eq!(
                serde_json::from_str::<ThemePreference>(&json).unwrap(),
                theme
            );
        }

        assert_eq!(
            serde_json::from_str::<ThemePreference>(r#""high_contrast""#).unwrap(),
            ThemePreference::BlackKnight
        );
        assert_eq!(
            serde_json::from_str::<ThemePreference>(r#""oled-black""#).unwrap(),
            ThemePreference::OledBlack
        );
    }

    #[test]
    fn settings_ignore_unknown_fields_and_reject_bad_input() {
        let fixture = TempFixture::new();
        let store = SettingsStore::new(fixture.path.join("settings.json"));

        fs::write(
            &store.path,
            r#"{"schema_version":1,"theme":"dark","recent_limit":10,"default_recursive_folder_compare":false,"future_field":true}"#,
        )
        .unwrap();
        let settings = store.load_or_default().unwrap();
        assert_eq!(settings.theme_preference, ThemePreference::Dark);
        assert!(!settings.default_recursive_folder_compare);

        fs::write(&store.path, "{not json").unwrap();
        assert!(matches!(store.load_or_default(), Err(StoreError::Json(_))));

        fs::write(
            &store.path,
            r#"{"schema_version":1,"theme":"dark","recent_limit":10,"default_recursive_folder_compare":false,"window_size":{"width":0,"height":720}}"#,
        )
        .unwrap();
        assert!(matches!(
            store.load_or_default(),
            Err(StoreError::InvalidData(message)) if message.contains("window_size")
        ));

        fs::write(
            &store.path,
            r#"{"schema_version":99,"theme":"dark","recent_limit":10,"default_recursive_folder_compare":false}"#,
        )
        .unwrap();
        assert!(matches!(
            store.load_or_default(),
            Err(StoreError::UnsupportedSchema {
                artifact: "settings",
                version: 99,
                supported: 1
            })
        ));
    }

    #[test]
    fn concurrent_settings_writes_remain_valid_json_without_temp_leftovers() {
        let fixture = TempFixture::new();
        let path = fixture.path.join("settings.json");
        let mut handles = Vec::new();

        for worker in 0..12 {
            let path = path.clone();
            handles.push(std::thread::spawn(move || {
                let store = SettingsStore::new(path);
                for iteration in 0..25 {
                    store
                        .save(&Settings {
                            theme_preference: if worker % 2 == 0 {
                                ThemePreference::Light
                            } else {
                                ThemePreference::Dark
                            },
                            recent_limit: worker * 100 + iteration + 1,
                            default_recursive_folder_compare: worker % 3 == 0,
                            ..Settings::default()
                        })
                        .unwrap();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let settings = SettingsStore::new(path.clone()).load_or_default().unwrap();
        assert_eq!(settings.schema_version, Settings::CURRENT_SCHEMA_VERSION);
        assert!(settings.recent_limit > 0);
        let persisted = fs::read_to_string(&path).unwrap();
        serde_json::from_str::<Settings>(&persisted).unwrap();
        let temp_leftovers = fs::read_dir(&fixture.path)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temp_leftovers, 0);
    }

    #[test]
    fn recent_paths_dedupes_and_caps() {
        let fixture = TempFixture::new();
        let store = RecentPathStore::new(fixture.path.join("recent.json"), 2);

        store.add(PathBuf::from("/one")).unwrap();
        store.add(PathBuf::from("/two")).unwrap();
        let recent = store.add(PathBuf::from("/one")).unwrap();

        assert_eq!(
            recent.paths,
            vec![PathBuf::from("/one"), PathBuf::from("/two")]
        );

        let recent = store.add(PathBuf::from("/three")).unwrap();
        assert_eq!(
            recent.paths,
            vec![PathBuf::from("/three"), PathBuf::from("/one")]
        );

        let matches = store.search("thr").unwrap();
        assert_eq!(matches, vec![PathBuf::from("/three")]);

        let recent = store.remove(Path::new("/three")).unwrap();
        assert_eq!(recent.paths, vec![PathBuf::from("/one")]);
    }

    #[test]
    fn named_filters_round_trip_and_upsert_by_name() {
        let fixture = TempFixture::new();
        let store = FilterStore::new(fixture.path.join("filters.json"));
        let first = FileFilter::parse("name: Source\nwf:*.rs").unwrap();
        let replacement = FileFilter::parse("name: Source\nwf:*.toml").unwrap();

        store.upsert(first).unwrap();
        let filters = store.upsert(replacement).unwrap();

        assert_eq!(filters.filters.len(), 1);
        assert_eq!(filters.filters[0].rules[0].pattern, "*.toml");
        assert_eq!(store.load_or_default().unwrap(), filters);
    }

    #[test]
    fn session_file_round_trip_persists_options_filters_and_layout() {
        let fixture = TempFixture::new();
        let store = SessionFileStore::new(fixture.path.join("sessions/compare.linsync-session"));
        let mut session = SessionFile::new(sample_compare_session("Source"));
        session.selected_view = CompareViewMode::Text;
        session.filter_names = vec!["Generated files".to_owned()];
        session.layout.active_side = Some(CompareSide::Right);
        session.layout.visible_columns = vec!["path".to_owned(), "state".to_owned()];
        session.layout.sort_column = Some("path".to_owned());

        store.save(&session).unwrap();
        assert_eq!(store.load().unwrap(), session);
    }

    #[test]
    fn project_file_round_trip_and_active_session_validation() {
        let fixture = TempFixture::new();
        let store = ProjectFileStore::new(fixture.path.join("projects/release.linsync-project"));
        let mut project = ProjectFile::new("Release audit");
        project
            .sessions
            .push(SessionFile::new(sample_compare_session("Main")));
        project
            .sessions
            .push(SessionFile::new(sample_compare_session("Docs")));
        project.active_session_index = Some(1);

        store.save(&project).unwrap();
        assert_eq!(store.load().unwrap(), project);

        project.active_session_index = Some(2);
        store.save(&project).unwrap();
        assert!(matches!(store.load(), Err(StoreError::InvalidData(_))));
    }

    #[test]
    fn project_relative_paths_travel_with_the_directory() {
        let fixture = TempFixture::new();
        let proj_dir = fixture.path.join("workspace");
        let project_file = proj_dir.join("compare.linsync-project");

        // A comparison whose files live under the project directory.
        let mut session = sample_compare_session("Main");
        session.left = proj_dir.join("src/a.txt");
        session.right = proj_dir.join("src/b.txt");

        // Relativize against the project dir, then save.
        relativize_session_paths_against(&mut session, &proj_dir);
        assert_eq!(
            session.left,
            PathBuf::from("src/a.txt"),
            "stored path is relative"
        );
        let mut project = ProjectFile::new("Portable");
        project.sessions.push(SessionFile::new(session));
        ProjectFileStore::new(project_file.clone())
            .save(&project)
            .unwrap();

        // Loading from the original location resolves back to absolute.
        let loaded = ProjectFileStore::new(project_file.clone()).load().unwrap();
        assert_eq!(loaded.sessions[0].session.left, proj_dir.join("src/a.txt"));

        // Move the whole project directory; relative paths still resolve.
        let moved_dir = fixture.path.join("relocated");
        fs::create_dir_all(&moved_dir).unwrap();
        let moved_file = moved_dir.join("compare.linsync-project");
        fs::copy(&project_file, &moved_file).unwrap();
        let relocated = ProjectFileStore::new(moved_file).load().unwrap();
        assert_eq!(
            relocated.sessions[0].session.left,
            moved_dir.join("src/a.txt"),
            "relative paths resolve against the project file's new location"
        );

        // A path outside the project dir stays absolute through relativize.
        let mut outside = sample_compare_session("Outside");
        outside.left = PathBuf::from("/etc/hosts");
        relativize_session_paths_against(&mut outside, &proj_dir);
        assert_eq!(outside.left, PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn recent_sessions_dedupes_and_caps() {
        let fixture = TempFixture::new();
        let store = RecentSessionStore::new(fixture.path.join("recent-sessions.json"), 2);
        let one = SessionFile::new(sample_compare_session("One"));
        let two = SessionFile::new(sample_compare_session("Two"));
        let three = SessionFile::new(sample_compare_session("Three"));

        store.add(one.clone()).unwrap();
        store.add(two.clone()).unwrap();
        let recent = store.add(one.clone()).unwrap();
        assert_eq!(
            recent
                .sessions
                .iter()
                .map(|session| session.session.title.as_str())
                .collect::<Vec<_>>(),
            vec!["One", "Two"]
        );

        let recent = store.add(three.clone()).unwrap();
        assert_eq!(
            recent
                .sessions
                .iter()
                .map(|session| session.session.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Three", "One"]
        );
    }

    fn sample_compare_session(title: &str) -> CompareSession {
        let slug = title.to_lowercase();
        CompareSession {
            title: title.to_owned(),
            left: PathBuf::from(format!("/left/{slug}.txt")),
            base: None,
            right: PathBuf::from(format!("/right/{slug}.txt")),
            options: crate::text::CompareOptions {
                text: crate::text::TextCompareOptions {
                    ignore_case: true,
                    ignore_whitespace: true,
                    ignore_eol: false,
                    ignore_blank_lines: true,
                    ignore_line_patterns: vec![r"^Generated:".to_owned()],
                    substitutions: vec![crate::text::TextSubstitution {
                        pattern: r"id=\d+".to_owned(),
                        replacement: "id=<id>".to_owned(),
                    }],
                    ..crate::text::TextCompareOptions::default()
                },
            },
        }
    }

    #[test]
    fn save_artifact_creates_file() {
        let fixture = TempFixture::new();
        let paths = crate::paths::AppPaths::from_base_dirs(
            fixture.path.join("config"),
            fixture.path.join("data"),
            fixture.path.join("cache"),
            fixture.path.join("state"),
        );
        let data = b"overlay-png-bytes";
        let saved = save_artifact(&paths, "overlay", data).unwrap();
        assert!(saved.exists());
        assert!(saved.starts_with(paths.cache_dir.join("artifacts")));
        assert_eq!(fs::read(&saved).unwrap(), data);
        assert!(
            saved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("overlay-")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = fs::metadata(paths.cache_dir.join("artifacts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            let file_mode = fs::metadata(&saved).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
            assert_eq!(file_mode, 0o600);
        }
    }

    #[test]
    fn save_artifact_sanitizes_names_inside_artifact_dir() {
        let fixture = TempFixture::new();
        let paths = crate::paths::AppPaths::from_base_dirs(
            fixture.path.join("config"),
            fixture.path.join("data"),
            fixture.path.join("cache"),
            fixture.path.join("state"),
        );
        let saved = save_artifact(&paths, "../escape/report", b"private").unwrap();
        assert!(saved.starts_with(paths.cache_dir.join("artifacts")));
        assert!(!saved.file_name().unwrap().to_string_lossy().contains('/'));
        assert!(!paths.cache_dir.join("escape").exists());
    }

    #[test]
    fn cleanup_artifacts_removes_old() {
        let fixture = TempFixture::new();
        let paths = crate::paths::AppPaths::from_base_dirs(
            fixture.path.join("config"),
            fixture.path.join("data"),
            fixture.path.join("cache"),
            fixture.path.join("state"),
        );
        let dir = artifact_dir(&paths);
        let old_path = dir.join("stale-file");
        fs::write(&old_path, b"old").unwrap();
        {
            let f = fs::File::open(&old_path).unwrap();
            let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
            let _ = f.set_modified(two_hours_ago);
        }

        let new_path = dir.join("fresh-file");
        fs::write(&new_path, b"new").unwrap();

        let removed = cleanup_artifacts(&paths, Duration::from_secs(3600)).unwrap();
        assert_eq!(removed, 1);
        assert!(!old_path.exists());
        assert!(new_path.exists());
    }

    #[test]
    fn artifact_manifest_serialization() {
        let manifest = ArtifactManifest {
            session_id: Some("sess-123".to_owned()),
            created_at: Some("2026-01-01T00:00:00Z".to_owned()),
            artifacts: vec![
                CompareArtifact::ImageOverlay {
                    path: PathBuf::from("/tmp/overlay.png"),
                    width: 800,
                    height: 600,
                },
                CompareArtifact::ExtractedText {
                    path: PathBuf::from("/tmp/left.txt"),
                    side: "left".to_owned(),
                    format: "plain".to_owned(),
                },
                CompareArtifact::HexDump {
                    path: PathBuf::from("/tmp/hex-left.bin"),
                    side: "left".to_owned(),
                },
                CompareArtifact::ReportFile {
                    path: PathBuf::from("/tmp/report.json"),
                    format: "json".to_owned(),
                },
            ],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let roundtrip: ArtifactManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, manifest);

        let empty: ArtifactManifest = serde_json::from_str("{}").unwrap();
        assert!(empty.session_id.is_none());
        assert!(empty.artifacts.is_empty());
    }

    struct TempFixture {
        path: PathBuf,
    }

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    impl TempFixture {
        fn new() -> Self {
            let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "linsync-storage-test-{}-{}-{sequence}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
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
}
