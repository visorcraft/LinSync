//! Regression test: a planned folder delete must actually remove the file.
//!
//! `execute_delete` previously read `op.target`, but `plan_delete_side` stores
//! the path to remove in `op.source` (and leaves `target` = `None`), so every
//! delete failed with "delete operation missing target" and nothing was ever
//! deleted or trashed. No prior test executed a real delete.

use std::fs;

use linsync_core::{
    FolderCompareOptions, FolderOperationKind, FolderOperationStatus, compare_folders,
    execute_folder_operation_plan, plan_folder_operation,
};

#[test]
fn planned_delete_left_removes_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let left = tmp.path().join("left");
    let right = tmp.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    // `gone.txt` exists only on the left, so it is a deletable entry.
    let victim = left.join("gone.txt");
    fs::write(&victim, "remove me").unwrap();

    let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
    let plan = plan_folder_operation(
        &result,
        FolderOperationKind::DeleteLeft,
        &[std::path::PathBuf::from("gone.txt")],
    );
    assert_eq!(
        plan.operations.len(),
        1,
        "expected exactly one delete operation, got {:?}",
        plan.operations
    );

    // use_trash = false → permanent delete, so the test needs no XDG trash dirs.
    let outcomes = execute_folder_operation_plan(&plan, tmp.path(), false);
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
