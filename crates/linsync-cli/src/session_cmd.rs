use super::*;

pub(crate) const SESSION_HISTORY_FALLBACK_LIMIT: usize = 20;

/// Match the GUI's history depth so CLI saves and GUI saves truncate alike.
pub(crate) fn session_history_limit(paths: &AppPaths) -> usize {
    SettingsStore::new(paths.settings_file())
        .load_or_default()
        .map(|settings| settings.recent_limit)
        .unwrap_or(SESSION_HISTORY_FALLBACK_LIMIT)
}

pub(crate) fn parse_compare_view(value: &str) -> Result<CompareViewMode, String> {
    Ok(match value {
        "text" => CompareViewMode::Text,
        "folder" => CompareViewMode::Folder,
        "binary" => CompareViewMode::Binary,
        "table" => CompareViewMode::Table,
        "image" => CompareViewMode::Image,
        "document" => CompareViewMode::Document,
        "archive" => CompareViewMode::Archive,
        "webpage" => CompareViewMode::Webpage,
        other => {
            return Err(format!(
                "unknown --view '{other}': expected text|folder|binary|table|image|document|archive|webpage"
            ));
        }
    })
}

pub(crate) fn compare_view_str(view: CompareViewMode) -> String {
    format!("{view:?}").to_lowercase()
}

/// `session <save | list | show | clear>` — manage the recent-session history
/// shared with the GUI (`recent-sessions.json`). A saved session is restored by
/// the GUI on next launch when "open last session" is enabled.
pub(crate) fn session_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli session <save LEFT RIGHT [--base BASE] [--title T] [--view MODE] [--profile ID] | list [--json] | show [INDEX] [--json] | clear>"
        );
        return Ok(ExitCode::from(2));
    };
    let paths = AppPaths::from_env();
    let store =
        RecentSessionStore::new(paths.recent_sessions_file(), session_history_limit(&paths));
    let rest = &args[1..];
    match subcommand {
        "save" => session_save(&store, rest),
        "list" => session_list(&store, rest),
        "show" => session_show(&store, rest),
        "clear" => {
            store
                .save(&linsync_core::RecentSessions::default())
                .map_err(|err| err.to_string())?;
            println!("cleared session history");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!(
            "unknown session subcommand '{other}'; expected save, list, show, or clear"
        )),
    }
}

pub(crate) fn session_save(
    store: &RecentSessionStore,
    args: &[String],
) -> Result<ExitCode, String> {
    let mut positionals = Vec::new();
    let mut base = None;
    let mut title = None;
    let mut view = CompareViewMode::default();
    let mut profile = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--base" => {
                base = Some(PathBuf::from(
                    args.get(index + 1).ok_or("--base requires a path")?,
                ));
                index += 2;
            }
            "--title" => {
                title = Some(
                    args.get(index + 1)
                        .ok_or("--title requires a value")?
                        .clone(),
                );
                index += 2;
            }
            "--profile" => {
                profile = Some(
                    args.get(index + 1)
                        .ok_or("--profile requires an id")?
                        .clone(),
                );
                index += 2;
            }
            "--view" => {
                view = parse_compare_view(args.get(index + 1).ok_or("--view requires a value")?)?;
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown session save flag '{other}'"));
            }
            other => {
                positionals.push(other.to_owned());
                index += 1;
            }
        }
    }
    if positionals.len() != 2 {
        return Err(
            "usage: linsync-cli session save LEFT RIGHT [--base BASE] [--title T] [--view MODE] [--profile ID]"
                .to_owned(),
        );
    }
    let title = title.unwrap_or_else(|| format!("{} vs {}", positionals[0], positionals[1]));
    let mut session = SessionFile::new(CompareSession {
        title: title.clone(),
        left: PathBuf::from(&positionals[0]),
        base,
        right: PathBuf::from(&positionals[1]),
        options: CompareOptions::default(),
    });
    session.selected_view = view;
    session.profile = profile;
    let recent = store.add(session).map_err(|err| err.to_string())?;
    println!(
        "saved session '{title}' ({} in history)",
        recent.sessions.len()
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn session_list(
    store: &RecentSessionStore,
    args: &[String],
) -> Result<ExitCode, String> {
    let as_json = args.iter().any(|arg| arg == "--json");
    let recent = store.load_or_default().map_err(|err| err.to_string())?;
    if as_json {
        let items: Vec<serde_json::Value> = recent
            .sessions
            .iter()
            .enumerate()
            .map(|(i, sf)| session_json(i, sf))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "sessions": items }))
                .map_err(|err| err.to_string())?
        );
    } else if recent.sessions.is_empty() {
        println!("No saved sessions.");
    } else {
        for (i, sf) in recent.sessions.iter().enumerate() {
            println!(
                "{i}\t{}\t{}\t{} | {}",
                compare_view_str(sf.selected_view),
                sf.session.title,
                sf.session.left.display(),
                sf.session.right.display(),
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn session_show(
    store: &RecentSessionStore,
    args: &[String],
) -> Result<ExitCode, String> {
    let as_json = args.iter().any(|arg| arg == "--json");
    let index = match args.iter().find(|arg| !arg.starts_with("--")) {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| format!("invalid session index '{raw}'"))?,
        None => 0,
    };
    let recent = store.load_or_default().map_err(|err| err.to_string())?;
    let sf = recent.sessions.get(index).ok_or_else(|| {
        format!(
            "no session at index {index} ({} in history)",
            recent.sessions.len()
        )
    })?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&session_json(index, sf)).map_err(|err| err.to_string())?
        );
    } else {
        println!("index: {index}");
        println!("title: {}", sf.session.title);
        println!("view:  {}", compare_view_str(sf.selected_view));
        println!("left:  {}", sf.session.left.display());
        if let Some(base) = &sf.session.base {
            println!("base:  {}", base.display());
        }
        println!("right: {}", sf.session.right.display());
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn session_json(index: usize, sf: &SessionFile) -> serde_json::Value {
    serde_json::json!({
        "index": index,
        "title": sf.session.title,
        "view": compare_view_str(sf.selected_view),
        "profile": sf.profile,
        "left": sf.session.left.display().to_string(),
        "right": sf.session.right.display().to_string(),
        "base": sf.session.base.as_ref().map(|b| b.display().to_string()),
    })
}
