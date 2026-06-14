use super::*;

pub(crate) fn archive_command(args: &[String]) -> Result<ExitCode, String> {
    let mut paths = Vec::new();
    let mut keep_temp = false;
    let mut json = false;
    let mut unpacker = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--keep-temp" => {
                keep_temp = true;
                index += 1;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--unpacker" => {
                unpacker = Some(
                    args.get(index + 1)
                        .ok_or("--unpacker requires a plugin id")?
                        .clone(),
                );
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown archive flag '{value}'"));
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }
    if paths.len() != 2 {
        return Err(
            "usage: linsync-cli archive [--keep-temp] [--json] [--unpacker PLUGIN_ID] LEFT.{zip|tar|tgz|...} RIGHT.{...}"
                .to_owned(),
        );
    }

    if let Some(id) = unpacker {
        return archive_compare_via_plugin(&id, &paths[0], &paths[1], json);
    }

    // Auto-route to a folder-virtualizer plugin for extensions the built-in
    // extractor cannot read, when one is installed + enabled and declares the
    // extension. Supported built-in formats always use the built-in path.
    if !builtin_archive_supported(&paths[0]) {
        let ext = Path::new(&paths[0])
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if let Some(plugin) =
            resolve_enabled_virtualizer_for_extension(&AppPaths::from_env(), ext, None)
        {
            return archive_compare_via_plugin(&plugin.manifest.id, &paths[0], &paths[1], json);
        }
    }

    let cache_root = AppPaths::from_env().comparison_cache_dir();
    fs::create_dir_all(&cache_root).map_err(|err| format!("cannot prepare cache dir: {err}"))?;

    let mut left = extract_archive(&PathBuf::from(&paths[0]), &cache_root, "left")?;
    let mut right = extract_archive(&PathBuf::from(&paths[1]), &cache_root, "right")?;
    if keep_temp {
        left.keep();
        right.keep();
    }

    let result = compare_folders(&left.path, &right.path, &FolderCompareOptions::default())
        .map_err(|err| format!("folder compare failed: {err}"))?;

    if json {
        let body = serde_json::json!({
            "left": { "archive": paths[0], "extracted_to": left.path.display().to_string() },
            "right": { "archive": paths[1], "extracted_to": right.path.display().to_string() },
            "summary": {
                "compared": result.summary.compared_count,
                "identical": result.summary.identical_count,
                "different": result.summary.different_count,
                "one_sided": result.summary.one_sided_count,
                "errors": result.summary.errors_count,
            },
        });
        println!("{body}");
    } else {
        println!(
            "compared={} identical={} different={} one_sided={} errors={}",
            result.summary.compared_count,
            result.summary.identical_count,
            result.summary.different_count,
            result.summary.one_sided_count,
            result.summary.errors_count,
        );
    }

    // `left`/`right` clean their extracted trees on drop (best-effort
    // `remove_dir_all`) unless `keep_temp` flagged them to be retained, so the
    // cache dir is never leaked even on the error paths above.

    let code = if result.summary.different_count > 0
        || result.summary.one_sided_count > 0
        || result.summary.errors_count > 0
    {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    };
    Ok(code)
}

/// Archive name suffixes the built-in tar/unzip extractor handles.
pub(crate) const BUILTIN_ARCHIVE_SUFFIXES: &[&str] = &[
    ".zip", ".jar", ".war", ".apk", ".ipa", ".tar", ".tgz", ".tar.gz", ".tbz2", ".tar.bz2", ".txz",
    ".tar.xz", ".tzst", ".tar.zst",
];

/// Whether the built-in extractor recognizes this archive name's extension.
pub(crate) fn builtin_archive_supported(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    BUILTIN_ARCHIVE_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

/// Compare two archives by routing each through a folder-virtualizer / unpacker
/// plugin (`unpack_folder`) and comparing the resulting virtual trees, instead
/// of the built-in extractor. Useful for formats the built-in cannot read.
pub(crate) fn archive_compare_via_plugin(
    id: &str,
    left_archive: &str,
    right_archive: &str,
    json: bool,
) -> Result<ExitCode, String> {
    let paths = AppPaths::from_env();
    let discovery = discover_installed_plugins(&paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;
    if !plugin.manifest.classes.iter().any(|c| {
        matches!(
            c,
            linsync_core::PluginClass::Unpacker | linsync_core::PluginClass::FolderVirtualizer
        )
    }) {
        return Err(format!(
            "plugin '{id}' does not declare the unpacker or folder_virtualizer class"
        ));
    }

    let options = PluginExecutionOptions::default();
    // Core owns the plugin-based archive comparison; the CLI just renders it.
    let result = compare_archives_with_unpacker(
        &plugin.root,
        &plugin.manifest,
        left_archive,
        right_archive,
        &options,
    )
    .map_err(|err| err.to_string())?;
    let summary = &result.summary;
    // The unpacker helper ran under the sandbox; surface its confinement so a
    // degraded run is visible rather than silent.
    let sandbox = active_sandbox_status();

    if json {
        let body = serde_json::json!({
            "left": { "archive": left_archive, "unpacker": id },
            "right": { "archive": right_archive, "unpacker": id },
            "equal": result.is_equal(),
            "sandbox": { "label": sandbox.label, "confined": sandbox.confined },
            "summary": {
                "compared": summary.compared_count,
                "identical": summary.identical_count,
                "different": summary.different_count,
                "one_sided": summary.one_sided_count,
                "left_only": summary.left_only_count,
                "right_only": summary.right_only_count,
            },
        });
        println!("{body}");
    } else {
        println!(
            "unpacker={id} sandbox={} compared={} identical={} different={} one_sided={} left_only={} right_only={}",
            sandbox.label,
            summary.compared_count,
            summary.identical_count,
            summary.different_count,
            summary.one_sided_count,
            summary.left_only_count,
            summary.right_only_count,
        );
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub(crate) struct ExtractedArchive {
    pub(crate) path: PathBuf,
    pub(crate) temp_root: PathBuf,
    pub(crate) keep: bool,
}

impl ExtractedArchive {
    /// Retain the extracted tree past drop (honors `--keep-temp`).
    pub(crate) fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for ExtractedArchive {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.temp_root);
        }
    }
}

pub(crate) fn extract_archive(
    archive: &Path,
    cache_root: &Path,
    side: &str,
) -> Result<ExtractedArchive, String> {
    if !archive.is_file() {
        return Err(format!("archive '{}' is not a file", archive.display()));
    }
    let stem = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".to_owned());
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_root = cache_root.join(format!("archive-{side}-{timestamp}-{stem}"));
    let extracted = temp_root.join("extracted");
    fs::create_dir_all(&extracted).map_err(|err| {
        format!(
            "cannot create extraction dir '{}': {err}",
            extracted.display()
        )
    })?;

    let lower = stem.to_ascii_lowercase();
    let policy = SandboxPolicy::builder()
        .read(archive)
        .read(&extracted)
        .write(&extracted)
        .build();
    let status = if lower.ends_with(".zip")
        || lower.ends_with(".jar")
        || lower.ends_with(".war")
        || lower.ends_with(".apk")
        || lower.ends_with(".ipa")
    {
        let mut cmd = Command::new("unzip");
        cmd.arg("-q")
            .arg("-o")
            .arg("-d")
            .arg(&extracted)
            .arg(archive);
        match SandboxedCommand::new(cmd, policy).spawn() {
            Ok(mut child) => child
                .wait()
                .map_err(|err| format!("unzip wait failed: {err}"))?,
            Err(err) => {
                return Err(format!(
                    "sandboxed unzip failed (set LINSYNC_SANDBOX_ALLOW_UNSANDBOXED=1 to bypass): {err}"
                ));
            }
        }
    } else if lower.ends_with(".tar")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".txz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tzst")
        || lower.ends_with(".tar.zst")
    {
        let mut cmd = Command::new("tar");
        cmd.arg("-xf").arg(archive).arg("-C").arg(&extracted);
        match SandboxedCommand::new(cmd, policy).spawn() {
            Ok(mut child) => child
                .wait()
                .map_err(|err| format!("tar wait failed: {err}"))?,
            Err(err) => {
                return Err(format!(
                    "sandboxed tar failed (set LINSYNC_SANDBOX_ALLOW_UNSANDBOXED=1 to bypass): {err}"
                ));
            }
        }
    } else {
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!(
            "unsupported archive extension for '{}'; install a plugin or use a supported type (zip, jar, tar, tgz, tar.xz, tar.zst, ...)",
            archive.display()
        ));
    };

    if !status.success() {
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!(
            "archive extraction failed for '{}': exit status {status}",
            archive.display()
        ));
    }

    Ok(ExtractedArchive {
        path: extracted,
        temp_root,
        keep: false,
    })
}

// ── Profile management ───────────────────────────────────────────────────────
