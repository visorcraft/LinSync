use super::*;

pub(crate) fn folders_command(args: &[String]) -> Result<ExitCode, String> {
    let folder_args = split_folder_args(args)?;

    if folder_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli folders [--recursive] [--profile NAME-OR-PATH] [--method METHOD] [--timestamp-tolerance-ms MS] [--symlinks target|follow|special] [--large-file-threshold-bytes BYTES] [--large-file-method quick|binary] [--hash-algorithm blake3|sha256|crc32] [--compare-permissions] [--compare-ownership] [--compare-xattrs] [--dry-run] [--exclude-generated] [--filter RULE] [--filter-name NAME] [--case-insensitive-filter] [--hide-skipped] [--state STATE] [--types LIST] [--search SUBSTR] [--sort KEY] [--desc] [--group-by GROUP] [--offset N] [--limit N] [--json|--csv|--count|--quiet] LEFT RIGHT"
                .to_owned(),
        );
    }

    let options = folder_args.compare_options();
    let result = compare_folders(
        PathBuf::from(&folder_args.paths[0]).as_path(),
        PathBuf::from(&folder_args.paths[1]).as_path(),
        &options,
    )
    .map_err(|err| err.to_string())?;

    let summary = &result.summary;
    let differences = summary.different_count + summary.one_sided_count + summary.errors_count;
    let query = folder_args.query();
    let entry_filter = query.state;
    let page = result.query(&query);
    let filtered_entries: Vec<&FolderEntryDiff> = page
        .groups
        .iter()
        .flat_map(|group| group.entries.iter().copied())
        .collect();
    let count = if folder_args.query_is_restricting() {
        page.total_matched
    } else {
        differences
    };

    match folder_args.output {
        FolderOutput::Structured(OutputMode::Text) => {
            println!(
                "compared={} skipped={} identical={} different={} one_sided={} left_only={} right_only={} errors={} aborted={} method_downgrades={} filtered={} returned={} offset={} has_more={} elapsed_ms={} status={}",
                summary.compared_count,
                summary.skipped_count,
                summary.identical_count,
                summary.different_count,
                summary.one_sided_count,
                summary.left_only_count,
                summary.right_only_count,
                summary.errors_count,
                summary.aborted_count,
                summary.method_downgrade_count,
                page.total_matched,
                page.returned,
                page.offset,
                page.has_more,
                summary.elapsed.as_millis(),
                folder_status(summary.status)
            );
            if folder_args.dry_run {
                let selected: Vec<PathBuf> = result
                    .entries
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.state,
                            FolderEntryState::Different
                                | FolderEntryState::LeftOnly
                                | FolderEntryState::RightOnly
                        )
                    })
                    .map(|e| e.relative_path.clone())
                    .collect();
                let mut plan = plan_folder_operation(
                    &result,
                    linsync_core::FolderOperationKind::CopyLeftToRight,
                    &selected,
                );
                let left_path = Path::new(&folder_args.paths[0]);
                let right_path = Path::new(&folder_args.paths[1]);
                let _ = assess_operation_risks(&mut plan, left_path, right_path);
                let risk = plan.risk_summary();
                println!(
                    "dry_run: {} operations ({} overwrites, {} deletes, {} high-risk warnings)",
                    risk.total_operations,
                    risk.overwrite_count,
                    risk.delete_count,
                    risk.high_risk_count,
                );
                for warning in &risk.warnings {
                    println!(
                        "  warning: {:?}: {} ({})",
                        warning.kind,
                        warning.message,
                        warning.relative_path.display()
                    );
                }
            }
        }
        FolderOutput::Structured(OutputMode::Json) => {
            let entries = filtered_entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "path": entry.relative_path.display().to_string(),
                        "name": entry.name,
                        "extension": entry.extension,
                        "state": folder_state(entry.state),
                        "left_size": entry.left_size,
                        "right_size": entry.right_size,
                        "left_modified_ms": entry.left_modified.and_then(system_time_millis),
                        "right_modified_ms": entry.right_modified.and_then(system_time_millis),
                        "type": entry.entry_type.as_str(),
                        "effective_method": entry.effective_method.map(CompareMethod::as_str),
                        "method_note": entry.method_note,
                        "error": entry.error,
                    })
                })
                .collect::<Vec<_>>();
            let risk_metadata = if folder_args.dry_run {
                let selected: Vec<PathBuf> = result
                    .entries
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.state,
                            FolderEntryState::Different
                                | FolderEntryState::LeftOnly
                                | FolderEntryState::RightOnly
                        )
                    })
                    .map(|e| e.relative_path.clone())
                    .collect();
                let mut plan = plan_folder_operation(
                    &result,
                    linsync_core::FolderOperationKind::CopyLeftToRight,
                    &selected,
                );
                let left_path = Path::new(&folder_args.paths[0]);
                let right_path = Path::new(&folder_args.paths[1]);
                let _ = assess_operation_risks(&mut plan, left_path, right_path);
                let risk = plan.risk_summary();
                Some(serde_json::json!({
                    "total_operations": risk.total_operations,
                    "overwrite_count": risk.overwrite_count,
                    "delete_count": risk.delete_count,
                    "high_risk_count": risk.high_risk_count,
                    "warnings": risk.warnings.iter().map(|w| serde_json::json!({
                        "relative_path": w.relative_path.display().to_string(),
                        "kind": format!("{:?}", w.kind),
                        "message": w.message,
                    })).collect::<Vec<_>>(),
                }))
            } else {
                None
            };
            let mut output = serde_json::json!({
                "equal": result.is_equal(),
                "compared": summary.compared_count,
                "skipped": summary.skipped_count,
                "identical": summary.identical_count,
                "different": summary.different_count,
                "one_sided": summary.one_sided_count,
                "left_only": summary.left_only_count,
                "right_only": summary.right_only_count,
                "errors": summary.errors_count,
                "aborted": summary.aborted_count,
                "method_downgrades": summary.method_downgrade_count,
                "filtered": page.total_matched,
                "returned": page.returned,
                "offset": page.offset,
                "has_more": page.has_more,
                "elapsed_ms": summary.elapsed.as_millis(),
                "status": folder_status(summary.status),
                "options": folder_options_metadata_json(
                    &options,
                    folder_args.effective_profile.as_deref(),
                    entry_filter,
                ),
                "entries": entries,
            });
            if let Some(profile_id) = folder_args.effective_profile.as_deref() {
                output["profile"] = serde_json::json!(profile_id);
            }
            if let Some(risk) = risk_metadata {
                output["risk"] = risk;
            }
            println!("{output}");
        }
        FolderOutput::Structured(OutputMode::Count) => println!("{count}"),
        FolderOutput::Structured(OutputMode::Quiet) => {}
        FolderOutput::Csv => {
            println!(
                "path,state,left_size,right_size,name,extension,type,left_modified_ms,right_modified_ms,effective_method,method_note,error"
            );
            for entry in filtered_entries {
                println!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_escape(&entry.relative_path.display().to_string()),
                    folder_state(entry.state),
                    entry
                        .left_size
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    entry
                        .right_size
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    csv_escape(&entry.name),
                    csv_escape(entry.extension.as_deref().unwrap_or("")),
                    entry.entry_type.as_str(),
                    entry
                        .left_modified
                        .and_then(system_time_millis)
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    entry
                        .right_modified
                        .and_then(system_time_millis)
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    entry
                        .effective_method
                        .map(CompareMethod::as_str)
                        .unwrap_or(""),
                    csv_escape(entry.method_note.as_deref().unwrap_or("")),
                    csv_escape(entry.error.as_deref().unwrap_or(""))
                );
            }
        }
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
