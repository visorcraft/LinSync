//! Compare profiles — named bundles of per-mode comparison options.
//!
//! A `CompareProfile` aggregates the option structs for every supported
//! compare mode (text, folder, table, binary, image, document, webpage)
//! into a single, schema-versioned, persistable record. Profiles are the
//! source of truth for "how should LinSync compare things" across the
//! CLI, the HTTP bridge, and the cxx-qt bridge. They replace the
//! previous status quo where each surface invoked a `*Options::default()`
//! independently.
//!
//! ## On-disk layout
//!
//! - One JSON file per profile under `$XDG_CONFIG_HOME/linsync/profiles/`.
//! - An `active-profile.json` pointer file recording which profile id is
//!   currently selected for new compares.
//!
//! ## Built-in vs. user profiles
//!
//! Built-in profiles (added in a follow-up commit) ship with the binary
//! and cannot be deleted or overwritten by the user. User-defined
//! profiles live in the same directory; saving a profile whose `id`
//! matches a built-in is rejected by [`ProfileStore::save`].
//!
//! ## Image / document feature flags
//!
//! The `image` and `document` fields are feature-gated on the same
//! Cargo features that gate the underlying option structs. Builds
//! without those features omit the field entirely from both the in-
//! memory struct and the serialized JSON. Readers that load a profile
//! written by a richer build silently drop unknown fields via
//! `#[serde(default)]` semantics.

pub mod builtin;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::binary::BinaryCompareOptions;
use crate::document::DocumentCompareOptions;
use crate::folder::FolderCompareOptions;
use crate::image::ImageCompareOptions;
use crate::table::TableCompareOptions;
use crate::text::TextCompareOptions;
use crate::webpage::WebpageCompareOptions;

/// Bump when the on-disk profile shape changes in a way old readers
/// can't safely consume. `ProfileStore::load` upgrades older versions
/// to this version on read; if a profile carries a newer
/// `schema_version` the store refuses to load it.
pub(crate) const CURRENT_PROFILE_SCHEMA_VERSION: u32 = 1;

/// Stable identifier for a profile, used in file names and active-profile
/// pointers. Must be ASCII kebab-case (`[a-z0-9-]+`) and non-empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProfileValidationError> {
        let raw = id.into();
        validate_profile_id(&raw)?;
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_profile_id(id: &str) -> Result<(), ProfileValidationError> {
    if id.is_empty() {
        return Err(ProfileValidationError::EmptyId);
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ProfileValidationError::InvalidIdChars(id.to_owned()));
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(ProfileValidationError::InvalidIdShape(id.to_owned()));
    }
    Ok(())
}

/// A complete, named bundle of per-mode compare options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompareProfile {
    pub schema_version: u32,
    pub id: ProfileId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// True iff this profile is a built-in shipped with LinSync. User
    /// profiles set this to `false`; saving a user profile whose `id`
    /// matches a reserved built-in id is rejected by
    /// [`ProfileStore::save`] when a reserved-id set has been registered.
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub text: TextCompareOptions,
    #[serde(default)]
    pub folder: FolderCompareOptions,
    #[serde(default)]
    pub table: TableCompareOptions,
    #[serde(default)]
    pub binary: BinaryCompareOptions,
    #[serde(default)]
    pub image: ImageCompareOptions,
    #[serde(default)]
    pub document: DocumentCompareOptions,
    #[serde(default)]
    pub webpage: WebpageCompareOptions,
    /// Per-profile plugin enable/disable overrides, keyed by plugin id. An
    /// entry here wins over the global `plugins.json` enabled map, which in turn
    /// wins over the default (enabled). Plugin ids are globally unique, so a
    /// single id-keyed map covers every plugin class. Empty for built-in
    /// profiles and for any profile that does not override anything; omitted
    /// from the JSON when empty so existing profiles round-trip unchanged.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub plugin_enablement: std::collections::BTreeMap<String, bool>,
    /// Catch-all for fields produced by a richer build (e.g. a GUI
    /// build that enabled `image-compare` and `document-compare`) so
    /// that round-tripping a profile through a slimmer build does not
    /// silently drop those sections. Never edited directly; deserialize
    /// collects unknown keys here and serialize re-emits them.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl CompareProfile {
    /// Construct a fresh user profile with default options for every mode.
    pub fn new(id: ProfileId, name: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_PROFILE_SCHEMA_VERSION,
            id,
            name: name.into(),
            description: String::new(),
            builtin: false,
            text: TextCompareOptions::default(),
            folder: FolderCompareOptions::default(),
            table: TableCompareOptions::default(),
            binary: BinaryCompareOptions::default(),
            image: ImageCompareOptions::default(),
            document: DocumentCompareOptions::default(),
            webpage: WebpageCompareOptions::default(),
            plugin_enablement: std::collections::BTreeMap::new(),
            extra: serde_json::Map::new(),
        }
    }

    /// Validate the entire profile. Run before save and after load.
    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        if self.schema_version > CURRENT_PROFILE_SCHEMA_VERSION {
            return Err(ProfileValidationError::FutureSchemaVersion {
                profile: self.schema_version,
                supported: CURRENT_PROFILE_SCHEMA_VERSION,
            });
        }
        validate_profile_id(self.id.as_str())?;
        if self.name.trim().is_empty() {
            return Err(ProfileValidationError::EmptyName);
        }
        Ok(())
    }
}

/// Schema-version 0 profile: the original on-disk shape before
/// `schema_version` was introduced. Missing per-mode option sections are
/// filled with their current defaults on migration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct LegacyProfileV0 {
    #[serde(default)]
    id: Option<ProfileId>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    builtin: bool,
    #[serde(default)]
    text: TextCompareOptions,
    #[serde(default)]
    folder: FolderCompareOptions,
    #[serde(default)]
    table: TableCompareOptions,
    #[serde(default)]
    binary: BinaryCompareOptions,
    #[serde(default)]
    image: ImageCompareOptions,
    #[serde(default)]
    document: DocumentCompareOptions,
    #[serde(default)]
    webpage: WebpageCompareOptions,
    #[serde(default)]
    plugin_enablement: std::collections::BTreeMap<String, bool>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl From<LegacyProfileV0> for CompareProfile {
    fn from(legacy: LegacyProfileV0) -> Self {
        let mut profile = Self {
            schema_version: CURRENT_PROFILE_SCHEMA_VERSION,
            id: legacy
                .id
                .unwrap_or_else(|| ProfileId::new("legacy").unwrap()),
            name: legacy.name,
            description: legacy.description,
            builtin: legacy.builtin,
            text: legacy.text,
            folder: legacy.folder,
            table: legacy.table,
            binary: legacy.binary,
            image: legacy.image,
            document: legacy.document,
            webpage: legacy.webpage,
            plugin_enablement: legacy.plugin_enablement,
            extra: legacy.extra,
        };
        // The migrated profile carries the current schema version in its
        // dedicated field; do not keep the old value in the catch-all map.
        profile.extra.remove("schema_version");
        profile
    }
}

impl CompareProfile {
    /// Parse a profile from its raw JSON representation, upgrading older
    /// schema versions to the current one. Returns an error if the JSON is
    /// malformed or if the profile declares a future schema version.
    pub(crate) fn migrate_from_legacy(
        raw: serde_json::Value,
    ) -> Result<Self, ProfileMigrationError> {
        let version = parse_schema_version(&raw)?;

        if version > CURRENT_PROFILE_SCHEMA_VERSION {
            return Err(ProfileMigrationError::Parse {
                message: format!(
                    "profile schema_version {version} is newer than the supported version {CURRENT_PROFILE_SCHEMA_VERSION}; upgrade LinSync"
                ),
            });
        }

        let profile = if version == 0 {
            serde_json::from_value::<LegacyProfileV0>(raw)
                .map_err(|err| ProfileMigrationError::Parse {
                    message: err.to_string(),
                })?
                .into()
        } else {
            serde_json::from_value::<CompareProfile>(raw).map_err(|err| {
                ProfileMigrationError::Parse {
                    message: err.to_string(),
                }
            })?
        };

        profile
            .validate()
            .map_err(ProfileMigrationError::Validation)?;
        Ok(profile)
    }
}

fn parse_schema_version(raw: &serde_json::Value) -> Result<u32, ProfileMigrationError> {
    let Some(value) = raw.get("schema_version") else {
        return Ok(0);
    };
    let Some(version) = value.as_u64() else {
        return Err(ProfileMigrationError::Parse {
            message: "profile schema_version must be a non-negative integer".to_owned(),
        });
    };
    if version > u64::from(u32::MAX) {
        return Err(ProfileMigrationError::Parse {
            message: format!("profile schema_version {version} is too large to read"),
        });
    }
    Ok(version as u32)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileValidationError {
    EmptyId,
    InvalidIdChars(String),
    InvalidIdShape(String),
    EmptyName,
    FutureSchemaVersion { profile: u32, supported: u32 },
}

impl std::fmt::Display for ProfileValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyId => write!(f, "profile id cannot be empty"),
            Self::InvalidIdChars(id) => {
                write!(f, "profile id {id:?} must be ASCII kebab-case ([a-z0-9-]+)")
            }
            Self::InvalidIdShape(id) => {
                write!(f, "profile id {id:?} cannot start or end with a hyphen")
            }
            Self::EmptyName => write!(f, "profile name cannot be empty"),
            Self::FutureSchemaVersion { profile, supported } => write!(
                f,
                "profile schema_version {profile} is newer than the supported version {supported}; upgrade LinSync"
            ),
        }
    }
}

impl std::error::Error for ProfileValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileMigrationError {
    Parse { message: String },
    Validation(ProfileValidationError),
}

impl std::fmt::Display for ProfileMigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { message } => write!(f, "profile migration failed: {message}"),
            Self::Validation(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ProfileMigrationError {}

/// Records the currently-selected profile id. Stored at
/// `AppPaths::active_profile_pointer_file()`. Future versions may grow
/// additional fields; readers tolerate them via `serde(default)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ActiveProfilePointer {
    pub schema_version: u32,
    pub profile_id: ProfileId,
}

impl ActiveProfilePointer {
    pub fn new(profile_id: ProfileId) -> Self {
        Self {
            schema_version: CURRENT_PROFILE_SCHEMA_VERSION,
            profile_id,
        }
    }
}

/// Errors returned by [`ProfileStore`].
#[derive(Debug)]
pub enum ProfileStoreError {
    Io(io::Error),
    Validation(ProfileValidationError),
    Parse { path: PathBuf, message: String },
    NotFound(ProfileId),
    RefusesToOverwriteBuiltin(ProfileId),
}

impl std::fmt::Display for ProfileStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Validation(err) => write!(f, "validation error: {err}"),
            Self::Parse { path, message } => {
                write!(f, "parse error in {}: {message}", path.display())
            }
            Self::NotFound(id) => write!(f, "profile {id:?} not found"),
            Self::RefusesToOverwriteBuiltin(id) => write!(
                f,
                "profile {id:?} is a built-in; built-ins cannot be overwritten"
            ),
        }
    }
}

impl std::error::Error for ProfileStoreError {}

impl From<io::Error> for ProfileStoreError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<ProfileValidationError> for ProfileStoreError {
    fn from(err: ProfileValidationError) -> Self {
        Self::Validation(err)
    }
}

/// Persistence layer for user-defined profiles. Built-ins are returned
/// from a separate registry (see the built-in profiles module) so the
/// store only ever reads / writes files in `profiles_dir`.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    profiles_dir: PathBuf,
    active_pointer: PathBuf,
    reserved_ids: Vec<ProfileId>,
}

impl ProfileStore {
    pub fn new(profiles_dir: PathBuf, active_pointer: PathBuf) -> Self {
        Self {
            profiles_dir,
            active_pointer,
            reserved_ids: Vec::new(),
        }
    }

    /// Register the ids of built-in profiles that user-defined profiles
    /// must not shadow. Call this once at startup with the ids returned
    /// from the built-in registry.
    pub fn with_reserved_ids(mut self, ids: impl IntoIterator<Item = ProfileId>) -> Self {
        self.reserved_ids = ids.into_iter().collect();
        self
    }

    /// Convenience constructor that builds a store and registers every
    /// shipped built-in id as reserved. Equivalent to
    /// `ProfileStore::new(...).with_reserved_ids(builtin::builtin_profile_ids())`.
    /// Use this from app startup so production callers never forget to
    /// reserve the built-in ids.
    pub fn with_builtins(profiles_dir: PathBuf, active_pointer: PathBuf) -> Self {
        Self::new(profiles_dir, active_pointer).with_reserved_ids(builtin::builtin_profile_ids())
    }

    /// List user-defined profile ids (does not include built-ins).
    pub fn list_user_ids(&self) -> Result<Vec<ProfileId>, ProfileStoreError> {
        if !self.profiles_dir.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Ok(id) = ProfileId::new(stem.to_owned()) {
                ids.push(id);
            }
        }
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(ids)
    }

    pub fn load(&self, id: &ProfileId) -> Result<CompareProfile, ProfileStoreError> {
        let path = self.profiles_dir.join(format!("{id}.json"));
        if !path.exists() {
            return Err(ProfileStoreError::NotFound(id.clone()));
        }
        let bytes = fs::read(&path)?;
        let raw: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|err| ProfileStoreError::Parse {
                path: path.clone(),
                message: err.to_string(),
            })?;
        let mut profile = CompareProfile::migrate_from_legacy(raw).map_err(|err| match err {
            ProfileMigrationError::Validation(v) => ProfileStoreError::Validation(v),
            ProfileMigrationError::Parse { message } => ProfileStoreError::Parse {
                path: path.clone(),
                message,
            },
        })?;
        // The filename is the source of truth for a profile's id: every
        // write/lookup keys off it. A file body declaring a mismatched id
        // (hand-edited, copied, or imported) is self-healed here so the
        // returned profile is consistent with where it was loaded from.
        profile.id = id.clone();
        profile.validate()?;
        Ok(profile)
    }

    pub fn save(&self, profile: &CompareProfile) -> Result<(), ProfileStoreError> {
        profile.validate()?;
        if profile.builtin || self.reserved_ids.contains(&profile.id) {
            return Err(ProfileStoreError::RefusesToOverwriteBuiltin(
                profile.id.clone(),
            ));
        }
        fs::create_dir_all(&self.profiles_dir)?;
        let path = self.profiles_dir.join(format!("{}.json", profile.id));
        let bytes = serde_json::to_vec_pretty(profile).map_err(|err| ProfileStoreError::Parse {
            path: path.clone(),
            message: err.to_string(),
        })?;
        write_atomic(&path, &bytes)
    }

    pub fn delete(&self, id: &ProfileId) -> Result<(), ProfileStoreError> {
        let path = self.profiles_dir.join(format!("{id}.json"));
        if !path.exists() {
            return Err(ProfileStoreError::NotFound(id.clone()));
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    pub fn load_active_pointer(&self) -> Result<Option<ProfileId>, ProfileStoreError> {
        if !self.active_pointer.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.active_pointer)?;
        let pointer: ActiveProfilePointer =
            serde_json::from_slice(&bytes).map_err(|err| ProfileStoreError::Parse {
                path: self.active_pointer.clone(),
                message: err.to_string(),
            })?;
        Ok(Some(pointer.profile_id))
    }

    pub fn save_active_pointer(&self, id: &ProfileId) -> Result<(), ProfileStoreError> {
        if let Some(parent) = self.active_pointer.parent() {
            fs::create_dir_all(parent)?;
        }
        let pointer = ActiveProfilePointer::new(id.clone());
        let bytes =
            serde_json::to_vec_pretty(&pointer).map_err(|err| ProfileStoreError::Parse {
                path: self.active_pointer.clone(),
                message: err.to_string(),
            })?;
        write_atomic(&self.active_pointer, &bytes)
    }

    /// Remove the active-profile pointer file, reverting the selection to the
    /// built-in `default`. Idempotent: succeeds silently when no pointer exists.
    ///
    /// Used to clear a stale pointer once (e.g. when the selected profile has
    /// been deleted) so the per-request resolver stops falling back — and
    /// warning — on every request.
    pub fn clear_active_pointer(&self) -> Result<(), ProfileStoreError> {
        if self.active_pointer.exists() {
            fs::remove_file(&self.active_pointer)?;
        }
        Ok(())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProfileStoreError> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, ProfileStore) {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::new(
            dir.path().join("profiles"),
            dir.path().join("active-profile.json"),
        );
        (dir, store)
    }

    #[test]
    fn profile_id_accepts_kebab_case() {
        assert!(ProfileId::new("default").is_ok());
        assert!(ProfileId::new("code-review").is_ok());
        assert!(ProfileId::new("ignore-formatting-2").is_ok());
    }

    #[test]
    fn profile_id_rejects_bad_input() {
        assert!(ProfileId::new("").is_err());
        assert!(ProfileId::new("UPPER").is_err());
        assert!(ProfileId::new("with_underscore").is_err());
        assert!(ProfileId::new("with space").is_err());
        assert!(ProfileId::new("-leading").is_err());
        assert!(ProfileId::new("trailing-").is_err());
    }

    #[test]
    fn validate_rejects_future_schema() {
        let mut p = CompareProfile::new(ProfileId::new("user1").unwrap(), "User 1");
        p.schema_version = CURRENT_PROFILE_SCHEMA_VERSION + 1;
        assert!(matches!(
            p.validate(),
            Err(ProfileValidationError::FutureSchemaVersion { .. })
        ));
    }

    #[test]
    fn round_trip_through_store() {
        let (_dir, store) = temp_store();
        let mut p = CompareProfile::new(ProfileId::new("user1").unwrap(), "User 1");
        p.description = "Test profile".into();
        p.text.ignore_case = true;
        store.save(&p).unwrap();

        let loaded = store.load(&p.id).unwrap();
        assert_eq!(loaded.name, "User 1");
        assert_eq!(loaded.description, "Test profile");
        assert!(loaded.text.ignore_case);
    }

    #[test]
    fn round_trip_preserves_prediffer_plugins() {
        let (_dir, store) = temp_store();
        let mut p = CompareProfile::new(ProfileId::new("pred").unwrap(), "Prediffer");
        p.text.prediffer_plugins = vec!["org.example.normalize".into(), "org.example.strip".into()];
        store.save(&p).unwrap();

        let loaded = store.load(&p.id).unwrap();
        assert_eq!(
            loaded.text.prediffer_plugins,
            vec![
                "org.example.normalize".to_string(),
                "org.example.strip".to_string()
            ],
            "a profile must carry its prediffer plugin ids across save/load"
        );
    }

    #[test]
    fn round_trip_preserves_plugin_enablement() {
        let (_dir, store) = temp_store();
        let mut p = CompareProfile::new(ProfileId::new("enable").unwrap(), "Enablement");
        p.plugin_enablement
            .insert("org.example.normalize".into(), false);
        p.plugin_enablement.insert("org.example.unzip".into(), true);
        store.save(&p).unwrap();

        let loaded = store.load(&p.id).unwrap();
        assert_eq!(
            loaded.plugin_enablement.get("org.example.normalize"),
            Some(&false),
            "a per-profile plugin override must survive save/load"
        );
        assert_eq!(
            loaded.plugin_enablement.get("org.example.unzip"),
            Some(&true)
        );
    }

    #[test]
    fn empty_plugin_enablement_is_omitted_from_json() {
        let p = CompareProfile::new(ProfileId::new("empty").unwrap(), "Empty");
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            !json.contains("plugin_enablement"),
            "an empty override map must not be serialized so existing profiles round-trip unchanged"
        );
    }

    #[test]
    fn save_refuses_to_overwrite_builtin() {
        let (_dir, store) = temp_store();
        let mut p = CompareProfile::new(ProfileId::new("default").unwrap(), "Default");
        p.builtin = true;
        let err = store.save(&p).unwrap_err();
        assert!(matches!(
            err,
            ProfileStoreError::RefusesToOverwriteBuiltin(_)
        ));
    }

    #[test]
    fn list_user_ids_skips_unrelated_files() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(store.profiles_dir.join("user-a.json"), b"{}").unwrap();
        fs::write(store.profiles_dir.join("user-b.json"), b"{}").unwrap();
        fs::write(store.profiles_dir.join("notes.txt"), b"ignored").unwrap();
        fs::write(store.profiles_dir.join("INVALID_NAME.json"), b"{}").unwrap();
        let ids = store.list_user_ids().unwrap();
        assert_eq!(
            ids.iter().map(ProfileId::as_str).collect::<Vec<_>>(),
            vec!["user-a", "user-b"]
        );
        drop(dir);
    }

    #[test]
    fn active_pointer_round_trips() {
        let (_dir, store) = temp_store();
        assert!(store.load_active_pointer().unwrap().is_none());
        let id = ProfileId::new("user1").unwrap();
        store.save_active_pointer(&id).unwrap();
        assert_eq!(store.load_active_pointer().unwrap(), Some(id));
    }

    #[test]
    fn clear_active_pointer_removes_the_selection() {
        let (_dir, store) = temp_store();
        store
            .save_active_pointer(&ProfileId::new("user1").unwrap())
            .unwrap();
        assert!(store.load_active_pointer().unwrap().is_some());
        store.clear_active_pointer().unwrap();
        assert!(store.load_active_pointer().unwrap().is_none());
    }

    #[test]
    fn clear_active_pointer_is_idempotent_when_absent() {
        let (_dir, store) = temp_store();
        // No pointer written yet — clearing must succeed silently.
        store.clear_active_pointer().unwrap();
        assert!(store.load_active_pointer().unwrap().is_none());
    }

    #[test]
    fn missing_profile_returns_not_found() {
        let (_dir, store) = temp_store();
        let err = store.load(&ProfileId::new("nope").unwrap()).unwrap_err();
        assert!(matches!(err, ProfileStoreError::NotFound(_)));
    }

    #[test]
    fn save_refuses_reserved_id_even_when_user_marked() {
        let (_dir, base_store) = temp_store();
        let reserved = ProfileId::new("default").unwrap();
        let store = base_store.with_reserved_ids([reserved.clone()]);
        let user = CompareProfile::new(reserved, "Custom Default");
        // builtin: false on the profile, but the id matches a reserved built-in.
        let err = store.save(&user).unwrap_err();
        assert!(matches!(
            err,
            ProfileStoreError::RefusesToOverwriteBuiltin(_)
        ));
    }

    #[test]
    fn with_builtins_reserves_every_builtin_id() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_builtins(
            dir.path().join("profiles"),
            dir.path().join("active-profile.json"),
        );
        // Trying to save under any built-in id must fail.
        for id in builtin::builtin_profile_ids() {
            let p = CompareProfile::new(id.clone(), "Custom");
            assert!(
                matches!(
                    store.save(&p),
                    Err(ProfileStoreError::RefusesToOverwriteBuiltin(_))
                ),
                "expected builtin id {id} to be reserved"
            );
        }
        // A non-builtin id still saves fine.
        let user_id = ProfileId::new("my-profile").unwrap();
        store
            .save(&CompareProfile::new(user_id, "Mine"))
            .expect("non-builtin id should save");
    }

    #[test]
    fn delete_removes_a_saved_profile() {
        let (_dir, store) = temp_store();
        let p = CompareProfile::new(ProfileId::new("user1").unwrap(), "User 1");
        store.save(&p).unwrap();
        store.delete(&p.id).unwrap();
        assert!(matches!(
            store.load(&p.id).unwrap_err(),
            ProfileStoreError::NotFound(_)
        ));
    }

    #[test]
    fn load_tolerates_missing_optional_fields() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        // Minimal JSON: only the required id, name, schema_version. No
        // description, no builtin, no per-mode options.
        let path = store.profiles_dir.join("minimal.json");
        fs::write(
            &path,
            br#"{"schema_version": 1, "id": "minimal", "name": "Minimal"}"#,
        )
        .unwrap();
        let loaded = store.load(&ProfileId::new("minimal").unwrap()).unwrap();
        assert_eq!(loaded.name, "Minimal");
        assert!(loaded.description.is_empty());
        assert!(!loaded.builtin);
        // Default options should be populated.
        assert_eq!(loaded.text, TextCompareOptions::default());
        drop(dir);
    }

    #[test]
    fn load_overrides_mismatched_body_id_with_filename() {
        // The filename is the source of truth. A file body declaring a
        // different id must load as the requested (filename) id so every
        // subsequent write/lookup stays consistent.
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(
            store.profiles_dir.join("user-a.json"),
            br#"{"schema_version": 1, "id": "user-b", "name": "Mismatch"}"#,
        )
        .unwrap();
        let requested = ProfileId::new("user-a").unwrap();
        let loaded = store.load(&requested).unwrap();
        assert_eq!(loaded.id, requested);
        assert_eq!(loaded.name, "Mismatch");
        drop(dir);
    }

    #[test]
    fn unknown_top_level_fields_round_trip_through_extra() {
        // Simulate a profile written by a richer build that included a
        // hypothetical future "video" section. A slimmer build that does
        // not recognise "video" must preserve it through load + save so
        // the data is not silently dropped.
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        let raw = br#"{
            "schema_version": 1,
            "id": "with-future-field",
            "name": "With Future",
            "video": {"codec": "av1"}
        }"#;
        fs::write(store.profiles_dir.join("with-future-field.json"), raw).unwrap();
        let loaded = store
            .load(&ProfileId::new("with-future-field").unwrap())
            .unwrap();
        assert!(loaded.extra.contains_key("video"));
        store.save(&loaded).unwrap();
        let reread = fs::read_to_string(store.profiles_dir.join("with-future-field.json")).unwrap();
        assert!(
            reread.contains(r#""video""#),
            "expected unknown field preserved through round-trip; got: {reread}"
        );
        drop(dir);
    }

    #[test]
    fn migrate_schema_zero_fills_defaults() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(
            store.profiles_dir.join("legacy.json"),
            br#"{"name": "Legacy"}"#,
        )
        .unwrap();
        let loaded = store.load(&ProfileId::new("legacy").unwrap()).unwrap();
        assert_eq!(loaded.name, "Legacy");
        assert_eq!(loaded.schema_version, CURRENT_PROFILE_SCHEMA_VERSION);
        assert!(loaded.description.is_empty());
        assert!(!loaded.builtin);
        assert_eq!(loaded.text, TextCompareOptions::default());
        assert_eq!(loaded.folder, FolderCompareOptions::default());
        drop(dir);
    }

    #[test]
    fn migrate_schema_zero_validation_error() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(
            store.profiles_dir.join("legacy-empty.json"),
            br#"{"name": ""}"#,
        )
        .unwrap();
        let err = store
            .load(&ProfileId::new("legacy-empty").unwrap())
            .unwrap_err();
        assert!(matches!(
            err,
            ProfileStoreError::Validation(ProfileValidationError::EmptyName)
        ));
        drop(dir);
    }

    #[test]
    fn migrate_schema_zero_does_not_rewrite_file() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(
            store.profiles_dir.join("legacy.json"),
            br#"{"name": "Legacy"}"#,
        )
        .unwrap();
        let loaded = store.load(&ProfileId::new("legacy").unwrap()).unwrap();
        assert_eq!(loaded.schema_version, CURRENT_PROFILE_SCHEMA_VERSION);
        let contents = fs::read_to_string(store.profiles_dir.join("legacy.json")).unwrap();
        assert!(
            !contents.contains("\"schema_version\""),
            "loading a v0 profile must not auto-migrate the on-disk file; got {contents:?}"
        );
        drop(dir);
    }

    #[test]
    fn migration_preserves_unknown_future_section() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        let raw = br#"{"name": "Legacy Future", "video": {"codec": "av1"}}"#;
        fs::write(store.profiles_dir.join("legacy-future.json"), raw).unwrap();
        let loaded = store
            .load(&ProfileId::new("legacy-future").unwrap())
            .unwrap();
        assert!(loaded.extra.contains_key("video"));
        assert_eq!(loaded.schema_version, CURRENT_PROFILE_SCHEMA_VERSION);
        drop(dir);
    }

    #[test]
    fn future_schema_still_rejected_for_user_profile() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(
            store.profiles_dir.join("future.json"),
            br#"{"schema_version": 99, "id": "future", "name": "Future"}"#,
        )
        .unwrap();
        let err = store.load(&ProfileId::new("future").unwrap()).unwrap_err();
        assert!(
            matches!(err, ProfileStoreError::Parse { .. }),
            "expected parse error for future schema, got {err:?}"
        );
        drop(dir);
    }

    #[test]
    fn malformed_schema_version_is_rejected() {
        for (file, raw) in [
            (
                "schema-string.json",
                br#"{"schema_version": "1", "id": "schema-string", "name": "Bad"}"# as &[u8],
            ),
            (
                "schema-negative.json",
                br#"{"schema_version": -1, "id": "schema-negative", "name": "Bad"}"#,
            ),
            (
                "schema-null.json",
                br#"{"schema_version": null, "id": "schema-null", "name": "Bad"}"#,
            ),
        ] {
            let (dir, store) = temp_store();
            fs::create_dir_all(&store.profiles_dir).unwrap();
            fs::write(store.profiles_dir.join(file), raw).unwrap();
            let id = ProfileId::new(file.trim_end_matches(".json")).unwrap();
            let err = store.load(&id).unwrap_err();
            assert!(
                matches!(err, ProfileStoreError::Parse { .. }),
                "expected parse error for malformed schema_version in {file}, got {err:?}"
            );
            drop(dir);
        }
    }

    #[test]
    fn oversized_schema_version_is_rejected() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&store.profiles_dir).unwrap();
        fs::write(
            store.profiles_dir.join("schema-huge.json"),
            br#"{"schema_version": 4294967297, "id": "schema-huge", "name": "Bad"}"#,
        )
        .unwrap();
        let err = store
            .load(&ProfileId::new("schema-huge").unwrap())
            .unwrap_err();
        assert!(
            matches!(err, ProfileStoreError::Parse { .. }),
            "expected parse error for oversized schema_version, got {err:?}"
        );
        drop(dir);
    }
}
