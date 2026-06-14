use super::*;

pub(crate) fn webpage_command(args: &[String]) -> Result<ExitCode, String> {
    let mut urls: Vec<&str> = Vec::new();
    let mut sub_mode = "html";
    let mut depth: u8 = 1;
    let mut timeout: u32 = 30;
    let mut max_requests: u32 = 50;
    let mut accept_network = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sub-mode" => {
                i += 1;
                sub_mode = args.get(i).map(String::as_str).unwrap_or("html");
            }
            "--depth" => {
                i += 1;
                depth = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1u8)
                    .clamp(1, 3);
            }
            "--timeout" => {
                i += 1;
                timeout = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(30);
            }
            "--max-requests" => {
                i += 1;
                max_requests = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(50);
            }
            "--accept-network-fetch" => accept_network = true,
            other if !other.starts_with('-') => urls.push(other),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }

    if urls.len() != 2 {
        return Err("webpage requires exactly two URL arguments".to_string());
    }

    if !accept_network {
        eprintln!("error: network fetch requires --accept-network-fetch");
        return Ok(ExitCode::from(2));
    }

    let options = linsync_core::WebpageCompareOptions {
        resource_tree_depth: depth,
        timeout_secs: timeout,
        max_requests,
        confirmed_by_user: true,
        user_agent: None,
    };
    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;

    let result = match sub_mode {
        "html" => linsync_core::compare_webpage_html_source(urls[0], urls[1], &options, &cache_dir),
        "text" => {
            linsync_core::compare_webpage_extracted_text(urls[0], urls[1], &options, &cache_dir)
        }
        "tree" => {
            linsync_core::compare_webpage_resource_tree(urls[0], urls[1], &options, &cache_dir)
        }
        #[cfg(feature = "web-engine")]
        "rendered" => {
            linsync_core::compare_webpage_rendered(urls[0], urls[1], &options, &cache_dir)
        }
        #[cfg(feature = "web-engine")]
        "screenshot" => {
            linsync_core::compare_webpage_screenshot(urls[0], urls[1], &options, &cache_dir)
        }
        #[cfg(not(feature = "web-engine"))]
        "rendered" | "screenshot" => {
            eprintln!("error: {sub_mode} mode requires the web-engine build feature");
            return Ok(ExitCode::from(2));
        }
        other => return Err(format!("unknown sub-mode: {other}")),
    };

    // Collapse via WebpageCompareResult::is_equal() rather than matching the
    // result variants: the Rendered/Screenshot variants only exist when
    // linsync-core's `web-engine` feature is on, which a sibling crate (the
    // GUI) can enable workspace-wide independently of linsync-cli's own
    // `web-engine` feature — a variant `match` here would then be non-exhaustive
    // and break the packaging/CI builds. is_equal() handles every variant
    // inside linsync-core, where the cfg is authoritative.
    match result {
        Ok(cmp) => {
            if cmp.is_equal() {
                println!("identical");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        Err(linsync_core::WebpageCompareError::ConfirmationRequired) => {
            eprintln!("error: network fetch requires --accept-network-fetch");
            Ok(ExitCode::from(2))
        }
        Err(e) => Err(e.to_string()),
    }
}
