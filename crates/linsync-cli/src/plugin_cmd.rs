use super::*;

pub(crate) fn plugin_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli plugin <list [--json] | inspect ID [--json] | validate ID | enable ID | disable ID | trust ID | untrust ID | set-option ID KEY VALUE | clear-option ID KEY | install PATH | remove ID | run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]>"
        );
        return Ok(ExitCode::from(2));
    };
    let paths = AppPaths::from_env();
    let rest = &args[1..];
    let wants_json = rest.iter().any(|arg| arg == "--json");
    let first_positional = rest.iter().find(|arg| !arg.starts_with("--"));
    match subcommand {
        "list" => plugin_list(&paths, wants_json),
        "inspect" | "show" => {
            let Some(id) = first_positional else {
                return Err("usage: linsync-cli plugin inspect ID [--json]".to_owned());
            };
            plugin_inspect(&paths, id, wants_json)
        }
        "validate" => {
            let Some(id) = first_positional else {
                return Err("usage: linsync-cli plugin validate ID".to_owned());
            };
            plugin_validate(&paths, id)
        }
        "enable" | "disable" => {
            let Some(id) = first_positional else {
                return Err(format!("usage: linsync-cli plugin {subcommand} ID"));
            };
            let enabled = subcommand == "enable";
            set_plugin_enabled(&paths, id, enabled).map_err(|err| err.to_string())?;
            println!(
                "{} plugin '{id}'",
                if enabled { "enabled" } else { "disabled" }
            );
            Ok(ExitCode::SUCCESS)
        }
        "set-option" => {
            let (Some(id), Some(key), Some(raw)) = (rest.first(), rest.get(1), rest.get(2)) else {
                return Err("usage: linsync-cli plugin set-option ID KEY VALUE".to_owned());
            };
            // Parse VALUE as JSON so `true`/`7`/`"x"` get the right type; fall
            // back to a plain string for un-quoted convenience.
            let value: serde_json::Value = serde_json::from_str(raw)
                .unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()));
            set_plugin_option(&paths, id, key, value).map_err(|err| err.to_string())?;
            println!("set option '{key}' for plugin '{id}'");
            Ok(ExitCode::SUCCESS)
        }
        "clear-option" => {
            let (Some(id), Some(key)) = (rest.first(), rest.get(1)) else {
                return Err("usage: linsync-cli plugin clear-option ID KEY".to_owned());
            };
            clear_plugin_option(&paths, id, key).map_err(|err| err.to_string())?;
            println!("cleared option '{key}' for plugin '{id}'");
            Ok(ExitCode::SUCCESS)
        }
        "trust" | "untrust" => {
            let Some(id) = first_positional else {
                return Err(format!("usage: linsync-cli plugin {subcommand} ID"));
            };
            let trusted = subcommand == "trust";
            set_plugin_trusted(&paths, id, trusted).map_err(|err| err.to_string())?;
            println!(
                "{} plugin '{id}'",
                if trusted { "trusted" } else { "untrusted" }
            );
            Ok(ExitCode::SUCCESS)
        }
        "install" => {
            let Some(source) = first_positional else {
                return Err("usage: linsync-cli plugin install PATH".to_owned());
            };
            let installed = install_plugin(&paths, std::path::Path::new(source))
                .map_err(|err| err.to_string())?;
            println!(
                "installed plugin '{}' ({}) to {}",
                installed.manifest.id,
                installed.manifest.name,
                installed.root.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        "remove" | "uninstall" => {
            let Some(id) = first_positional else {
                return Err("usage: linsync-cli plugin remove ID".to_owned());
            };
            remove_plugin(&paths, id).map_err(|err| err.to_string())?;
            println!("removed plugin '{id}'");
            Ok(ExitCode::SUCCESS)
        }
        "run-diagnostic" | "diagnostic" => plugin_run_diagnostic(&paths, rest),
        other => Err(format!(
            "unknown plugin subcommand '{other}'; expected list, inspect, validate, enable, disable, trust, untrust, set-option, clear-option, install, remove, or run-diagnostic"
        )),
    }
}

/// `plugin run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]` — probe
/// a discovered plugin's helper and report exit / timeout / stdout / stderr and
/// the parsed protocol response. Exit 0 when healthy, 1 when the helper ran but
/// reported a problem, 2 on a transport/encoding error.
pub(crate) fn plugin_run_diagnostic(paths: &AppPaths, rest: &[String]) -> Result<ExitCode, String> {
    const USAGE: &str =
        "usage: linsync-cli plugin run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]";
    let mut id: Option<&str> = None;
    let mut input: Option<&str> = None;
    let mut timeout_ms: Option<u64> = None;
    let mut as_json = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => {
                as_json = true;
                index += 1;
            }
            "--input" => {
                input = Some(
                    rest.get(index + 1)
                        .ok_or("--input requires a FILE path")?
                        .as_str(),
                );
                index += 2;
            }
            "--timeout-ms" => {
                let raw = rest.get(index + 1).ok_or("--timeout-ms requires a value")?;
                timeout_ms =
                    Some(raw.parse::<u64>().map_err(|_| {
                        format!("invalid --timeout-ms '{raw}': expected an integer")
                    })?);
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag '{other}'; {USAGE}"));
            }
            other => {
                if id.is_some() {
                    return Err(format!(
                        "plugin run-diagnostic takes a single plugin ID; {USAGE}"
                    ));
                }
                id = Some(other);
                index += 1;
            }
        }
    }
    let id = id.ok_or(USAGE)?;

    let discovery = discover_installed_plugins(paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;

    let mut inputs = Vec::new();
    if let Some(path) = input {
        let path_buf = PathBuf::from(path);
        if !path_buf.exists() {
            return Err(format!("--input file '{path}' does not exist"));
        }
        let extension = path_buf
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned());
        inputs.push(PluginInputDescriptor {
            role: "input".to_owned(),
            path: path_buf,
            display_name: None,
            mime_type: None,
            extension,
            read_only: true,
        });
    }

    let mut options = PluginExecutionOptions::default();
    if let Some(ms) = timeout_ms {
        options.timeout = Duration::from_millis(ms);
    }

    let sandbox = active_sandbox_status();
    let outcome = probe_plugin(&plugin.root, &plugin.manifest, inputs, &options)
        .map_err(|err| err.to_string())?;

    if as_json {
        let response = outcome.response.as_ref().map(|r| {
            serde_json::json!({
                "status": format!("{:?}", r.status).to_lowercase(),
                "diagnostics": r
                    .diagnostics
                    .iter()
                    .map(|d| serde_json::json!({"severity": d.severity, "message": d.message}))
                    .collect::<Vec<_>>(),
                "error": r
                    .error
                    .as_ref()
                    .map(|e| serde_json::json!({"code": e.code, "message": e.message})),
            })
        });
        let body = serde_json::json!({
            "id": id,
            "healthy": outcome.is_healthy(),
            "exit_code": outcome.exit_code,
            "timed_out": outcome.timed_out,
            "stdout": outcome.stdout,
            "stderr": outcome.stderr,
            "response": response,
            "sandbox": {"label": sandbox.label, "confined": sandbox.confined},
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        );
    } else {
        println!("plugin:    {id}");
        println!("healthy:   {}", outcome.is_healthy());
        println!(
            "sandbox:   {} (confined={})",
            sandbox.label, sandbox.confined
        );
        match outcome.exit_code {
            Some(code) => println!("exit:      {code}"),
            None => println!("exit:      (none)"),
        }
        println!("timed_out: {}", outcome.timed_out);
        if let Some(response) = &outcome.response {
            println!("status:    {:?}", response.status);
            for d in &response.diagnostics {
                println!("  diagnostic [{}]: {}", d.severity, d.message);
            }
            if let Some(err) = &response.error {
                println!("  error [{}]: {}", err.code, err.message);
            }
        } else if !outcome.stdout.trim().is_empty() {
            println!("stdout:    {}", outcome.stdout.trim());
        }
        if !outcome.stderr.trim().is_empty() {
            println!("stderr:    {}", outcome.stderr.trim());
        }
    }

    Ok(if outcome.is_healthy() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub(crate) fn plugin_class_names(classes: &[linsync_core::PluginClass]) -> Vec<String> {
    classes.iter().map(|c| format!("{c:?}")).collect()
}

pub(crate) fn plugin_list(paths: &AppPaths, as_json: bool) -> Result<ExitCode, String> {
    let discovery = discover_installed_plugins(paths);
    let enabled = load_plugin_enabled_map(paths);
    let trusted = load_plugin_trusted_map(paths);
    if as_json {
        let plugins: Vec<serde_json::Value> = discovery
            .plugins
            .iter()
            .map(|p| {
                let m = &p.manifest;
                serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "version": m.version,
                    "classes": plugin_class_names(&m.classes),
                    "enabled": enabled.get(&m.id).copied().unwrap_or(true),
                    "trusted": trusted.get(&m.id).copied().unwrap_or(false),
                    "has_options": !m.options_schema.is_empty(),
                })
            })
            .collect();
        let errors: Vec<serde_json::Value> = discovery
            .errors
            .iter()
            .map(
                |e| serde_json::json!({"path": e.path.display().to_string(), "message": e.message}),
            )
            .collect();
        let body = serde_json::json!({ "plugins": plugins, "errors": errors });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        );
    } else {
        if discovery.plugins.is_empty() {
            println!("No plugins discovered.");
        }
        for p in &discovery.plugins {
            let m = &p.manifest;
            let state = if enabled.get(&m.id).copied().unwrap_or(true) {
                "enabled"
            } else {
                "disabled"
            };
            let opts = if m.options_schema.is_empty() {
                ""
            } else {
                " [options]"
            };
            println!(
                "{}\t{}\t{}{}",
                m.id,
                state,
                plugin_class_names(&m.classes).join(","),
                opts
            );
        }
        for e in &discovery.errors {
            eprintln!("error: {}: {}", e.path.display(), e.message);
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn plugin_inspect(
    paths: &AppPaths,
    id: &str,
    as_json: bool,
) -> Result<ExitCode, String> {
    let discovery = discover_installed_plugins(paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;
    let m = &plugin.manifest;
    let enabled = load_plugin_enabled_map(paths)
        .get(id)
        .copied()
        .unwrap_or(true);
    let trusted = linsync_core::is_plugin_trusted(paths, id);
    let values = load_plugin_options(paths, id);
    if as_json {
        let schema: Vec<serde_json::Value> = m
            .options_schema
            .iter()
            .map(|o| {
                serde_json::json!({
                    "key": o.key,
                    "label": o.label,
                    "kind": format!("{:?}", o.kind).to_lowercase(),
                    "default": o.default,
                    "choices": o.choices,
                })
            })
            .collect();
        let body = serde_json::json!({
            "id": m.id,
            "name": m.name,
            "version": m.version,
            "license": m.license,
            "classes": plugin_class_names(&m.classes),
            "enabled": enabled,
            "trusted": trusted,
            "root": plugin.root.display().to_string(),
            "options_schema": schema,
            "values": values,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        );
    } else {
        println!("id:       {}", m.id);
        println!("name:     {}", m.name);
        println!("version:  {}", m.version);
        println!("license:  {}", m.license);
        println!("classes:  {}", plugin_class_names(&m.classes).join(", "));
        println!("enabled:  {enabled}");
        println!("trusted:  {trusted}");
        println!("root:     {}", plugin.root.display());
        if m.options_schema.is_empty() {
            println!("options:  (none)");
        } else {
            println!("options:");
            for o in &m.options_schema {
                let current = values
                    .get(&o.key)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "<unset>".to_owned());
                let choices = if o.choices.is_empty() {
                    String::new()
                } else {
                    format!(" choices=[{}]", o.choices.join(","))
                };
                println!("  {} ({:?}){}  current={current}", o.key, o.kind, choices);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn plugin_validate(paths: &AppPaths, id: &str) -> Result<ExitCode, String> {
    let discovery = discover_installed_plugins(paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;
    let values = load_plugin_options(paths, id);
    match plugin.manifest.validate_options(&values) {
        Ok(()) => {
            println!(
                "plugin '{id}' options are valid ({} option(s) set)",
                values.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("invalid: {err}");
            Ok(ExitCode::from(1))
        }
    }
}
