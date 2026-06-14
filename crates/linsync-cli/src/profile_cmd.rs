use super::*;

pub(crate) fn profile_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli profile <list | show ID | validate (ID|PATH) | import PATH | export ID [--output PATH] | delete ID>"
        );
        return Ok(ExitCode::from(2));
    };

    let store = profile_store();
    match subcommand {
        "list" => profile_list(&store),
        "show" => {
            let Some(target) = args.get(1) else {
                return Err("usage: linsync-cli profile show ID".to_owned());
            };
            profile_show(target)
        }
        "validate" => {
            let Some(target) = args.get(1) else {
                return Err("usage: linsync-cli profile validate (ID|PATH)".to_owned());
            };
            profile_validate(target)
        }
        "import" => {
            let Some(path) = args.get(1) else {
                return Err("usage: linsync-cli profile import PATH".to_owned());
            };
            profile_import(&store, path)
        }
        "export" => profile_export(&store, &args[1..]),
        "delete" => {
            let Some(id) = args.get(1) else {
                return Err("usage: linsync-cli profile delete ID".to_owned());
            };
            profile_delete(&store, id)
        }
        other => Err(format!(
            "unknown profile subcommand '{other}'; expected list, show, validate, import, export, or delete"
        )),
    }
}

pub(crate) fn profile_store() -> ProfileStore {
    let paths = AppPaths::from_env();
    ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file())
}

pub(crate) fn profile_list(store: &ProfileStore) -> Result<ExitCode, String> {
    for p in builtin_profiles() {
        println!("{}\t[built-in]\t{}", p.id, p.name);
    }
    let user_ids = store.list_user_ids().map_err(|err| err.to_string())?;
    for id in user_ids {
        match store.load(&id) {
            Ok(p) => println!("{}\t[user]\t{}", id, p.name),
            Err(err) => println!("{}\t[user, error]\t{err}", id),
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn profile_show(target: &str) -> Result<ExitCode, String> {
    let profile = resolve_profile_arg(target)?;
    let json = serde_json::to_string_pretty(&profile)
        .map_err(|err| format!("failed to serialize profile: {err}"))?;
    println!("{json}");
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn profile_validate(target: &str) -> Result<ExitCode, String> {
    match resolve_profile_arg(target) {
        Ok(p) => match p.validate() {
            Ok(()) => {
                println!("profile {} is valid", p.id);
                Ok(ExitCode::SUCCESS)
            }
            Err(err) => Err(format!("profile {} is invalid: {err}", p.id)),
        },
        Err(err) => Err(err),
    }
}

pub(crate) fn profile_import(store: &ProfileStore, src: &str) -> Result<ExitCode, String> {
    let bytes = fs::read(src).map_err(|err| format!("failed to read '{src}': {err}"))?;
    let profile: CompareProfile =
        serde_json::from_slice(&bytes).map_err(|err| format!("failed to parse '{src}': {err}"))?;
    profile
        .validate()
        .map_err(|err| format!("profile in '{src}' is invalid: {err}"))?;
    store
        .save(&profile)
        .map_err(|err| format!("failed to save profile: {err}"))?;
    println!("imported profile {} as user profile", profile.id);
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn profile_export(store: &ProfileStore, args: &[String]) -> Result<ExitCode, String> {
    let mut target: Option<&str> = None;
    let mut output: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("--output requires a path".to_owned());
                };
                output = Some(v.as_str());
                i += 2;
            }
            value if !value.starts_with("--") && target.is_none() => {
                target = Some(value);
                i += 1;
            }
            other => return Err(format!("unexpected profile export argument '{other}'")),
        }
    }
    let Some(target) = target else {
        return Err("usage: linsync-cli profile export ID [--output PATH]".to_owned());
    };
    let profile = resolve_profile_id_only(store, target)?;
    let bytes = serde_json::to_vec_pretty(&profile)
        .map_err(|err| format!("failed to serialize profile: {err}"))?;
    match output {
        Some(path) => {
            fs::write(path, &bytes).map_err(|err| format!("failed to write '{path}': {err}"))?;
            println!("exported profile {} to {path}", profile.id);
        }
        None => {
            std::io::Write::write_all(&mut std::io::stdout(), &bytes)
                .map_err(|err| format!("failed to write to stdout: {err}"))?;
            println!();
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn profile_delete(store: &ProfileStore, id_str: &str) -> Result<ExitCode, String> {
    let id = ProfileId::new(id_str.to_owned())
        .map_err(|err| format!("invalid profile id '{id_str}': {err}"))?;
    match store.delete(&id) {
        Ok(()) => {
            println!("deleted profile {id}");
            Ok(ExitCode::SUCCESS)
        }
        Err(ProfileStoreError::NotFound(_)) => {
            // Maybe the user is trying to delete a built-in; report a
            // clearer error in that case.
            if find_builtin(&id).is_some() {
                Err(format!("profile {id} is a built-in and cannot be deleted"))
            } else {
                Err(format!("no user profile named {id}"))
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Resolve a `--profile X` argument to a [`CompareProfile`]. Accepts
/// a built-in id, a user-store id, or a path to a JSON file.
pub(crate) fn resolve_profile_arg(value: &str) -> Result<CompareProfile, String> {
    // Heuristic: anything containing '/' or ending in `.json` is a file
    // path. Otherwise treat it as a profile id.
    let looks_like_path = value.contains('/') || value.ends_with(".json");
    if looks_like_path {
        let bytes = fs::read(value)
            .map_err(|err| format!("failed to read profile file '{value}': {err}"))?;
        let profile: CompareProfile = serde_json::from_slice(&bytes)
            .map_err(|err| format!("failed to parse profile file '{value}': {err}"))?;
        profile
            .validate()
            .map_err(|err| format!("profile in '{value}' is invalid: {err}"))?;
        return Ok(profile);
    }
    let id = ProfileId::new(value.to_owned())
        .map_err(|err| format!("invalid profile id '{value}': {err}"))?;
    if let Some(p) = find_builtin(&id) {
        return Ok(p);
    }
    let store = profile_store();
    match store.load(&id) {
        Ok(p) => Ok(p),
        Err(ProfileStoreError::NotFound(_)) => {
            let known: Vec<String> = builtin_profiles()
                .into_iter()
                .map(|p| p.id.to_string())
                .collect();
            Err(format!(
                "no profile named '{value}'. Known built-ins: {}. Use `linsync-cli profile list` for the full list.",
                known.join(", ")
            ))
        }
        Err(err) => Err(format!("failed to load user profile '{value}': {err}")),
    }
}

pub(crate) fn resolve_profile_id_only(
    store: &ProfileStore,
    value: &str,
) -> Result<CompareProfile, String> {
    let id = ProfileId::new(value.to_owned())
        .map_err(|err| format!("invalid profile id '{value}': {err}"))?;
    if let Some(p) = find_builtin(&id) {
        return Ok(p);
    }
    store
        .load(&id)
        .map_err(|err| format!("failed to load profile '{value}': {err}"))
}
