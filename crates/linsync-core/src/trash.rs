use std::ffi::CString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::storage::Settings;
use crate::text::CompareSide;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletePreference {
    #[default]
    MoveToTrash,
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteBackend {
    FreeDesktopTrash,
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermanentDeleteConfirmation {
    NotConfirmed,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletePlan {
    pub targets: Vec<PathBuf>,
    pub side: Option<CompareSide>,
    pub backend: DeleteBackend,
    pub requires_confirmation: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteOutcome {
    pub original_path: PathBuf,
    pub action: DeleteBackend,
    pub trashed: Option<TrashedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRestoreGuidance {
    pub original_path: PathBuf,
    pub restorable: bool,
    pub restore_source: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashedEntry {
    pub original_path: PathBuf,
    pub trash_file_path: PathBuf,
    pub trash_info_path: PathBuf,
}

#[derive(Debug)]
pub enum DeleteError {
    EmptyTargetSet,
    InvalidTarget(PathBuf),
    TrashUnavailable {
        path: PathBuf,
        message: String,
    },
    PermanentDeleteRequiresConfirmation {
        item_count: usize,
        side: Option<CompareSide>,
    },
    Io(io::Error),
}

impl std::fmt::Display for DeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTargetSet => write!(f, "delete target set must not be empty"),
            Self::InvalidTarget(path) => write!(f, "invalid delete target: {}", path.display()),
            Self::TrashUnavailable { path, message } => {
                write!(f, "trash unavailable for {}: {message}", path.display())
            }
            Self::PermanentDeleteRequiresConfirmation { item_count, side } => {
                write!(
                    f,
                    "permanent delete requires confirmation for {item_count} {}",
                    side_item_label(*side, *item_count)
                )
            }
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for DeleteError {}

impl From<io::Error> for DeleteError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn plan_delete(
    settings: &Settings,
    side: Option<CompareSide>,
    targets: Vec<PathBuf>,
    trash_available: bool,
) -> Result<DeletePlan, DeleteError> {
    if targets.is_empty() {
        return Err(DeleteError::EmptyTargetSet);
    }

    let item_count = targets.len();
    let backend = match settings.delete_preference {
        DeletePreference::MoveToTrash if trash_available => DeleteBackend::FreeDesktopTrash,
        DeletePreference::MoveToTrash | DeletePreference::Permanent => DeleteBackend::Permanent,
    };
    let requires_confirmation = backend == DeleteBackend::Permanent
        && (settings.confirm_permanent_delete
            || settings.delete_preference == DeletePreference::MoveToTrash);
    let warning = requires_confirmation.then(|| {
        if settings.delete_preference == DeletePreference::MoveToTrash && !trash_available {
            format!(
                "Trash is unavailable; permanently deleting {item_count} {} requires confirmation.",
                side_item_label(side, item_count)
            )
        } else {
            format!(
                "Permanently deleting {item_count} {} requires confirmation.",
                side_item_label(side, item_count)
            )
        }
    });

    Ok(DeletePlan {
        targets,
        side,
        backend,
        requires_confirmation,
        warning,
    })
}

pub fn execute_delete_plan(
    plan: &DeletePlan,
    data_home: &Path,
    confirmation: PermanentDeleteConfirmation,
) -> Result<Vec<DeleteOutcome>, DeleteError> {
    if plan.requires_confirmation && confirmation != PermanentDeleteConfirmation::Confirmed {
        return Err(DeleteError::PermanentDeleteRequiresConfirmation {
            item_count: plan.targets.len(),
            side: plan.side,
        });
    }

    plan.targets
        .iter()
        .map(|target| match plan.backend {
            DeleteBackend::FreeDesktopTrash => {
                let trashed = move_to_freedesktop_trash(target, data_home)?;
                Ok(DeleteOutcome {
                    original_path: target.clone(),
                    action: DeleteBackend::FreeDesktopTrash,
                    trashed: Some(trashed),
                })
            }
            DeleteBackend::Permanent => {
                permanently_delete(target)?;
                Ok(DeleteOutcome {
                    original_path: target.clone(),
                    action: DeleteBackend::Permanent,
                    trashed: None,
                })
            }
        })
        .collect()
}

pub fn delete_restore_guidance(outcomes: &[DeleteOutcome]) -> Vec<DeleteRestoreGuidance> {
    outcomes
        .iter()
        .map(|outcome| match (&outcome.action, &outcome.trashed) {
            (DeleteBackend::FreeDesktopTrash, Some(trashed)) => DeleteRestoreGuidance {
                original_path: outcome.original_path.clone(),
                restorable: true,
                restore_source: Some(trashed.trash_file_path.clone()),
                message: format!(
                    "Restore '{}' from the desktop Trash or move '{}' back to its original path.",
                    outcome.original_path.display(),
                    trashed.trash_file_path.display()
                ),
            },
            _ => DeleteRestoreGuidance {
                original_path: outcome.original_path.clone(),
                restorable: false,
                restore_source: None,
                message: format!(
                    "'{}' was permanently deleted and cannot be restored by LinSync.",
                    outcome.original_path.display()
                ),
            },
        })
        .collect()
}

pub fn move_to_freedesktop_trash(
    path: &Path,
    data_home: &Path,
) -> Result<TrashedEntry, DeleteError> {
    if !path.exists() {
        return Err(DeleteError::InvalidTarget(path.to_path_buf()));
    }

    let original_path = absolute_path(path)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| DeleteError::InvalidTarget(path.to_path_buf()))?;
    let trash_dir = data_home.join("Trash");
    let files_dir = trash_dir.join("files");
    let info_dir = trash_dir.join("info");
    create_owner_only_dir_all(&files_dir)?;
    create_owner_only_dir_all(&info_dir)?;

    for attempt in 0..1000 {
        let trash_name = trash_file_name(file_name, attempt);
        let trash_file_path = files_dir.join(&trash_name);
        if trash_file_path.exists() {
            continue;
        }

        let trash_info_path = info_dir.join(format!("{}.trashinfo", trash_name.to_string_lossy()));
        let info_text = trash_info_text(&original_path, SystemTime::now());
        let mut info_file = match create_owner_only_file(&trash_info_path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(DeleteError::Io(err)),
        };
        if let Err(err) = info_file.write_all(info_text.as_bytes()) {
            let _ = fs::remove_file(&trash_info_path);
            return Err(DeleteError::Io(err));
        }
        if let Err(err) = info_file.sync_all() {
            let _ = fs::remove_file(&trash_info_path);
            return Err(DeleteError::Io(err));
        }
        drop(info_file);

        if trash_file_path.exists() {
            let _ = fs::remove_file(&trash_info_path);
            continue;
        }

        if let Err(err) = rename_no_replace(path, &trash_file_path) {
            let _ = fs::remove_file(&trash_info_path);
            if err.kind() == io::ErrorKind::AlreadyExists {
                continue;
            }

            return Err(DeleteError::TrashUnavailable {
                path: path.to_path_buf(),
                message: err.to_string(),
            });
        }

        return Ok(TrashedEntry {
            original_path,
            trash_file_path,
            trash_info_path,
        });
    }

    Err(DeleteError::TrashUnavailable {
        path: path.to_path_buf(),
        message: "could not allocate a unique trash entry name".to_owned(),
    })
}

pub fn permanently_delete(path: &Path) -> Result<(), DeleteError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    let source_c = path_to_cstring(source)?;
    let destination_c = path_to_cstring(destination)?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source_c.as_ptr(),
            libc::AT_FDCWD,
            destination_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };

    if result == 0 {
        return Ok(());
    }

    let err = io::Error::last_os_error();
    if renameat2_unsupported(&err) {
        return rename_no_replace_fallback(source, destination);
    }

    Err(err)
}

fn renameat2_unsupported(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(libc::ENOSYS | libc::EINVAL | libc::EPERM)
    )
}

fn rename_no_replace_fallback(source: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }

    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_file() {
        fs::hard_link(source, destination)?;
        if let Err(err) = fs::remove_file(source) {
            // The source still holds the content; remove the trash-side hard
            // link so the file never lives in both places.
            let _ = fs::remove_file(destination);
            return Err(err);
        }
        return Ok(());
    }

    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    fs::rename(source, destination)
}

fn create_owner_only_dir_all(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
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

fn path_to_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL byte"))
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn trash_file_name(file_name: &std::ffi::OsStr, attempt: u32) -> std::ffi::OsString {
    if attempt == 0 {
        return file_name.to_os_string();
    }

    let mut candidate = file_name.to_os_string();
    candidate.push(format!(".{}", attempt));
    candidate
}

fn trash_info_text(original_path: &Path, deleted_at: SystemTime) -> String {
    format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode_path(original_path),
        deletion_date(deleted_at)
    )
}

fn percent_encode_path(path: &Path) -> String {
    let mut encoded = String::new();
    for &byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'_' | b'-' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn deletion_date(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

fn side_item_label(side: Option<CompareSide>, item_count: usize) -> String {
    let plural = if item_count == 1 { "item" } else { "items" };
    match side {
        Some(CompareSide::Left) => format!("left-side {plural}"),
        Some(CompareSide::Base) => format!("base-side {plural}"),
        Some(CompareSide::Right) => format!("right-side {plural}"),
        Some(CompareSide::Result) => format!("result-side {plural}"),
        None => plural.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Settings, ThemePreference};
    use std::time::Duration;

    #[test]
    fn plans_trash_or_confirmable_permanent_delete() {
        let settings = Settings::default();
        let trash_plan = plan_delete(
            &settings,
            Some(CompareSide::Left),
            vec![PathBuf::from("left.txt")],
            true,
        )
        .unwrap();
        assert_eq!(trash_plan.backend, DeleteBackend::FreeDesktopTrash);
        assert!(!trash_plan.requires_confirmation);
        assert!(trash_plan.warning.is_none());

        let fallback_plan = plan_delete(
            &settings,
            Some(CompareSide::Right),
            vec![PathBuf::from("right.txt"), PathBuf::from("other.txt")],
            false,
        )
        .unwrap();
        assert_eq!(fallback_plan.backend, DeleteBackend::Permanent);
        assert!(fallback_plan.requires_confirmation);
        assert!(
            fallback_plan
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("2 right-side items"))
        );

        let permanent_settings = Settings {
            delete_preference: DeletePreference::Permanent,
            confirm_permanent_delete: false,
            theme_preference: ThemePreference::Dark,
            ..Settings::default()
        };
        let permanent_plan = plan_delete(
            &permanent_settings,
            None,
            vec![PathBuf::from("delete.txt")],
            true,
        )
        .unwrap();
        assert_eq!(permanent_plan.backend, DeleteBackend::Permanent);
        assert!(!permanent_plan.requires_confirmation);
    }

    #[test]
    fn permanent_delete_requires_confirmation_when_planned() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("target.txt");
        fs::write(&target, "delete").unwrap();
        let plan = DeletePlan {
            targets: vec![target.clone()],
            side: Some(CompareSide::Left),
            backend: DeleteBackend::Permanent,
            requires_confirmation: true,
            warning: Some("confirm".to_owned()),
        };

        let err = execute_delete_plan(
            &plan,
            &fixture.path,
            PermanentDeleteConfirmation::NotConfirmed,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DeleteError::PermanentDeleteRequiresConfirmation {
                item_count: 1,
                side: Some(CompareSide::Left)
            }
        ));
        assert!(target.exists());

        let outcomes =
            execute_delete_plan(&plan, &fixture.path, PermanentDeleteConfirmation::Confirmed)
                .unwrap();
        assert_eq!(outcomes[0].action, DeleteBackend::Permanent);
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn permanent_delete_removes_symlink_without_touching_target() {
        let fixture = TempFixture::new();
        let target_dir = fixture.path.join("target");
        let target_file = target_dir.join("keep.txt");
        let link = fixture.path.join("target-link");
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&target_file, "keep").unwrap();
        std::os::unix::fs::symlink(&target_dir, &link).unwrap();

        permanently_delete(&link).unwrap();

        assert!(!link.exists());
        assert!(target_file.exists());
        assert_eq!(fs::read_to_string(target_file).unwrap(), "keep");
    }

    #[test]
    fn permanent_delete_handles_readonly_file_targets() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("readonly.txt");
        fs::write(&target, "delete").unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions).unwrap();

        permanently_delete(&target).unwrap();

        assert!(!target.exists());
    }

    #[test]
    fn moves_file_to_freedesktop_home_trash_with_metadata() {
        let fixture = TempFixture::new();
        let data_home = fixture.path.join("data");
        let target = fixture.path.join("alpha beta%.txt");
        fs::write(&target, "content").unwrap();

        let trashed = move_to_freedesktop_trash(&target, &data_home).unwrap();

        assert!(!target.exists());
        assert_eq!(
            fs::read_to_string(&trashed.trash_file_path).unwrap(),
            "content"
        );
        assert!(
            trashed
                .trash_file_path
                .starts_with(data_home.join("Trash/files"))
        );
        assert!(
            trashed
                .trash_info_path
                .starts_with(data_home.join("Trash/info"))
        );
        let info = fs::read_to_string(&trashed.trash_info_path).unwrap();
        assert!(info.starts_with("[Trash Info]\n"));
        assert!(info.contains("Path="));
        assert!(info.contains("alpha%20beta%25.txt"));
        assert!(info.contains("DeletionDate="));
    }

    #[cfg(unix)]
    #[test]
    fn freedesktop_trash_metadata_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempFixture::new();
        let data_home = fixture.path.join("data");
        let target = fixture.path.join("private.txt");
        fs::write(&target, "content").unwrap();

        let trashed = move_to_freedesktop_trash(&target, &data_home).unwrap();

        assert_eq!(
            fs::metadata(data_home.join("Trash/files"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(data_home.join("Trash/info"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(trashed.trash_info_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn restore_guidance_distinguishes_trash_from_permanent_delete() {
        let fixture = TempFixture::new();
        let data_home = fixture.path.join("data");
        let trashed_target = fixture.path.join("trashed.txt");
        let permanent_target = fixture.path.join("permanent.txt");
        fs::write(&trashed_target, "trash").unwrap();
        fs::write(&permanent_target, "delete").unwrap();
        let trash_plan = DeletePlan {
            targets: vec![trashed_target.clone()],
            side: None,
            backend: DeleteBackend::FreeDesktopTrash,
            requires_confirmation: false,
            warning: None,
        };
        let permanent_plan = DeletePlan {
            targets: vec![permanent_target.clone()],
            side: None,
            backend: DeleteBackend::Permanent,
            requires_confirmation: false,
            warning: None,
        };

        let mut outcomes = execute_delete_plan(
            &trash_plan,
            &data_home,
            PermanentDeleteConfirmation::NotConfirmed,
        )
        .unwrap();
        outcomes.extend(
            execute_delete_plan(
                &permanent_plan,
                &data_home,
                PermanentDeleteConfirmation::NotConfirmed,
            )
            .unwrap(),
        );
        let guidance = delete_restore_guidance(&outcomes);

        assert!(guidance[0].restorable);
        assert!(guidance[0].restore_source.is_some());
        assert!(guidance[0].message.contains("desktop Trash"));
        assert!(!guidance[1].restorable);
        assert!(guidance[1].restore_source.is_none());
        assert!(guidance[1].message.contains("permanently deleted"));
    }

    #[test]
    fn trash_move_allocates_unique_names() {
        let fixture = TempFixture::new();
        let data_home = fixture.path.join("data");
        let first = fixture.path.join("same.txt");
        let second = fixture.path.join("same.txt");
        fs::write(&first, "first").unwrap();
        let first_trash = move_to_freedesktop_trash(&first, &data_home).unwrap();
        fs::write(&second, "second").unwrap();
        let second_trash = move_to_freedesktop_trash(&second, &data_home).unwrap();

        assert_ne!(first_trash.trash_file_path, second_trash.trash_file_path);
        assert_eq!(
            fs::read_to_string(second_trash.trash_file_path).unwrap(),
            "second"
        );
    }

    #[test]
    fn trash_rename_does_not_replace_existing_destination() {
        let fixture = TempFixture::new();
        let source = fixture.path.join("source.txt");
        let destination = fixture.path.join("destination.txt");
        fs::write(&source, "source").unwrap();
        fs::write(&destination, "destination").unwrap();

        let err = rename_no_replace(&source, &destination).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&source).unwrap(), "source");
        assert_eq!(fs::read_to_string(&destination).unwrap(), "destination");
    }

    #[test]
    fn formats_deletion_date_from_unix_time() {
        assert_eq!(
            deletion_date(UNIX_EPOCH + Duration::from_secs(86_400 + 3_661)),
            "1970-01-02T01:01:01"
        );
    }

    #[cfg(unix)]
    #[test]
    fn percent_encode_preserves_non_utf8_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        // A path containing an invalid UTF-8 byte (0xFF) must survive
        // round-trip through the trashinfo Path= field rather than being
        // replaced by U+FFFD.
        let raw = OsString::from_vec(vec![b'/', b'a', 0xFF, b'b', b'.', b't', b'x', b't']);
        let encoded = percent_encode_path(Path::new(&raw));
        assert_eq!(encoded, "/a%FFb.txt");
        assert!(!encoded.contains('\u{FFFD}'));
    }

    #[cfg(unix)]
    #[test]
    fn rename_fallback_rolls_back_when_source_unlink_fails() {
        use std::os::unix::fs::PermissionsExt;

        // Directory permissions do not restrict root, so the source unlink
        // would succeed and the rollback path would never be exercised.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }

        let fixture = TempFixture::new();
        let source_dir = fixture.path.join("locked");
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("source.txt");
        let destination = fixture.path.join("destination.txt");
        fs::write(&source, "content").unwrap();

        // Make the source's parent read-only so unlinking the source fails
        // after the hard link into the destination succeeds.
        fs::set_permissions(&source_dir, fs::Permissions::from_mode(0o500)).unwrap();

        let err = rename_no_replace_fallback(&source, &destination).unwrap_err();

        // Restore permissions so the fixture can be cleaned up.
        fs::set_permissions(&source_dir, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(err.kind() != io::ErrorKind::NotFound);
        // Source content preserved, destination hard link rolled back so the
        // file never lives in both places.
        assert!(source.exists());
        assert_eq!(fs::read_to_string(&source).unwrap(), "content");
        assert!(!destination.exists());
    }

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "linsync-trash-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
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
