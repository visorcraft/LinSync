use super::*;

pub(crate) fn project_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli project <validate PATH | show PATH [--json] | run PATH [--json] | report PATH --output DIR | list [DIR] [--json]>"
        );
        return Ok(ExitCode::from(2));
    };
    let rest = &args[1..];
    // `report` takes a value-bearing `--output`, and `list` takes a DIR rather
    // than a project file, so they parse their own args.
    if subcommand == "report" {
        return project_report(rest);
    }
    if subcommand == "list" {
        return project_list(rest);
    }
    let as_json = rest.iter().any(|arg| arg == "--json");
    let path = rest
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .ok_or_else(|| format!("usage: linsync-cli project {subcommand} PATH"))?;
    let path = PathBuf::from(path);
    let project = ProjectFileStore::new(path.clone())
        .load()
        .map_err(|err| format!("cannot load project '{}': {err}", path.display()))?;

    match subcommand {
        "validate" => {
            println!(
                "ok: project '{}' ({} comparison{})",
                project.name,
                project.sessions.len(),
                if project.sessions.len() == 1 { "" } else { "s" }
            );
            Ok(ExitCode::SUCCESS)
        }
        "show" => {
            if as_json {
                let items: Vec<serde_json::Value> = project
                    .sessions
                    .iter()
                    .enumerate()
                    .map(|(i, sf)| session_json(i, sf))
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "name": project.name,
                        "comparisons": items,
                    }))
                    .map_err(|err| err.to_string())?
                );
            } else {
                println!("project: {}", project.name);
                for (i, sf) in project.sessions.iter().enumerate() {
                    println!(
                        "{i}\t{}\t{} | {}",
                        compare_view_str(sf.selected_view),
                        sf.session.left.display(),
                        sf.session.right.display(),
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "run" => project_run(&project, as_json),
        other => Err(format!(
            "unknown project subcommand '{other}'; expected validate, show, run, report, or list"
        )),
    }
}

/// `project report PATH --output DIR` — write an HTML report per comparison
/// (text or folder, like the `report` command) into DIR. Exits 0 (all equal),
/// 1 (some differ), or 2 (error), matching `project run`.
pub(crate) fn project_report(args: &[String]) -> Result<ExitCode, String> {
    let mut path = None;
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                output = Some(PathBuf::from(
                    args.get(index + 1).ok_or("--output requires a directory")?,
                ));
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown project report flag '{other}'"));
            }
            other => {
                if path.is_some() {
                    return Err("project report takes a single PATH".to_owned());
                }
                path = Some(PathBuf::from(other));
                index += 1;
            }
        }
    }
    let path = path.ok_or("usage: linsync-cli project report PATH --output DIR")?;
    let output = output.ok_or("project report requires --output DIR")?;
    let project = ProjectFileStore::new(path.clone())
        .load()
        .map_err(|err| format!("cannot load project '{}': {err}", path.display()))?;
    fs::create_dir_all(&output)
        .map_err(|err| format!("cannot create output dir '{}': {err}", output.display()))?;

    let mut any_diff = false;
    let mut any_err = false;
    for (index, sf) in project.sessions.iter().enumerate() {
        let (left, right) = (&sf.session.left, &sf.session.right);
        let file = output.join(format!("{index:02}-{}.html", slugify(&sf.session.title)));
        let profile = match resolve_session_profile(sf) {
            Ok(profile) => profile,
            Err(err) => {
                any_err = true;
                eprintln!("error: comparison {index} ({}): {err}", sf.session.title);
                continue;
            }
        };
        let rendered = if left.is_dir() && right.is_dir() {
            let default_folder = FolderCompareOptions::default();
            let opts = profile.as_ref().map_or(&default_folder, |p| &p.folder);
            compare_folders(left, right, opts)
                .map_err(|err| err.to_string())
                .map(|result| {
                    if !result.is_equal() {
                        any_diff = true;
                    }
                    folder_html_report(&result, &[], ReportTreeState::Expanded, false, None)
                })
        } else {
            let default_text = TextCompareOptions::default();
            let opts = profile.as_ref().map_or(&default_text, |p| &p.text);
            compare_text_files(left, right, opts)
                .map_err(|err| err.to_string())
                .map(|result| {
                    if !result.is_equal() {
                        any_diff = true;
                    }
                    result.to_html_report_with_context(None)
                })
        };
        match rendered {
            Ok(html) => {
                fs::write(&file, html).map_err(|err| err.to_string())?;
                println!("wrote {}", file.display());
            }
            Err(err) => {
                any_err = true;
                eprintln!("error: comparison {index} ({}): {err}", sf.session.title);
            }
        }
    }

    Ok(if any_err {
        ExitCode::from(2)
    } else if any_diff {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

/// `project list [DIR] [--json]` — list `*.linsync-project` files in DIR
/// (default `.`) with their name and comparison count.
pub(crate) fn project_list(args: &[String]) -> Result<ExitCode, String> {
    let as_json = args.iter().any(|arg| arg == "--json");
    let dir = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let read = fs::read_dir(&dir)
        .map_err(|err| format!("cannot read directory '{}': {err}", dir.display()))?;
    let mut paths: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("linsync-project"))
        .collect();
    paths.sort();

    let mut items: Vec<serde_json::Value> = Vec::new();
    for path in &paths {
        match ProjectFileStore::new(path.clone()).load() {
            Ok(project) => {
                if as_json {
                    items.push(serde_json::json!({
                        "path": path.display().to_string(),
                        "name": project.name,
                        "comparisons": project.sessions.len(),
                    }));
                } else {
                    println!(
                        "{}\t{}\t{} comparison{}",
                        path.display(),
                        project.name,
                        project.sessions.len(),
                        if project.sessions.len() == 1 { "" } else { "s" }
                    );
                }
            }
            Err(err) => {
                if as_json {
                    items.push(serde_json::json!({
                        "path": path.display().to_string(),
                        "error": err.to_string(),
                    }));
                } else {
                    eprintln!("warning: {}: {err}", path.display());
                }
            }
        }
    }
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "projects": items }))
                .map_err(|err| err.to_string())?
        );
    } else if paths.is_empty() {
        println!("No project files in {}.", dir.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// Lower-case, dash-separated filename slug from a session title.
pub(crate) fn slugify(title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "comparison".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Run every comparison in a project. Directories compare as folders, otherwise
/// using the same auto-detection as `compare` (folder / text / binary / table,
/// default options). Exit 0 = all equal, 1 = some differ, 2 = error.
pub(crate) fn run_project_comparison(
    left: &Path,
    right: &Path,
    profile: Option<&CompareProfile>,
) -> Result<(&'static str, bool), String> {
    // Detect the mode from content (default options); a profile then supplies
    // the per-mode options for the chosen engine.
    let kind = detect_compare_type(left, right, &TextCompareOptions::default(), false)?;
    let (mode, equal) = match kind {
        CompareType::Folder => {
            let default_folder = FolderCompareOptions::default();
            let opts = profile.map_or(&default_folder, |p| &p.folder);
            (
                "folder",
                compare_folders(left, right, opts)
                    .map_err(|err| err.to_string())?
                    .is_equal(),
            )
        }
        CompareType::Binary | CompareType::Hex => {
            let opts = profile
                .map(|p| p.binary.clone())
                .unwrap_or(BinaryCompareOptions {
                    bytes_per_row: 16,
                    compare_content: true,
                    compare_metadata: true,
                });
            let result = compare_binary_files(left, right, &opts).map_err(|err| err.to_string())?;
            (
                "binary",
                result.differences.is_empty() && result.metadata_differences.is_empty(),
            )
        }
        CompareType::Table => {
            let default_table = TableCompareOptions::default();
            let opts = profile.map_or(&default_table, |p| &p.table);
            (
                "table",
                compare_table_files(left, right, opts)
                    .map_err(|err| err.to_string())?
                    .is_equal(),
            )
        }
        _ => {
            let default_text = TextCompareOptions::default();
            let opts = profile.map_or(&default_text, |p| &p.text);
            (
                "text",
                compare_text_files(left, right, opts)
                    .map_err(|err| err.to_string())?
                    .is_equal(),
            )
        }
    };
    Ok((mode, equal))
}

/// Resolve a session entry's optional profile id to a `CompareProfile`.
pub(crate) fn resolve_session_profile(sf: &SessionFile) -> Result<Option<CompareProfile>, String> {
    match &sf.profile {
        Some(id) => resolve_profile_arg(id)
            .map(Some)
            .map_err(|err| format!("profile '{id}': {err}")),
        None => Ok(None),
    }
}

pub(crate) fn project_run(
    project: &linsync_core::ProjectFile,
    as_json: bool,
) -> Result<ExitCode, String> {
    let mut any_diff = false;
    let mut any_err = false;
    let mut items: Vec<serde_json::Value> = Vec::new();
    for (index, sf) in project.sessions.iter().enumerate() {
        let (left, right) = (&sf.session.left, &sf.session.right);
        let outcome = resolve_session_profile(sf)
            .and_then(|profile| run_project_comparison(left, right, profile.as_ref()));
        let (status, mode, detail) = match outcome {
            Ok((mode, true)) => ("equal", mode, None),
            Ok((mode, false)) => ("different", mode, None),
            Err(err) => ("error", "", Some(err)),
        };
        match status {
            "different" => any_diff = true,
            "error" => any_err = true,
            _ => {}
        }
        if as_json {
            items.push(serde_json::json!({
                "index": index,
                "title": sf.session.title,
                "mode": mode,
                "profile": sf.profile,
                "left": left.display().to_string(),
                "right": right.display().to_string(),
                "status": status,
                "detail": detail,
            }));
        } else {
            let suffix = detail.map(|d| format!(" ({d})")).unwrap_or_default();
            let profile_note = sf
                .profile
                .as_deref()
                .map(|p| format!(" [{p}]"))
                .unwrap_or_default();
            println!(
                "{index}\t{status}\t{mode}{profile_note}\t{}\t{} | {}{suffix}",
                sf.session.title,
                left.display(),
                right.display(),
            );
        }
    }
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": project.name,
                "comparisons": items,
                "equal": !any_diff && !any_err,
            }))
            .map_err(|err| err.to_string())?
        );
    }
    Ok(if any_err {
        ExitCode::from(2)
    } else if any_diff {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
