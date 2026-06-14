use super::*;

pub(crate) fn patch_command(args: &[String]) -> Result<ExitCode, String> {
    let patch_args = split_patch_args(args)?;
    if patch_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli patch LEFT RIGHT [--format unified|context|normal] [--context LINES] [--preview|--output FILE]"
                .to_owned(),
        );
    }

    let left = PathBuf::from(&patch_args.paths[0]);
    let right = PathBuf::from(&patch_args.paths[1]);
    if left.is_dir() || right.is_dir() {
        if !(left.is_dir() && right.is_dir()) {
            return Err("patch paths must both be files or both be directories".to_owned());
        }
        return patch_folder_command(&left, &right, patch_args);
    }

    let result = compare_text_files(
        left.as_path(),
        right.as_path(),
        &TextCompareOptions::default(),
    )
    .map_err(|err| err.to_string())?;
    let patch = render_text_patch(&result, patch_args.format, patch_args.context);

    if patch_args.preview {
        print!("{patch}");
    } else if let Some(output) = patch_args.output {
        fs::write(output, patch).map_err(|err| err.to_string())?;
    } else {
        print!("{patch}");
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub(crate) fn patch_folder_command(
    left: &Path,
    right: &Path,
    patch_args: PatchArgs,
) -> Result<ExitCode, String> {
    let result = compare_folders(left, right, &FolderCompareOptions::default())
        .map_err(|err| err.to_string())?;
    let mut patch = String::new();

    for entry in &result.entries {
        if entry.is_dir
            || matches!(
                entry.state,
                FolderEntryState::Identical | FolderEntryState::Skipped | FolderEntryState::Aborted
            )
        {
            continue;
        }
        if entry.state == FolderEntryState::Error {
            return Err(format!(
                "cannot generate patch for '{}': {}",
                entry.relative_path.display(),
                entry.error.as_deref().unwrap_or("folder compare error")
            ));
        }

        let left_path = left.join(&entry.relative_path);
        let right_path = right.join(&entry.relative_path);
        let text_result = match entry.state {
            FolderEntryState::Different => {
                let left_text = read_representable_text(&left_path)?;
                let right_text = read_representable_text(&right_path)?;
                compare_text(
                    &left_path.display().to_string(),
                    &left_text,
                    &right_path.display().to_string(),
                    &right_text,
                    &TextCompareOptions::default(),
                )
            }
            FolderEntryState::LeftOnly => {
                let left_text = read_representable_text(&left_path)?;
                compare_text(
                    &left_path.display().to_string(),
                    &left_text,
                    "/dev/null",
                    "",
                    &TextCompareOptions::default(),
                )
            }
            FolderEntryState::RightOnly => {
                let right_text = read_representable_text(&right_path)?;
                compare_text(
                    "/dev/null",
                    "",
                    &right_path.display().to_string(),
                    &right_text,
                    &TextCompareOptions::default(),
                )
            }
            FolderEntryState::Identical
            | FolderEntryState::Skipped
            | FolderEntryState::Error
            | FolderEntryState::Aborted => continue,
        };

        patch.push_str(&render_text_patch(
            &text_result,
            patch_args.format,
            patch_args.context,
        ));
    }

    if patch_args.preview {
        print!("{patch}");
    } else if let Some(output) = patch_args.output {
        fs::write(output, patch).map_err(|err| err.to_string())?;
    } else {
        print!("{patch}");
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub(crate) fn render_text_patch(
    result: &linsync_core::TextCompareResult,
    format: PatchFormat,
    context: usize,
) -> String {
    match format {
        PatchFormat::Unified => result.to_unified_diff(context),
        PatchFormat::Context => result.to_context_diff(context),
        PatchFormat::Normal => result.to_normal_diff(),
    }
}

pub(crate) fn read_representable_text(path: &Path) -> Result<String, String> {
    read_utf8_text_for_export(path, "folder patch")
}

pub(crate) fn read_nested_report_text(path: &Path) -> Result<String, String> {
    read_utf8_text_for_export(path, "nested file report")
}

pub(crate) fn read_utf8_text_for_export(path: &Path, purpose: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    if is_likely_binary(&bytes) {
        return Err(format!(
            "{purpose} cannot represent binary file '{}'",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|err| {
        format!(
            "{purpose} requires UTF-8 text for '{}': {err}",
            path.display()
        )
    })
}
