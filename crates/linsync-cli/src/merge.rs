use super::*;

pub(crate) fn compare3_command(args: &[String]) -> Result<ExitCode, String> {
    let mut emit_markers = false;
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--markers" => emit_markers = true,
            "--json" => json = true,
            value => paths.push(value),
        }
    }

    if emit_markers && json {
        return Err("compare3 --markers cannot be combined with --json".to_owned());
    }

    if paths.len() != 3 {
        return Err("usage: linsync-cli compare3 [--markers|--json] LEFT BASE RIGHT".to_owned());
    }

    let left = PathBuf::from(paths[0]);
    let base = PathBuf::from(paths[1]);
    let right = PathBuf::from(paths[2]);
    let left_base = compare_text_files(&left, &base, &TextCompareOptions::default())
        .map_err(|err| err.to_string())?;
    let right_base = compare_text_files(&right, &base, &TextCompareOptions::default())
        .map_err(|err| err.to_string())?;
    let left_text = fs::read_to_string(&left).map_err(|err| err.to_string())?;
    let base_text = fs::read_to_string(&base).map_err(|err| err.to_string())?;
    let right_text = fs::read_to_string(&right).map_err(|err| err.to_string())?;
    let merge = merge_three_way(&base_text, &left_text, &right_text);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "equal": left_base.is_equal() && right_base.is_equal() && !merge.has_conflicts(),
                "left_base_differences": left_base.summary.differences,
                "left_base_blocks": left_base.summary.diff_blocks,
                "right_base_differences": right_base.summary.differences,
                "right_base_blocks": right_base.summary.diff_blocks,
                "conflicts": merge.conflicts.len(),
            })
        );
    } else {
        println!(
            "left/base differences={} blocks={}",
            left_base.summary.differences, left_base.summary.diff_blocks
        );
        println!(
            "right/base differences={} blocks={}",
            right_base.summary.differences, right_base.summary.diff_blocks
        );
        println!("conflicts={}", merge.conflicts.len());
    }

    if emit_markers {
        print!(
            "{}",
            merge.conflict_marker_text(
                &left.display().to_string(),
                &base.display().to_string(),
                &right.display().to_string()
            )
        );
    }

    Ok(
        if left_base.is_equal() && right_base.is_equal() && !merge.has_conflicts() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        },
    )
}

pub(crate) fn conflict_command(args: &[String]) -> Result<ExitCode, String> {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            value => paths.push(value),
        }
    }

    if paths.len() != 1 {
        return Err("usage: linsync-cli conflict [--json] FILE".to_owned());
    }

    let path = PathBuf::from(paths[0]);
    let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let conflicts = parse_conflict_markers(&text).map_err(|err| err.to_string())?;

    if json {
        let conflicts_json = conflicts
            .iter()
            .map(|conflict| {
                serde_json::json!({
                    "index": conflict.index,
                    "start_line": conflict.start_line,
                    "end_line": conflict.end_line,
                    "left_label": conflict.left_label,
                    "base_label": conflict.base_label,
                    "right_label": conflict.right_label,
                    "left_lines": conflict.left_lines.len(),
                    "base_lines": conflict.base_lines.len(),
                    "right_lines": conflict.right_lines.len(),
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "path": path.display().to_string(),
                "conflicts": conflicts.len(),
                "items": conflicts_json,
            })
        );
    } else {
        println!("{}: conflicts={}", path.display(), conflicts.len());
        for conflict in &conflicts {
            println!(
                "conflict={} lines={}-{} left={} base={} right={} left_lines={} base_lines={} right_lines={}",
                conflict.index + 1,
                conflict.start_line,
                conflict.end_line,
                display_label(&conflict.left_label),
                conflict
                    .base_label
                    .as_deref()
                    .map(display_label)
                    .unwrap_or("-"),
                display_label(&conflict.right_label),
                conflict.left_lines.len(),
                conflict.base_lines.len(),
                conflict.right_lines.len()
            );
        }
    }

    Ok(if conflicts.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub(crate) fn display_label(label: &str) -> &str {
    if label.is_empty() { "-" } else { label }
}

pub(crate) fn merge_conflicts_json(conflicts: &[ThreeWayConflict]) -> Vec<serde_json::Value> {
    conflicts
        .iter()
        .map(|conflict| {
            serde_json::json!({
                "id": conflict.id.0,
                "start_line": conflict.start_line,
                "end_line": conflict.end_line,
                "left_lines": conflict.left_lines.len(),
                "base_lines": conflict.base_lines.len(),
                "right_lines": conflict.right_lines.len(),
            })
        })
        .collect()
}

pub(crate) fn mergetool_command(args: &[String]) -> Result<ExitCode, String> {
    let mut base: Option<String> = None;
    let mut local: Option<String> = None;
    let mut remote: Option<String> = None;
    let mut merged: Option<String> = None;
    let mut auto: Option<String> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => {
                base = args.get(i + 1).cloned();
                i += 2;
            }
            "--local" => {
                local = args.get(i + 1).cloned();
                i += 2;
            }
            "--remote" => {
                remote = args.get(i + 1).cloned();
                i += 2;
            }
            "--merged" => {
                merged = args.get(i + 1).cloned();
                i += 2;
            }
            "--auto-resolve" => {
                auto = args.get(i + 1).cloned();
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other => {
                return Err(format!("unknown mergetool flag: {other}"));
            }
        }
    }

    let (base, local, remote, merged) = match (base, local, remote, merged) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => {
            return Err("mergetool requires --base, --local, --remote, --merged".to_owned());
        }
    };

    let base_text = fs::read_to_string(&base).map_err(|e| e.to_string())?;
    let local_text = fs::read_to_string(&local).map_err(|e| e.to_string())?;
    let remote_text = fs::read_to_string(&remote).map_err(|e| e.to_string())?;

    let base_doc = TextDocument::from_text("base", &base_text);
    let local_doc = TextDocument::from_text("local", &local_text);
    let remote_doc = TextDocument::from_text("remote", &remote_text);

    let mut state = ThreeWayMergeState::new(base_doc, local_doc, remote_doc);
    let initial_conflicts = state.conflicts();

    if let Some(choice) = auto.as_deref() {
        let mc = match choice {
            "left" => MergeChoice::Left,
            "right" => MergeChoice::Right,
            "base" => MergeChoice::Base,
            other => {
                return Err(format!(
                    "invalid --auto-resolve value '{other}'; expected left, right, or base"
                ));
            }
        };
        let conflict_ids: Vec<_> = initial_conflicts.iter().map(|c| c.id).collect();
        for id in conflict_ids {
            state
                .resolve(id, mc.clone())
                .map_err(|e| format!("resolve failed: {e}"))?;
        }
        state
            .save_to(std::path::Path::new(&merged))
            .map_err(|e| format!("save failed: {e}"))?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "resolved",
                    "mode": "auto",
                    "auto_choice": choice,
                    "base": base,
                    "local": local,
                    "remote": remote,
                    "merged": merged,
                    "conflicts": initial_conflicts.len(),
                    "resolved_conflicts": initial_conflicts.len(),
                    "unresolved_conflicts": state.unresolved_count(),
                    "written": true,
                    "items": merge_conflicts_json(&initial_conflicts),
                })
            );
        }
        return Ok(ExitCode::SUCCESS);
    }

    // No --auto-resolve: launch the GUI for interactive resolution. It opens
    // the Merge workspace on the three inputs (via LINSYNC_MERGE_* env) and
    // writes the resolved output to `merged` on save. We wait for it to exit,
    // then verify a fully-resolved file was written.
    let gui = resolve_gui_binary();
    let status = Command::new(&gui)
        .env("LINSYNC_MERGE_BASE", &base)
        .env("LINSYNC_MERGE_LOCAL", &local)
        .env("LINSYNC_MERGE_REMOTE", &remote)
        .env("LINSYNC_MERGE_MERGED", &merged)
        .env("LINSYNC_STARTUP_SECTION", "merge")
        .status()
        .map_err(|err| {
            format!(
                "failed to launch GUI '{}' for mergetool: {err}",
                gui.display()
            )
        })?;

    // Success is determined by the written output, not the GUI's exit status:
    // the resolved file must exist and contain no conflict markers.
    let outcome = match fs::read_to_string(&merged) {
        Ok(text) if !merge_output_has_conflicts(&text) => "resolved",
        Ok(_) => "unresolved",
        Err(_) => "missing",
    };
    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": outcome,
                "mode": "interactive",
                "base": base,
                "local": local,
                "remote": remote,
                "merged": merged,
                "conflicts": initial_conflicts.len(),
                "gui_exit_success": status.success(),
                "written": outcome == "resolved",
            })
        );
    }
    match outcome {
        "resolved" => Ok(ExitCode::SUCCESS),
        "unresolved" => {
            eprintln!("merge left unresolved conflict markers in {merged}");
            Ok(ExitCode::from(1))
        }
        _ => {
            eprintln!("no resolved output was written to {merged}");
            Ok(ExitCode::from(2))
        }
    }
}

/// Whether merged output still contains Git conflict markers (an unresolved
/// merge). Checks line starts for the canonical 7-character markers.
pub(crate) fn merge_output_has_conflicts(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with("<<<<<<<") || line.starts_with("=======") || line.starts_with(">>>>>>>")
    })
}
