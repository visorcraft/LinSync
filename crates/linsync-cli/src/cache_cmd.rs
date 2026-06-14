use super::*;

pub(crate) fn cache_command(args: &[String]) -> Result<ExitCode, String> {
    let subcommand = args.first().map(String::as_str).unwrap_or("");
    match subcommand {
        "clear" => {
            let scope = args.get(1).map(String::as_str).unwrap_or("all");
            match scope {
                "--scope" => {
                    let scope_val = args.get(2).map(String::as_str).unwrap_or("");
                    match scope_val {
                        "webcompare" => {
                            let cache_dir = linsync_core::AppPaths::from_env().cache_dir;
                            linsync_core::clear_webcompare_cache(&cache_dir)
                                .map_err(|e| e.to_string())?;
                            println!("webcompare cache cleared");
                            Ok(ExitCode::SUCCESS)
                        }
                        other => Err(format!("unknown cache scope '{other}'")),
                    }
                }
                "webcompare" => {
                    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;
                    linsync_core::clear_webcompare_cache(&cache_dir).map_err(|e| e.to_string())?;
                    println!("webcompare cache cleared");
                    Ok(ExitCode::SUCCESS)
                }
                other => Err(format!("unknown cache clear scope '{other}'")),
            }
        }
        other => Err(format!(
            "unknown cache subcommand '{other}'; expected: clear [--scope webcompare]"
        )),
    }
}
