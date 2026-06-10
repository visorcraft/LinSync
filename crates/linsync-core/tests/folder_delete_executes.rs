//! Regression test: a planned folder delete must actually remove the file.
//!
//! `execute_delete` previously read `op.target`, but `plan_delete_side` stores
//! the path to remove in `op.source` (and leaves `target` = `None`), so every
//! delete failed with "delete operation missing target" and nothing was ever
//! deleted or trashed. No prior test executed a real delete.
//!
//! Also covers the permanent-delete confirmation gate: executing a plan with
//! `use_trash_for_deletes = false` must fail unless the caller passes
//! `PermanentDeleteConfirmation::Confirmed`.

use std::fs;
use std::path::PathBuf;

use linsync_core::{
    FolderCompareOptions, FolderOperationKind, FolderOperationPlan, FolderOperationStatus,
    PermanentDeleteConfirmation, compare_folders, execute_folder_operation_plan,
    plan_folder_operation,
};

/// Builds left/right trees where `left/gone.txt` is the only deletable entry,
/// and returns the planned `DeleteLeft` operation plus the victim's path.
fn plan_single_delete(tmp: &std::path::Path) -> (FolderOperationPlan, PathBuf) {
    let left = tmp.join("left");
    let right = tmp.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    // `gone.txt` exists only on the left, so it is a deletable entry.
    let victim = left.join("gone.txt");
    fs::write(&victim, "remove me").unwrap();

    let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
    let plan = plan_folder_operation(
        &result,
        FolderOperationKind::DeleteLeft,
        &[PathBuf::from("gone.txt")],
    );
    assert_eq!(
        plan.operations.len(),
        1,
        "expected exactly one delete operation, got {:?}",
        plan.operations
    );
    (plan, victim)
}

#[test]
fn planned_delete_left_removes_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, victim) = plan_single_delete(tmp.path());

    // use_trash = false → permanent delete, so the test needs no XDG trash dirs.
    let outcomes = execute_folder_operation_plan(
        &plan,
        tmp.path(),
        false,
        PermanentDeleteConfirmation::Confirmed,
    );
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].status,
        FolderOperationStatus::Succeeded,
        "delete should succeed, got: {} ({:?})",
        outcomes[0].message,
        outcomes[0].status
    );
    assert!(!victim.exists(), "the file should have been deleted");
}

#[test]
fn permanent_folder_delete_requires_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, victim) = plan_single_delete(tmp.path());
    assert!(
        plan.contains_deletes,
        "a delete plan must advertise contains_deletes so callers know to confirm"
    );

    let outcomes = execute_folder_operation_plan(
        &plan,
        tmp.path(),
        false,
        PermanentDeleteConfirmation::NotConfirmed,
    );
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].status,
        FolderOperationStatus::Failed,
        "unconfirmed permanent delete must fail, got: {} ({:?})",
        outcomes[0].message,
        outcomes[0].status
    );
    assert!(
        outcomes[0].message.contains("requires confirmation"),
        "message should explain the confirmation gate, got: {}",
        outcomes[0].message
    );
    assert!(
        victim.exists(),
        "nothing may be deleted without confirmation"
    );
}

#[test]
fn permanent_folder_delete_executes_when_confirmed() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, victim) = plan_single_delete(tmp.path());

    let outcomes = execute_folder_operation_plan(
        &plan,
        tmp.path(),
        false,
        PermanentDeleteConfirmation::Confirmed,
    );
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].status,
        FolderOperationStatus::Succeeded,
        "confirmed permanent delete should succeed, got: {} ({:?})",
        outcomes[0].message,
        outcomes[0].status
    );
    assert!(!victim.exists(), "the file should have been deleted");
}

#[test]
fn trash_delete_needs_no_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, victim) = plan_single_delete(tmp.path());
    let data_home = tmp.path().join("data");
    fs::create_dir_all(&data_home).unwrap();

    // use_trash = true is recoverable, so the confirmation gate does not apply.
    let outcomes = execute_folder_operation_plan(
        &plan,
        &data_home,
        true,
        PermanentDeleteConfirmation::NotConfirmed,
    );
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].status,
        FolderOperationStatus::Succeeded,
        "trash delete should succeed without confirmation, got: {} ({:?})",
        outcomes[0].message,
        outcomes[0].status
    );
    assert!(!victim.exists(), "the file should have moved to the trash");
    assert!(
        data_home
            .join("Trash")
            .join("files")
            .join("gone.txt")
            .exists(),
        "the file should be recoverable from the trash"
    );
}
