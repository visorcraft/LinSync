use super::*;

pub(crate) fn filter_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli filter <validate RULE | validate-file PATH | list | migrate INPUT [--out OUTPUT | --in-place]>"
        );
        return Ok(ExitCode::from(2));
    };

    match subcommand {
        "validate" => {
            let Some(rule) = args.get(1) else {
                return Err("usage: linsync-cli filter validate RULE".to_owned());
            };
            print_filter_validation(rule)
        }
        "validate-file" => {
            let Some(path) = args.get(1) else {
                return Err("usage: linsync-cli filter validate-file PATH".to_owned());
            };
            let body = fs::read_to_string(path)
                .map_err(|err| format!("failed to read filter file '{path}': {err}"))?;
            print_filter_validation(&body)
        }
        "list" => {
            let store = FilterStore::new(AppPaths::from_env().filters_file());
            let filters = store.load_or_default().map_err(|err| err.to_string())?;
            if filters.filters.is_empty() {
                println!("(no saved filters)");
            } else {
                for filter in &filters.filters {
                    println!(
                        "{}: {} rule(s)",
                        filter.name.as_deref().unwrap_or("(unnamed)"),
                        filter.rules.len()
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "migrate" => filter_migrate_command(&args[1..]),
        other => Err(format!(
            "unknown filter subcommand '{other}'; expected validate, validate-file, list, or migrate"
        )),
    }
}

pub(crate) fn filter_migrate_command(args: &[String]) -> Result<ExitCode, String> {
    let mut input: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut in_place = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("filter migrate --out requires a file path".to_owned());
                };
                out = Some(value.as_str());
                index += 2;
            }
            "--in-place" => {
                in_place = true;
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown filter migrate flag '{value}'"));
            }
            value => {
                if input.is_some() {
                    return Err(
                        "usage: linsync-cli filter migrate INPUT [--out OUTPUT | --in-place]"
                            .to_owned(),
                    );
                }
                input = Some(value);
                index += 1;
            }
        }
    }

    let Some(input) = input else {
        return Err(
            "usage: linsync-cli filter migrate INPUT [--out OUTPUT | --in-place]".to_owned(),
        );
    };

    if out.is_some() && in_place {
        return Err(
            "filter migrate: --out and --in-place cannot be combined; use one or the other"
                .to_owned(),
        );
    }

    let text = fs::read_to_string(input)
        .map_err(|err| format!("failed to read filter file '{input}': {err}"))?;
    let result = linsync_core::migrate_filter_text(&text);

    // Print any warnings to stderr so they don't pollute stdout output.
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    if let Some(out_path) = out {
        fs::write(out_path, &result.migrated)
            .map_err(|err| format!("failed to write output file '{out_path}': {err}"))?;
    } else if in_place {
        // Atomic replace: write to temp file then rename.
        let temp_path = format!("{input}.migrate-tmp");
        fs::write(&temp_path, &result.migrated)
            .map_err(|err| format!("failed to write temp file '{temp_path}': {err}"))?;
        fs::rename(&temp_path, input).map_err(|err| {
            let _ = fs::remove_file(&temp_path);
            format!("failed to replace '{input}' with migrated content: {err}")
        })?;
    } else {
        print!("{}", result.migrated);
    }

    Ok(ExitCode::SUCCESS)
}

pub(crate) fn print_filter_validation(body: &str) -> Result<ExitCode, String> {
    match FileFilter::parse(body) {
        Ok(filter) => {
            println!(
                "ok: parsed filter '{}' with {} rule(s)",
                filter.name.as_deref().unwrap_or("(unnamed)"),
                filter.rules.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            let prefix = match err.kind {
                FilterParseErrorKind::UnsupportedLegacyExpression => "migration",
                FilterParseErrorKind::UnsupportedWindowsMetadata => "migration",
                _ => "error",
            };
            eprintln!("{prefix}: line {}: {}", err.line, err.message);
            Ok(ExitCode::from(if err.is_migration_hint() { 3 } else { 2 }))
        }
    }
}
