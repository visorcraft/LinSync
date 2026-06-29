use super::*;

pub(crate) fn is_table_path(path: &Path) -> bool {
    is_csv_path(path) || is_tsv_path(path)
}

pub(crate) fn is_csv_path(path: &Path) -> bool {
    has_extension(path, "csv")
}

pub(crate) fn is_tsv_path(path: &Path) -> bool {
    has_extension(path, "tsv")
}

pub(crate) fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

pub(crate) fn start_bridge_server(
    paths: AppPaths,
    initial_context: Option<GuiLaunchContext>,
) -> Result<BridgeServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("failed to bind LinSync GUI bridge: {err}"))?;
    let address = listener
        .local_addr()
        .map_err(|err| format!("failed to read LinSync GUI bridge address: {err}"))?;
    let token = bridge_token()
        .map_err(|err| format!("failed to create LinSync GUI bridge token: {err}"))?;
    let base_url = format!("http://{address}/{token}");
    let server_token = token.clone();
    let state = Arc::new(Mutex::new(GuiBridgeState::new(initial_context)));

    // Pre-load the plugin-enabled map from disk so the in-memory copy is
    // authoritative from the first request onward.
    if let Ok(s) = state.lock()
        && let Ok(mut pe) = s.plugin_enabled.lock()
    {
        *pe = linsync_core::load_plugin_enabled_map(&paths);
    }

    // Clear a stale active-profile pointer once at startup (e.g. a user profile
    // deleted while selected) so the per-request resolver doesn't warn on every
    // request.
    cleanup_stale_active_pointer(&paths);

    // Reclaim archive-edit staging dirs and portal backups orphaned by a
    // crash or kill mid-edit (edit tokens live only in process memory, so
    // nothing else ever references them again). Age-gated so a concurrent
    // LinSync instance's live edit is never swept.
    sweep_orphaned_archive_edits(&paths);

    thread::spawn(move || {
        // Cap concurrent handler threads. This is a local single-client bridge
        // (one QML XMLHttpRequest at a time in normal use), but a retrigger
        // storm or progress-poll flood could otherwise spawn unbounded OS
        // threads. Each excess connection is rejected with a 503 so the client
        // retries on the next tick rather than queuing indefinitely.
        const MAX_CONCURRENT_BRIDGE_CONNECTIONS: usize = 16;
        let active = Arc::new(AtomicUsize::new(0));
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    // Handle each connection on its own thread so a `/cancel`
                    // request can be served while a `/compare` is still running
                    // (the accept loop must not block on a single request).
                    if active.load(Ordering::Relaxed) >= MAX_CONCURRENT_BRIDGE_CONNECTIONS {
                        tracing::warn!(
                            concurrent = active.load(Ordering::Relaxed),
                            "LinSync GUI bridge connection limit reached, rejecting"
                        );
                        // Return a 503 so the client sees a retry signal rather than
                        // a silently dropped connection.
                        let response = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        if let Err(err) = stream.write_all(response) {
                            tracing::warn!(error = %err, "failed to write 503 to rejected bridge connection");
                        } else if let Err(err) = stream.flush() {
                            tracing::warn!(error = %err, "failed to flush 503 to rejected bridge connection");
                        }
                        drop(stream);
                        continue;
                    }
                    let paths = paths.clone();
                    let state = Arc::clone(&state);
                    let token = server_token.clone();
                    let active = Arc::clone(&active);
                    active.fetch_add(1, Ordering::Relaxed);
                    thread::spawn(move || {
                        // RAII guard ensures the permit is released even if the
                        // handler panics (default unwind strategy). Without this,
                        // a panic would leak a permit and after MAX_CONCURRENT
                        // panics wedge the bridge — the exact session-
                        // accumulating hang this cap exists to prevent.
                        let _guard = PermitGuard(Arc::clone(&active));
                        if let Err(err) = handle_bridge_connection(stream, &paths, &state, &token) {
                            tracing::warn!(error = %err, "LinSync GUI bridge request failed");
                        }
                    });
                }
                Err(err) => {
                    tracing::warn!(error = %err, "LinSync GUI bridge accept failed");
                    break;
                }
            }
        }
    });

    Ok(BridgeServer { base_url })
}

/// RAII guard that decrements the bridge connection counter on drop, including
/// panic unwind. Prevents a panicking handler from permanently leaking a
/// connection permit and wedging the bridge.
struct PermitGuard(Arc<AtomicUsize>);

impl Drop for PermitGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

pub(crate) const MAX_BRIDGE_REQUEST_BYTES: u64 = 256 * 1024; // 256 KB — bumped for raw-text paste via query params
pub(crate) const MAX_BRIDGE_HEADERS: usize = 64;

pub(crate) fn handle_bridge_connection(
    mut stream: TcpStream,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
    token: &str,
) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut reader = BufReader::new(stream.try_clone()?).take(MAX_BRIDGE_REQUEST_BYTES);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let mut origin: Option<String> = None;
    let mut headers_seen: usize = 0;
    let mut content_length: usize = 0;
    loop {
        if headers_seen > MAX_BRIDGE_HEADERS {
            return Ok(());
        }
        headers_seen += 1;
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("origin") {
                origin = Some(value.trim().to_owned());
            } else if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let body = if content_length > 0 && content_length <= MAX_BRIDGE_REQUEST_BYTES as usize {
        let mut buf = Vec::with_capacity(content_length);
        reader.take(content_length as u64).read_to_end(&mut buf)?;
        buf
    } else {
        Vec::new()
    };

    if let Some(value) = origin.as_deref()
        && !origin_is_loopback(value)
    {
        let response = bridge_error(403, "Forbidden", "cross-origin requests are not allowed");
        stream.write_all(&response)?;
        return stream.flush();
    }

    let response = bridge_response_with_token(&request_line, &body, paths, state, Some(token));
    stream.write_all(&response)?;
    stream.flush()
}

pub(crate) fn origin_is_loopback(origin: &str) -> bool {
    let scheme_end = match origin.find("://") {
        Some(index) => index + 3,
        None => return false,
    };
    let host = &origin[scheme_end..];
    let host = host.split_once('/').map(|(host, _)| host).unwrap_or(host);
    let host = if let Some(rest) = host.strip_prefix('[') {
        let Some((address, after_bracket)) = rest.split_once(']') else {
            return false;
        };
        if !after_bracket.is_empty() && !after_bracket.starts_with(':') {
            return false;
        }
        address
    } else if host == "::1" {
        host
    } else {
        host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host)
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
pub(crate) fn bridge_response(
    request_line: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    bridge_response_with_token(request_line, &[], paths, state, None)
}

pub(crate) fn bridge_response_with_token(
    request_line: &str,
    body: &[u8],
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
    required_token: Option<&str>,
) -> Vec<u8> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();

    if method == "OPTIONS" {
        return http_response(204, "No Content", "application/json", b"{}".to_vec());
    }

    if method != "GET" && method != "POST" {
        return bridge_error(405, "Method Not Allowed", "unsupported method");
    }

    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = match strip_required_bridge_token(path, required_token) {
        Ok(path) => path,
        Err(response) => return response,
    };
    match path {
        "/health" => http_response(
            200,
            "OK",
            "application/json",
            format!(r#"{{"ok":true,"bridge_version":{BRIDGE_VERSION}}}"#).into_bytes(),
        ),
        "/session" => session_bridge_response(state),
        "/settings" => settings_bridge_response(paths),
        "/settings/set" => settings_set_bridge_response(query, paths),
        "/settings/reset" => settings_reset_bridge_response(paths),
        "/file/read" => file_read_bridge_response(query, state),
        "/file/write" => file_write_bridge_response(query, body, state),
        "/compare" => compare_bridge_response(query, paths, state),
        "/cancel" => cancel_bridge_response(query, state),
        "/progress" => progress_bridge_response(query, state),
        "/copy" => copy_bridge_response(query, state),
        "/copy-all" => copy_all_bridge_response(query, state),
        "/undo" => undo_bridge_response(state),
        "/redo" => redo_bridge_response(state),
        "/save" => save_bridge_response(query, state),
        "/tab/activate" => activate_tab_bridge_response(query, state),
        "/tab/close" => close_tab_bridge_response(query, state),
        "/bookmark/set" => bookmark_set_bridge_response(query, state),
        "/folder/open" => folder_open_bridge_response(query, paths),
        "/sessions/recent" => sessions_recent_bridge_response(paths),
        "/sessions/reopen" => sessions_reopen_bridge_response(query, paths, state),
        "/sessions/delete" => sessions_delete_bridge_response(query, paths),
        "/sessions/rename" => sessions_rename_bridge_response(query, paths),
        "/filters/list" => filters_list_bridge_response(paths),
        "/filters/save" => filters_save_bridge_response(query, paths),
        "/filters/delete" => filters_delete_bridge_response(query, paths),
        "/filters/validate" => filters_validate_bridge_response(query),
        "/filters/migrate" => filters_migrate_bridge_response(query),
        "/walk" => walk_options_bridge_response(paths),
        "/walk/set" => walk_options_set_bridge_response(query, paths),
        "/plugins/list" => {
            let pe = match state.lock() {
                Ok(s) => Arc::clone(&s.plugin_enabled),
                Err(_) => {
                    return bridge_error(500, "Internal Server Error", "session state unavailable");
                }
            };
            plugins_list_bridge_response(paths, &pe)
        }
        "/plugins/toggle" => {
            let pe = match state.lock() {
                Ok(s) => Arc::clone(&s.plugin_enabled),
                Err(_) => {
                    return bridge_error(500, "Internal Server Error", "session state unavailable");
                }
            };
            plugins_toggle_bridge_response(query, paths, &pe)
        }
        "/plugins/options/get" => plugins_options_get_bridge_response(query, paths),
        "/plugins/options/set" => plugins_options_set_bridge_response(query, paths),
        "/plugins/diagnostic" => plugins_diagnostic_bridge_response(query, paths),
        "/plugins/install" => plugins_install_bridge_response(query, paths),
        "/plugins/remove" => plugins_remove_bridge_response(query, paths),
        "/plugins/trust" => plugins_trust_bridge_response(query, paths),
        "/capabilities" => capabilities_bridge_response(),
        "/folder/query" => folder_query_bridge_response(query, paths, state),
        "/compare/text/window" => text_window_bridge_response(query, paths),
        "/compare/table/window" => table_window_bridge_response(query, state),
        "/binary/window" => binary_window_bridge_response(query, state),
        "/folder/op/plan" => folder_op_plan_bridge_response(query, paths, state),
        "/folder/op/execute" => folder_op_execute_bridge_response(query, paths, state),
        "/archive/can-edit" => archive_can_edit_bridge_response(query),
        "/archive/member/edit" => archive_member_edit_bridge_response(query, paths, state),
        "/archive/member/commit" => archive_member_commit_bridge_response(query, state),
        "/archive/member/discard" => archive_member_discard_bridge_response(query, state),
        "/merge/conflicts" => merge_conflicts_bridge_response(state),
        "/merge3/start" => merge3_start_bridge_response(query, paths, state),
        "/merge3/resolve" => merge3_resolve_bridge_response(query, state),
        "/merge3/save" => merge3_save_bridge_response(query, state),
        "/compare/document" => {
            let params = query_params(query);
            let profile = match resolve_profile_for_request(paths, &params) {
                Ok(p) => p,
                Err(err) => return bridge_error(400, "Bad Request", &err),
            };
            let req =
                register_cancellable_request(&params, state, "extracting", 3, "Extracting text");
            set_progress(
                &req.progress(),
                "extracting",
                1,
                3,
                "Running document extractor".to_owned(),
            );
            let (mut body, artifacts) =
                linsync::document_compare_bridge_response_with_profile_and_artifacts(
                    query,
                    &profile.document,
                    Some(req.cancellation_token()),
                );
            // If the user hit Stop during the (potentially slow) plugin
            // extraction, discard the result and report cancellation.
            if req.is_cancelled() {
                return http_response(
                    200,
                    "OK",
                    "application/json",
                    br#"{"cancelled":true}"#.to_vec(),
                );
            }
            set_progress(
                &req.progress(),
                "finalizing",
                2,
                3,
                "Building document tab".to_owned(),
            );
            if let (Some(left), Some(right), Ok(value)) = (
                query_value(&params, "left"),
                query_value(&params, "right"),
                serde_json::from_str::<serde_json::Value>(&body),
            ) {
                let tab = document_tab_from_response(left.to_owned(), right.to_owned(), &value);
                body = attach_session_to_response_body(
                    body,
                    tab,
                    query_bool(&params, "new_tab"),
                    paths,
                    state,
                );
                if !artifacts.is_empty()
                    && let Ok(mut state) = state.lock()
                {
                    let tab_id = state.session.active_tab_id;
                    if let Some(old) = state.rendered_page_cache_dirs.remove(&tab_id) {
                        std::thread::spawn(move || {
                            let _ = fs::remove_dir_all(old);
                        });
                    }
                    if let Some(dir) = artifacts.into_iter().next() {
                        state.rendered_page_cache_dirs.insert(tab_id, dir);
                    }
                }
            }
            set_progress(&req.progress(), "done", 3, 3, String::new());
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/profiles/list" => profiles_list_bridge_response(paths),
        "/profiles/active/get" => profiles_active_get_bridge_response(paths),
        "/profiles/active/set" => profiles_active_set_bridge_response(query, paths),
        "/profiles/active/prediffer" => profiles_active_prediffer_bridge_response(query, paths),
        "/profiles/active/plugin-enabled" => {
            profiles_active_plugin_enabled_bridge_response(query, paths)
        }
        "/raw-compare" => raw_compare_bridge_response(query, body, paths, state),
        "/compare/image" => {
            let params = query_params(query);
            let profile = match resolve_profile_for_request(paths, &params) {
                Ok(p) => p,
                Err(err) => return bridge_error(400, "Bad Request", &err),
            };
            let req =
                register_cancellable_request(&params, state, "comparing", 1, "Comparing images");
            let (mut body, result) = linsync::image_compare_bridge_response_with_profile_and_cancel(
                query,
                &profile.image,
                req.cancel_checker(),
            );
            // If the user hit Stop during the compare, discard the result.
            if req.is_cancelled() {
                return http_response(
                    200,
                    "OK",
                    "application/json",
                    br#"{"cancelled":true}"#.to_vec(),
                );
            }
            let result_for_tab = result.clone();
            let overlay_path = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("overlay_path")
                        .and_then(|uri| uri.as_str())
                        .and_then(file_uri_to_path)
                });
            if let Ok(mut s) = state.lock() {
                s.last_image_result = result;
                s.last_image_overlay_path = overlay_path;
            }
            if let (Some(result), Some(left), Some(right), Ok(value)) = (
                result_for_tab,
                query_value(&params, "left"),
                query_value(&params, "right"),
                serde_json::from_str::<serde_json::Value>(&body),
            ) {
                let tab = image_tab_from_result(left.to_owned(), right.to_owned(), &result, &value);
                body = attach_session_to_response_body(
                    body,
                    Some(tab),
                    query_bool(&params, "new_tab"),
                    paths,
                    state,
                );
            }
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/compare/image/regions" => image_regions_bridge_response(state),
        "/compare/image/save-overlay" => image_save_overlay_bridge_response(query, state),
        "/compare/image/formats" => http_response(
            200,
            "OK",
            "application/json",
            linsync::image_formats_bridge_response().into_bytes(),
        ),
        "/compare/webpage" => {
            let params = query_params(query);
            // The user-consent gate lives in QML (WebpageComparePage confirms the
            // network fetch dialog before calling runCompare()). Reject direct
            // bridge requests that did not pass through that dialog.
            if !query_bool(&params, "confirmed") {
                return bridge_error(
                    400,
                    "Bad Request",
                    "webpage compare requires confirmed=1 from the consent dialog",
                );
            }
            let profile = match resolve_profile_for_request(paths, &params) {
                Ok(p) => p,
                Err(err) => return bridge_error(400, "Bad Request", &err),
            };
            let req =
                register_cancellable_request(&params, state, "fetching", 3, "Fetching webpages");
            set_progress(
                &req.progress(),
                "fetching",
                1,
                3,
                "Fetching webpage content".to_owned(),
            );
            let mut body = linsync::webpage_compare_bridge_response_with_profile(
                query,
                paths,
                &profile.webpage,
            );
            // If the user hit Stop during the (potentially slow) fetch/render,
            // discard the result and report cancellation.
            if req.is_cancelled() {
                return http_response(
                    200,
                    "OK",
                    "application/json",
                    br#"{"cancelled":true}"#.to_vec(),
                );
            }
            set_progress(
                &req.progress(),
                "finalizing",
                2,
                3,
                "Building webpage tab".to_owned(),
            );
            if let (Some(left), Some(right), Ok(value)) = (
                query_value(&params, "left"),
                query_value(&params, "right"),
                serde_json::from_str::<serde_json::Value>(&body),
            ) {
                let mode = query_value(&params, "mode").unwrap_or("html");
                let tab =
                    webpage_tab_from_response(left.to_owned(), right.to_owned(), mode, &value);
                body = attach_session_to_response_body(
                    body,
                    tab,
                    query_bool(&params, "new_tab"),
                    paths,
                    state,
                );
            }
            set_progress(&req.progress(), "done", 3, 3, String::new());
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/compare/webpage/clear-cache" => {
            let body = linsync::webpage_clear_cache_bridge_response(paths);
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/binary/interpret" => binary_interpret_bridge_response(query, state),
        "/reveal" => reveal_bridge_response(query),
        "/open-external" => open_external_bridge_response(query),
        "/copy-clipboard" => copy_clipboard_bridge_response(query),
        "/report" => report_bridge_response(query, state, paths),
        "/project/save" => project_save_bridge_response(query, state, paths),
        "/project/open" => project_open_bridge_response(query, paths),
        "/project/recent" => project_recent_bridge_response(paths),
        "/sessions/save" => sessions_save_bridge_response(query, paths, state),
        "/artifacts/list" => artifacts_list_bridge_response(state),
        "/artifacts/cleanup" => artifacts_cleanup_bridge_response(query, paths),
        _ => bridge_error(404, "Not Found", "unknown bridge endpoint"),
    }
}

pub(crate) fn strip_required_bridge_token<'a>(
    path: &'a str,
    required_token: Option<&str>,
) -> Result<&'a str, Vec<u8>> {
    let Some(token) = required_token else {
        return Ok(path);
    };

    let expected_prefix = format!("/{token}");
    if path == expected_prefix {
        return Ok("/");
    }
    path.strip_prefix(&expected_prefix)
        .filter(|rest| rest.starts_with('/'))
        .ok_or_else(|| bridge_error(403, "Forbidden", "invalid bridge token"))
}

pub(crate) fn bridge_token() -> std::io::Result<String> {
    let mut bytes = [0_u8; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn session_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let context = match state.lock() {
        Ok(state) => state.context(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn settings_bridge_response(paths: &AppPaths) -> Vec<u8> {
    match load_gui_settings_json(paths) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

pub(crate) fn settings_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(key) = query_value(&params, "key") else {
        return bridge_error(400, "Bad Request", "missing setting key");
    };
    let Some(value) = query_value(&params, "value") else {
        return bridge_error(400, "Bad Request", "missing setting value");
    };

    match save_gui_setting_json(paths, key, value) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(400, "Bad Request", &err),
    }
}

pub(crate) fn settings_reset_bridge_response(paths: &AppPaths) -> Vec<u8> {
    match reset_gui_settings_json(paths) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

// ── Profile bridge endpoints ────────────────────────────────────────────────

pub(crate) fn profiles_list_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for p in builtin_profiles() {
        entries.push(serde_json::json!({
            "id": p.id.to_string(),
            "name": p.name,
            "description": p.description,
            "builtin": true,
        }));
    }
    let user_ids = match store.list_user_ids() {
        Ok(ids) => ids,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    for id in user_ids {
        match store.load(&id) {
            Ok(p) => entries.push(serde_json::json!({
                "id": p.id.to_string(),
                "name": p.name,
                "description": p.description,
                "builtin": false,
            })),
            Err(err) => entries.push(serde_json::json!({
                "id": id.to_string(),
                "name": id.to_string(),
                "description": String::new(),
                "builtin": false,
                "error": err.to_string(),
            })),
        }
    }
    let active = store
        .load_active_pointer()
        .ok()
        .flatten()
        .map(|id| id.to_string());
    let body = serde_json::json!({
        "active": active,
        "profiles": entries,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn profiles_active_get_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let active = match store.load_active_pointer() {
        Ok(maybe) => maybe.map(|id| id.to_string()),
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    let body = serde_json::json!({ "active": active }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn profiles_active_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(raw_id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id parameter");
    };
    let id = match ProfileId::new(raw_id.to_owned()) {
        Ok(id) => id,
        Err(err) => {
            return bridge_error(400, "Bad Request", &format!("invalid profile id: {err}"));
        }
    };
    // Reject ids that don't resolve to a built-in or stored user
    // profile. This prevents the GUI from quietly setting an active
    // pointer that subsequent compares would fall back away from.
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    if find_builtin(&id).is_none() && store.load(&id).is_err() {
        return bridge_error(
            404,
            "Not Found",
            &format!("profile '{id}' does not exist (built-in or user)"),
        );
    }
    if let Err(err) = store.save_active_pointer(&id) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({ "active": id.to_string() }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Add or remove a prediffer plugin from the active profile's prediffer chain
/// (`?id=PLUGIN_ID&enabled=true|false`). Only user profiles are editable;
/// built-in profiles (and "no active profile") are rejected with 409 so the
/// caller can prompt the user to create/select a user profile first.
pub(crate) fn profiles_active_prediffer_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(plugin_id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id parameter");
    };
    let enabled = query_value(&params, "enabled")
        .map(|v| v != "false")
        .unwrap_or(true);

    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let active_id = match store.load_active_pointer() {
        Ok(Some(id)) => id,
        Ok(None) => {
            return bridge_error(
                409,
                "Conflict",
                "no active profile selected; select a user profile to edit its prediffers",
            );
        }
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    if find_builtin(&active_id).is_some() {
        return bridge_error(
            409,
            "Conflict",
            "built-in profiles are read-only; copy to a user profile to edit prediffers",
        );
    }
    let mut profile = match store.load(&active_id) {
        Ok(p) => p,
        Err(err) => return bridge_error(404, "Not Found", &err.to_string()),
    };
    // Apply the add/remove, keeping the list de-duplicated and order-stable.
    profile
        .text
        .prediffer_plugins
        .retain(|existing| existing != plugin_id);
    if enabled {
        profile.text.prediffer_plugins.push(plugin_id.to_owned());
    }
    if let Err(err) = store.save(&profile) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({
        "ok": true,
        "profile": active_id.to_string(),
        "prediffers": profile.text.prediffer_plugins,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Set or clear a per-plugin enable/disable override on the active profile
/// (`?id=PLUGIN_ID&enabled=true|false`). Unlike the global `plugins.json`
/// toggle, this override is scoped to the active profile and wins over the
/// global state when that profile drives a comparison. Only user profiles are
/// editable; built-in profiles (and "no active profile") are rejected with 409.
pub(crate) fn profiles_active_plugin_enabled_bridge_response(
    query: &str,
    paths: &AppPaths,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(plugin_id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id parameter");
    };
    let Some(enabled_raw) = query_value(&params, "enabled") else {
        return bridge_error(400, "Bad Request", "missing enabled parameter");
    };
    let enabled = enabled_raw != "false";

    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let active_id = match store.load_active_pointer() {
        Ok(Some(id)) => id,
        Ok(None) => {
            return bridge_error(
                409,
                "Conflict",
                "no active profile selected; select a user profile to set per-profile plugin state",
            );
        }
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    if find_builtin(&active_id).is_some() {
        return bridge_error(
            409,
            "Conflict",
            "built-in profiles are read-only; copy to a user profile to set per-profile plugin state",
        );
    }
    let mut profile = match store.load(&active_id) {
        Ok(p) => p,
        Err(err) => return bridge_error(404, "Not Found", &err.to_string()),
    };
    // Record the override. We always store the explicit boolean (rather than
    // dropping back to "default") so the GUI can show a clear per-profile state;
    // the resolver treats a present entry as authoritative over the global map.
    //
    // This is an unsynchronized load-modify-write to the profile file, matching
    // the sibling /profiles/active/{set,prediffer} endpoints: the GUI drives one
    // edit at a time, so concurrent edits to the same profile are not a concern
    // here. If a multi-writer scenario ever arises, add file-level locking
    // across all profile-mutating endpoints rather than just this one.
    profile
        .plugin_enablement
        .insert(plugin_id.to_owned(), enabled);
    if let Err(err) = store.save(&profile) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({
        "ok": true,
        "profile": active_id.to_string(),
        "plugin_id": plugin_id,
        "enabled": enabled,
        "plugin_enablement": profile.plugin_enablement,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

// ── Profile resolution for bridge requests ──────────────────────────────────

/// Resolve the [`CompareProfile`] that should drive a single bridge
/// request:
///   1. If `?profile=<id>` is present:
///      - If the id resolves to a built-in or stored user profile, use it.
///      - Otherwise return `Err(...)` — the caller must surface a 400 so
///        the GUI cannot silently fall through to the wrong options. This
///        matches `/profiles/active/set`'s 404 semantics for unknown ids.
///   2. Otherwise read `active-profile.json`. If the id resolves to a
///      built-in or a stored user profile, use it.
///   3. As a last resort, return the `default` built-in.
pub(crate) fn resolve_profile_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<CompareProfile, String> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    if let Some(requested) = query_value(params, "profile") {
        let id = ProfileId::new(requested.to_owned())
            .map_err(|err| format!("invalid profile id '{requested}': {err}"))?;
        if let Some(p) = find_builtin(&id) {
            return Ok(p);
        }
        if let Ok(p) = store.load(&id) {
            return Ok(p);
        }
        return Err(format!("profile '{id}' does not exist (built-in or user)"));
    }
    if let Ok(Some(active_id)) = store.load_active_pointer() {
        if let Some(p) = find_builtin(&active_id) {
            return Ok(p);
        }
        if let Ok(p) = store.load(&active_id) {
            return Ok(p);
        }
        // Active pointer references a profile that no longer exists.
        // Fall through to the built-in default rather than fail; the
        // user may have removed a custom profile while it was still
        // selected. Logged so the GUI / CLI can surface a one-shot
        // notification later.
        eprintln!(
            "warning: active profile '{active_id}' no longer exists; using built-in 'default'"
        );
    }
    Ok(builtin_profiles()
        .into_iter()
        .next()
        .expect("at least one built-in profile is registered"))
}

/// Detect and clear a stale active-profile pointer once at startup.
///
/// If the active pointer references a profile that no longer exists (e.g. a
/// user profile deleted while it was selected), remove the pointer file so the
/// per-request resolver falls back to `default` cleanly — without emitting the
/// "active profile … no longer exists" warning on every request. Built-in ids
/// and live user profiles are left untouched. Returns `true` when a stale
/// pointer was cleared.
pub(crate) fn cleanup_stale_active_pointer(paths: &AppPaths) -> bool {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let Ok(Some(active_id)) = store.load_active_pointer() else {
        return false;
    };
    if find_builtin(&active_id).is_some() || store.load(&active_id).is_ok() {
        return false;
    }
    match store.clear_active_pointer() {
        Ok(()) => {
            eprintln!(
                "notice: cleared stale active profile pointer '{active_id}' (profile no longer exists); using built-in 'default'"
            );
            true
        }
        Err(err) => {
            eprintln!("warning: failed to clear stale active profile pointer '{active_id}': {err}");
            false
        }
    }
}

/// Minimum age before an unreferenced archive-edit staging dir or portal
/// backup is considered orphaned. Generous so an edit left open across a
/// long external-editor session (or owned by a concurrently running
/// instance) is never reclaimed out from under the user.
pub(crate) const ARCHIVE_EDIT_ORPHAN_MAX_AGE: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Remove archive-edit staging dirs (`cache_dir/archive-edits/<token>`) and
/// portal backups (`state_dir/archive-edit/<token>.bak`) older than
/// [`ARCHIVE_EDIT_ORPHAN_MAX_AGE`]. Edit tokens live only in process memory,
/// so entries from previous runs can never be committed or discarded again —
/// without this sweep a crash mid-edit leaks them forever.
pub(crate) fn sweep_orphaned_archive_edits(paths: &AppPaths) {
    let is_orphaned = |path: &Path| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .is_some_and(|age| age > ARCHIVE_EDIT_ORPHAN_MAX_AGE)
    };
    if let Ok(entries) = fs::read_dir(paths.cache_dir.join("archive-edits")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_orphaned(&path) {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
    if let Ok(entries) = fs::read_dir(paths.state_dir.join("archive-edit")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "bak") && is_orphaned(&path) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Build the `TextCompareOptions` for a single bridge request. Starts
/// from the resolved profile's text options, then applies per-request
/// query overrides (`ignore_case`, `ignore_whitespace`,
/// `ignore_blank_lines`, `ignore_eol`, `detect_moves`). Per the Phase 1
/// contract, an explicit `?ignore_case=true` always wins over the
/// profile's value; an absent flag leaves the profile value unchanged.
///
/// Returns `Err` when `?profile=` references an unknown id so the caller
/// can return 400 Bad Request rather than silently fall through.
pub(crate) fn resolve_text_options_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<TextCompareOptions, String> {
    let profile = resolve_profile_for_request(paths, params)?;
    let mut opts = profile.text;
    apply_text_query_overrides(&mut opts, params)?;
    Ok(opts)
}

pub(crate) fn apply_text_query_overrides(
    opts: &mut TextCompareOptions,
    params: &[(String, String)],
) -> Result<(), String> {
    if let Some(v) = query_value(params, "ignore_case")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_case = parsed;
    }
    if let Some(v) = query_value(params, "ignore_whitespace")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_whitespace = parsed;
    }
    if let Some(v) = query_value(params, "ignore_blank_lines")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_blank_lines = parsed;
    }
    if let Some(v) = query_value(params, "ignore_eol")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_eol = parsed;
    }
    if let Some(v) = query_value(params, "detect_moves")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.detect_moves = parsed;
    }
    if let Some(v) = query_value(params, "diff_algorithm") {
        opts.diff_algorithm = match v {
            "lcs" => linsync_core::DiffAlgorithm::Lcs,
            "patience" => linsync_core::DiffAlgorithm::Patience,
            "myers" => linsync_core::DiffAlgorithm::Myers,
            _ => return Err(format!("unknown diff_algorithm '{v}'")),
        };
    }
    if let Some(v) = query_value(params, "inline_granularity") {
        opts.inline_granularity = match v {
            "char" => linsync_core::InlineGranularity::Char,
            "word" => linsync_core::InlineGranularity::Word,
            "grapheme" => linsync_core::InlineGranularity::Grapheme,
            _ => return Err(format!("unknown inline_granularity '{v}'")),
        };
    }
    for value in params
        .iter()
        .filter(|(key, _)| key == "regex_rule_set")
        .map(|(_, value)| value)
    {
        opts.regex_rule_sets.push(value.clone());
    }
    if let Some(v) = query_value(params, "context_lines") {
        opts.context_lines = Some(
            v.parse::<usize>()
                .map_err(|_| format!("invalid context_lines '{v}'"))?,
        );
    }
    if let Some(v) = query_value(params, "show_only_changes")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.show_only_changes = parsed;
    }
    if let Some(v) = query_value(params, "render_mode") {
        opts.render_mode = parse_text_render_mode_query(v)?;
    }
    if let Some(v) = query_value(params, "syntax") {
        opts.syntax_mode = parse_text_syntax_mode_query(v)?;
    }
    if let Some(v) = query_value(params, "encoding") {
        opts.encoding = parse_text_encoding_query(v)?;
    }
    if let Some(pattern) = query_value(params, "find") {
        opts.find = Some(TextFindOptions {
            pattern: pattern.to_owned(),
            regex: query_bool(params, "find_regex"),
            case_sensitive: query_bool(params, "find_case_sensitive"),
        });
    }
    for value in params
        .iter()
        .filter(|(key, _)| key == "bookmark")
        .map(|(_, value)| value)
    {
        opts.bookmarks.push(parse_text_bookmark_query(value)?);
    }
    opts.validate_rule_sets()
        .map_err(|err| format!("invalid text options: {err}"))?;
    opts.validate_regex_options()
        .map_err(|err| format!("invalid text regex option: {err}"))?;
    Ok(())
}

pub(crate) fn parse_bool_query_param(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(crate) fn parse_text_render_mode_query(value: &str) -> Result<TextRenderMode, String> {
    match value {
        "side-by-side" | "side_by_side" | "side" => Ok(TextRenderMode::SideBySide),
        "unified" => Ok(TextRenderMode::Unified),
        "context" => Ok(TextRenderMode::Context),
        "normal" => Ok(TextRenderMode::Normal),
        "html" => Ok(TextRenderMode::Html),
        _ => Err(format!("unknown render_mode '{value}'")),
    }
}

pub(crate) fn parse_text_syntax_mode_query(value: &str) -> Result<TextSyntaxMode, String> {
    // Token set lives in core (`TextSyntaxMode: FromStr`), shared with the
    // CLI's `--syntax` — same precedent as `FolderGrouping` / `group_by=`.
    value.parse()
}

pub(crate) fn parse_text_encoding_query(value: &str) -> Result<TextInputEncoding, String> {
    match value {
        "auto" => Ok(TextInputEncoding::Auto),
        "utf8" | "utf-8" => Ok(TextInputEncoding::Utf8),
        "utf8-bom" | "utf-8-bom" => Ok(TextInputEncoding::Utf8Bom),
        "utf16le" | "utf-16le" | "utf-16-le" => Ok(TextInputEncoding::Utf16Le),
        "utf16be" | "utf-16be" | "utf-16-be" => Ok(TextInputEncoding::Utf16Be),
        "lossy-utf8" | "lossy-utf-8" => Ok(TextInputEncoding::LossyUtf8),
        _ => Err(format!("unknown encoding '{value}'")),
    }
}

pub(crate) fn parse_text_bookmark_query(value: &str) -> Result<TextBookmark, String> {
    let mut parts = value.splitn(3, ':');
    let side = match parts.next().unwrap_or_default() {
        "left" | "l" => CompareSide::Left,
        "right" | "r" => CompareSide::Right,
        other => {
            return Err(format!(
                "bookmark side '{other}' must be left or right; expected SIDE:LINE[:LABEL]"
            ));
        }
    };
    let Some(line_raw) = parts.next() else {
        return Err("bookmark requires SIDE:LINE[:LABEL]".to_owned());
    };
    let line = line_raw
        .parse::<usize>()
        .map_err(|_| "bookmark line must be a positive integer".to_owned())?;
    if line == 0 {
        return Err("bookmark line must be a positive integer".to_owned());
    }
    let label = parts.next().unwrap_or_default().to_owned();
    Ok(TextBookmark { side, line, label })
}

/// Parse a `?compare_method=` query token, or `None` if unrecognized. The
/// caller decides whether unknown is an error (folder query) or a no-op that
/// keeps the profile value (compare request).
fn parse_compare_method_query(value: &str) -> Option<linsync_core::CompareMethod> {
    Some(match value {
        "full-contents" => linsync_core::CompareMethod::FullContents,
        "quick-contents" => linsync_core::CompareMethod::QuickContents,
        "binary-contents" => linsync_core::CompareMethod::BinaryContents,
        "modified-date" => linsync_core::CompareMethod::ModifiedDate,
        "date-size" => linsync_core::CompareMethod::DateAndSize,
        "size" => linsync_core::CompareMethod::Size,
        "existence" => linsync_core::CompareMethod::Existence,
        "hash-blake3" => linsync_core::CompareMethod::HashBlake3,
        "normalized-text" => linsync_core::CompareMethod::NormalizedText,
        _ => return None,
    })
}

/// Parse a `?symlink_policy=` query token, or `None` if unrecognized.
fn parse_symlink_policy_query(value: &str) -> Option<linsync_core::SymlinkPolicy> {
    Some(match value {
        "compare-target" => linsync_core::SymlinkPolicy::CompareTarget,
        "follow" => linsync_core::SymlinkPolicy::Follow,
        "special-file" => linsync_core::SymlinkPolicy::SpecialFile,
        _ => return None,
    })
}

/// Resolve `FolderCompareOptions` for a single bridge request: start
/// from the active profile's folder options, then apply per-request
/// query overrides (`?recursive`, `?compare_method`, `?symlink_policy`,
/// `?include_skipped`).
/// Returns `Err` when `?profile=` references an unknown id.
pub(crate) fn resolve_folder_options_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<FolderCompareOptions, String> {
    let profile = resolve_profile_for_request(paths, params)?;
    let mut opts = profile.folder;
    if let Some(v) = query_value(params, "recursive")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.recursive = parsed;
    }
    if let Some(v) = query_value(params, "compare_method") {
        opts.compare_method =
            parse_compare_method_query(v).ok_or_else(|| format!("unknown compare_method '{v}'"))?;
    }
    if let Some(v) = query_value(params, "symlink_policy") {
        opts.symlink_policy =
            parse_symlink_policy_query(v).ok_or_else(|| format!("unknown symlink_policy '{v}'"))?;
    }
    if let Some(v) = query_value(params, "include_skipped")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.include_skipped = parsed;
    }
    Ok(opts)
}

pub(crate) fn resolve_compare_options_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<GuiCompareOptions, String> {
    let profile = resolve_profile_for_request(paths, params)?;

    let mut text = profile.text;
    apply_text_query_overrides(&mut text, params)?;

    let mut folder = profile.folder;
    if let Some(v) = query_value(params, "recursive")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        folder.recursive = parsed;
    }
    if let Some(v) = query_value(params, "compare_method") {
        folder.compare_method = parse_compare_method_query(v).unwrap_or(folder.compare_method);
    }
    if let Some(v) = query_value(params, "symlink_policy") {
        folder.symlink_policy = parse_symlink_policy_query(v).unwrap_or(folder.symlink_policy);
    }
    if let Some(v) = query_value(params, "include_skipped")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        folder.include_skipped = parsed;
    }

    let mut document = profile.document;
    apply_document_query_overrides(&mut document, params)?;

    let mut table = profile.table;
    if let Some(v) = query_value(params, "delimiter") {
        table.delimiter = match v {
            "tab" | "\\t" => '\t',
            s => s.chars().next().unwrap_or(table.delimiter),
        };
    }
    if let Some(v) = query_value(params, "has_header")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        table.has_header = parsed;
    }
    if let Some(v) = query_value(params, "key_columns") {
        table.key_columns = v
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
    }
    if let Some(v) = query_value(params, "ignore_columns") {
        table.ignore_columns = v
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
    }
    if let Some(v) = query_value(params, "numeric_tolerance")
        && let Ok(n) = v.parse::<f64>()
    {
        table.numeric_tolerance = Some(n);
    }
    if let Some(v) = query_value(params, "ignore_row_order")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        table.ignore_row_order = parsed;
    }

    let mut binary = profile.binary;
    if let Some(v) = query_value(params, "bytes_per_row")
        && let Ok(n) = v.parse::<usize>()
        && n > 0
    {
        binary.bytes_per_row = n;
    }
    if let Some(v) = query_value(params, "compare_content")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        binary.compare_content = parsed;
    }
    if let Some(v) = query_value(params, "compare_metadata")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        binary.compare_metadata = parsed;
    }

    Ok(GuiCompareOptions {
        text,
        folder,
        table,
        binary,
        image: profile.image,
        document,
    })
}

pub(crate) fn apply_document_query_overrides(
    opts: &mut DocumentCompareOptions,
    params: &[(String, String)],
) -> Result<(), String> {
    if let Some(v) = query_value(params, "mode") {
        opts.mode = match v {
            "Document" | "document" => opts.mode,
            "text" => DocumentCompareMode::Text,
            "ocr_text" | "ocr-text" => DocumentCompareMode::OcrText,
            "rendered" => DocumentCompareMode::Rendered,
            _ => opts.mode,
        };
    }
    if let Some(v) = query_value(params, "ocr_language") {
        opts.ocr_language = v.to_owned();
    }
    if let Some(v) = query_value(params, "document_timeout") {
        opts.timeout_secs = v
            .parse::<u64>()
            .map_err(|_| format!("invalid document_timeout '{v}'"))?;
    }
    Ok(())
}

pub(crate) fn compare_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(left) = query_value(&params, "left") else {
        return bridge_error(400, "Bad Request", "missing left path");
    };
    let Some(right) = query_value(&params, "right") else {
        return bridge_error(400, "Bad Request", "missing right path");
    };

    let options = match resolve_compare_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let new_tab = query_bool(&params, "new_tab");

    // Archive-as-folder: with no explicit mode (or an explicit "Archive"), if the
    // two inputs are an archive pair with an enabled unpacker, compare them as a
    // folder of their unpacked contents (nested archives recurse one level).
    // Any other explicit mode (Hex, Text, …) overrides this auto-routing.
    let requested_mode = query_value(&params, "mode")
        .map(str::trim)
        .filter(|m| !m.is_empty());
    if matches!(requested_mode, None | Some("Archive"))
        && let Some(plugin) = archive_pair_unpacker(Path::new(left), Path::new(right), paths)
    {
        let tab = archive_tab(left.to_owned(), right.to_owned(), &plugin, &options);
        let context = match state.lock() {
            Ok(mut state) => state.apply_compare(tab, new_tab),
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "session state unavailable");
            }
        };
        record_recent_context(paths, &context);
        return match context_to_json(&context) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        };
    }

    // Built-in archive-as-folder: if both inputs are supported archive formats
    // and no plugin took precedence, extract and compare as folders.
    if matches!(requested_mode, None | Some("Archive"))
        && linsync_core::is_builtin_archive_format(Path::new(left))
        && linsync_core::is_builtin_archive_format(Path::new(right))
    {
        let (tab, dirs) = builtin_archive_tab(
            Path::new(left),
            Path::new(right),
            left.to_owned(),
            right.to_owned(),
            &options,
            paths,
        );
        let Some(tab) = tab else {
            return bridge_error(
                500,
                "Internal Server Error",
                "archive compare produced no tab",
            );
        };
        let context = match state.lock() {
            Ok(mut state) => {
                let context = state.apply_compare(tab, new_tab);
                let tab_id = context.session.active_tab_id;
                if let Some(old) = state.archive_extract_dirs.remove(&tab_id) {
                    let _ = fs::remove_dir_all(old);
                }
                if let Some(dir) = dirs.into_iter().next() {
                    state.archive_extract_dirs.insert(tab_id, dir);
                }
                context
            }
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "session state unavailable");
            }
        };
        record_recent_context(paths, &context);
        return match context_to_json(&context) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        };
    }

    // Optional cancellation: when the QML supplies `?request_id=X`, register a
    // cancel flag so a concurrent `/cancel?id=X` can abort this compare. The
    // flag is registered/removed under the state lock, but the long compare
    // below runs WITHOUT holding the lock, so `/cancel` is never blocked by it.
    let req = register_cancellable_request(&params, state, "starting", 0, "Starting compare");

    let maybe_tab = build_tab_for_paths_with_mode_cancellable_and_artifacts(
        Path::new(left),
        Path::new(right),
        query_value(&params, "mode"),
        &options,
        req.cancel_checker(),
        req.progress(),
    );

    let Some((tab, artifact_dirs)) = maybe_tab else {
        // The compare was cancelled — leave the session state untouched.
        return http_response(
            200,
            "OK",
            "application/json",
            br#"{"cancelled":true}"#.to_vec(),
        );
    };

    let context = match state.lock() {
        Ok(mut state) => {
            let context = state.apply_compare(tab, new_tab);
            let tab_id = context.session.active_tab_id;
            if let Some(old) = state.rendered_page_cache_dirs.remove(&tab_id) {
                std::thread::spawn(move || {
                    let _ = fs::remove_dir_all(old);
                });
            }
            if let Some(dir) = artifact_dirs.into_iter().next() {
                state.rendered_page_cache_dirs.insert(tab_id, dir);
            }
            context
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    record_recent_context(paths, &context);
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Handle `/cancel?id=X` — flip the cancel flag for the in-flight `/compare`
/// request that registered `request_id == X`. Returns `{"cancelled":true}` if a
/// matching request was found, `{"cancelled":false}` otherwise (already
/// finished or unknown id). Always 200 so the QML treats it as best-effort.
pub(crate) fn cancel_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id");
    };
    let cancelled = match state.lock() {
        Ok(state) => state
            .compare_cancels
            .get(id)
            .map(|flag| {
                flag.store(true, Ordering::Relaxed);
                true
            })
            .unwrap_or(false),
        Err(_) => false,
    };
    http_response(
        200,
        "OK",
        "application/json",
        format!(r#"{{"cancelled":{cancelled}}}"#).into_bytes(),
    )
}

pub(crate) fn progress_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id");
    };
    let progress_json = match state.lock() {
        Ok(state) => state
            .compare_progress
            .get(id)
            .map(|p| {
                let prog = p.lock().ok();
                match &prog {
                    Some(prog) => serde_json::json!({
                        "phase": prog.phase,
                        "current": prog.current,
                        "total": prog.total,
                        "message": prog.message,
                    }),
                    None => {
                        serde_json::json!({"phase":"unknown","current":0,"total":0,"message":""})
                    }
                }
            })
            .unwrap_or_else(
                || serde_json::json!({"phase":"none","current":0,"total":0,"message":""}),
            ),
        Err(_) => serde_json::json!({"phase":"error","current":0,"total":0,"message":""}),
    };
    http_response(
        200,
        "OK",
        "application/json",
        serde_json::to_string(&progress_json)
            .unwrap_or_else(|_| r#"{"phase":"error"}"#.to_owned())
            .into_bytes(),
    )
}
#[derive(Default, serde::Deserialize)]
struct RawCompareBody {
    left_text: Option<String>,
    right_text: Option<String>,
    left_name: Option<String>,
    right_name: Option<String>,
}

struct RawCompareRequest {
    left_text: String,
    right_text: String,
    left_name: String,
    right_name: String,
    new_tab: bool,
    text_options: TextCompareOptions,
}

fn raw_compare_request(
    query: &str,
    body: &[u8],
    paths: &AppPaths,
) -> Result<RawCompareRequest, Vec<u8>> {
    let params = query_params(query);
    let body_payload = if body.is_empty() {
        RawCompareBody::default()
    } else {
        serde_json::from_slice::<RawCompareBody>(body)
            .map_err(|err| bridge_error(400, "Bad Request", &format!("invalid JSON body: {err}")))?
    };

    let left_text = body_payload
        .left_text
        .or_else(|| query_value(&params, "left_text").map(str::to_owned))
        .ok_or_else(|| bridge_error(400, "Bad Request", "missing left_text"))?;
    let right_text = body_payload
        .right_text
        .or_else(|| query_value(&params, "right_text").map(str::to_owned))
        .ok_or_else(|| bridge_error(400, "Bad Request", "missing right_text"))?;
    let left_name = body_payload
        .left_name
        .or_else(|| query_value(&params, "left_name").map(str::to_owned))
        .unwrap_or_else(|| "Left".to_owned());
    let right_name = body_payload
        .right_name
        .or_else(|| query_value(&params, "right_name").map(str::to_owned))
        .unwrap_or_else(|| "Right".to_owned());
    let text_options = resolve_text_options_for_request(paths, &params)
        .map_err(|err| bridge_error(400, "Bad Request", &err))?;

    Ok(RawCompareRequest {
        left_text,
        right_text,
        left_name,
        right_name,
        new_tab: query_bool(&params, "new_tab"),
        text_options,
    })
}

/// Handle `/raw-compare` (POST JSON body or query string).
///
/// Compares raw text strings directly without requiring files on disk.
/// Accepts `left_text`, `right_text`, `left_name`, `right_name` either as a
/// JSON body or as query parameters, plus the usual text option overrides.
pub(crate) fn raw_compare_bridge_response(
    query: &str,
    body: &[u8],
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let req = match raw_compare_request(query, body, paths) {
        Ok(req) => req,
        Err(resp) => return resp,
    };

    // Use linsync-core's compare_text which accepts raw &str
    let result = compare_text(
        &req.left_name,
        &req.left_text,
        &req.right_name,
        &req.right_text,
        &req.text_options,
    );
    let (left_rows, right_rows) = text_rows_for_gui_with_options(&result, &req.text_options);

    let tab = GuiCompareTab {
        id: 1,
        title: "Text: raw text compare".to_owned(),
        mode: "Text".to_owned(),
        left_path: format!("📄 {}", req.left_name),
        right_path: format!("📄 {}", req.right_name),
        base_path: None,
        status: "Text compare complete".to_owned(),
        difference_count: result.summary.differences,
        left_dirty: false,
        right_dirty: false,
        can_undo: false,
        can_redo: false,
        validation: GuiOpenValidation {
            compatible: true,
            path_kind: "RawText".to_owned(),
            message: "Compared pasted text".to_owned(),
        },
        summary: vec![
            summary_item("Diff blocks", result.summary.diff_blocks),
            summary_item("Changed lines", result.summary.changed_lines),
            summary_item("Left-only lines", result.summary.left_only_lines),
            summary_item("Right-only lines", result.summary.right_only_lines),
        ],
        left_rows,
        right_rows,
        total_rows: None,
        diff_row_indexes: Vec::new(),
        search_row_indexes: Vec::new(),
        folder_entries: vec![],
        folder_total: None,
        encoding_metadata: Some(result.encoding_summary()),
        table_headers: None,
        table_cells: None,
        artifacts: Vec::new(),
        rendered_pages: None,
        options: Some(GuiCompareOptions {
            text: req.text_options.clone(),
            ..GuiCompareOptions::default()
        }),
    };

    let context = match state.lock() {
        Ok(mut state) => state.apply_compare(tab, req.new_tab),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };

    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn copy_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(row) = query_value(&params, "row").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing row");
    };
    let Some(direction) = query_value(&params, "direction") else {
        return bridge_error(400, "Bad Request", "missing direction");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.copy_row(row, direction) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn copy_all_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(direction) = query_value(&params, "direction") else {
        return bridge_error(400, "Bad Request", "missing direction");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.copy_all(direction) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn undo_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let context = match state.lock() {
        Ok(mut state) => match state.undo() {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn redo_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let context = match state.lock() {
        Ok(mut state) => match state.redo() {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn save_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(side) = query_value(&params, "side") else {
        return bridge_error(400, "Bad Request", "missing side");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.save_side(side) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Validate that `path` is one of the active tab's compare paths (left, right,
/// or base). Prevents arbitrary file access via the inline editor endpoints.
pub(crate) fn path_is_active_tab_path(path: &str, state: &Arc<Mutex<GuiBridgeState>>) -> bool {
    let Ok(s) = state.lock() else {
        return false;
    };
    let Some(tab) = s
        .session
        .tabs
        .iter()
        .find(|t| t.id == s.session.active_tab_id)
    else {
        return false;
    };
    tab.left_path == path || tab.right_path == path || tab.base_path.as_deref() == Some(path)
}

/// Read raw text content from a file path. Used by the GUI inline editor.
pub(crate) fn file_read_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    if !path_is_active_tab_path(path, state) {
        return bridge_error(403, "Forbidden", "path is not an active compare path");
    }
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let body = serde_json::json!({ "ok": true, "content": content }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &format!("read failed: {err}")),
    }
}

/// Write raw text content to a file path. Used by the GUI inline editor.
/// Accepts the file content via POST body (preferred) or the `content=` query
/// parameter (legacy fallback for older QML clients).
pub(crate) fn file_write_bridge_response(
    query: &str,
    body: &[u8],
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    if !path_is_active_tab_path(path, state) {
        return bridge_error(403, "Forbidden", "path is not an active compare path");
    }
    let content: Vec<u8> = if !body.is_empty() {
        body.to_vec()
    } else if let Some(content) = query_value(&params, "content") {
        content.as_bytes().to_vec()
    } else {
        return bridge_error(400, "Bad Request", "missing content");
    };
    match std::fs::write(path, &content) {
        Ok(()) => http_response(200, "OK", "application/json", br#"{"ok":true}"#.to_vec()),
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("write failed: {err}"),
        ),
    }
}

pub(crate) fn activate_tab_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id").and_then(|value| value.parse::<u64>().ok()) else {
        return bridge_error(400, "Bad Request", "missing tab id");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.activate_tab(id) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn close_tab_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id").and_then(|value| value.parse::<u64>().ok()) else {
        return bridge_error(400, "Bad Request", "missing tab id");
    };

    let context = match state.lock() {
        Ok(mut state) => state.close_tab(id),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn bookmark_set_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(row) = query_value(&params, "row").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing bookmark row");
    };
    let bookmarked = query_value(&params, "bookmarked")
        .and_then(parse_bool_query_param)
        .unwrap_or(true);

    let context = match state.lock() {
        Ok(mut state) => match state.set_bookmark(row, bookmarked) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Build a core `FolderQuery` from `/folder/query` request parameters
/// (`search`, `types`, `offset`, `limit`).
pub(crate) fn folder_query_from_params(params: &[(String, String)]) -> linsync_core::FolderQuery {
    let mut query = linsync_core::FolderQuery::default();
    if let Some(search) = query_value(params, "search")
        && !search.is_empty()
    {
        query.search = Some(search.to_owned());
    }
    if let Some(types) = query_value(params, "types") {
        let mut filter = linsync_core::FolderTypeFilter {
            files: false,
            directories: false,
            symlinks: false,
            special: false,
        };
        for token in types.split(',') {
            match token.trim() {
                "file" | "files" => filter.files = true,
                "dir" | "directory" | "directories" => filter.directories = true,
                "symlink" | "symlinks" | "link" => filter.symlinks = true,
                "special" => filter.special = true,
                _ => {}
            }
        }
        if filter.files || filter.directories || filter.symlinks || filter.special {
            query.types = filter;
        }
    }
    if let Some(state) = query_value(params, "state") {
        use linsync_core::FolderEntryFilter;
        query.state = match state {
            "changed" | "differences" | "diff" => FolderEntryFilter::Differences,
            "left_only" => FolderEntryFilter::LeftOnly,
            "right_only" => FolderEntryFilter::RightOnly,
            "identical" | "equal" => FolderEntryFilter::Identical,
            "different" => FolderEntryFilter::Different,
            "errors" => FolderEntryFilter::Errors,
            // "all" / "" / anything else keeps the default (everything).
            _ => FolderEntryFilter::All,
        };
    }
    if let Some(sort) = query_value(params, "sort") {
        use linsync_core::FolderSortKey;
        query.sort = match sort {
            "name" => FolderSortKey::Name,
            "state" => FolderSortKey::State,
            "type" => FolderSortKey::Type,
            // The GUI's left/right size columns both map to the core's
            // "larger of the two sides" size key.
            "size" | "leftSize" | "rightSize" => FolderSortKey::Size,
            "modified" => FolderSortKey::Modified,
            // "path" / anything else keeps the default (relative path).
            _ => FolderSortKey::Path,
        };
    }
    if let Some(descending) = query_value(params, "descending").and_then(parse_bool_query_param) {
        query.descending = descending;
    }
    if let Some(group_by) = query_value(params, "group_by") {
        // Shared parser with the CLI (core's FromStr); the bridge stays
        // lenient and treats unknown values as "no grouping".
        query.group_by = group_by.parse().unwrap_or_default();
    }
    if let Some(offset) = query_value(params, "offset").and_then(|v| v.parse::<usize>().ok()) {
        query.offset = offset;
    }
    if let Some(limit) = query_value(params, "limit").and_then(|v| v.parse::<usize>().ok()) {
        query.limit = Some(limit);
    }
    query
}

/// Get a cached `FolderCompareResult` for the given paths + options, or compute
/// and cache it. The cache lives on `GuiBridgeState` and is invalidated on every
/// new `/compare`. This lets `/folder/query` and folder-op plan/execute reuse the
/// result instead of re-walking both directory trees on every request.
fn get_or_cache_folder_compare(
    left: &str,
    right: &str,
    options: &FolderCompareOptions,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<Arc<linsync_core::FolderCompareResult>, String> {
    let left_path = Path::new(left);
    let right_path = Path::new(right);

    if let Ok(s) = state.lock()
        && let Some(ref cache) = s.folder_compare_cache
        && cache.left == left_path
        && cache.right == right_path
        && &cache.options == options
    {
        return Ok(Arc::clone(&cache.result));
    }

    let result = compare_folders(left_path, right_path, options).map_err(|e| e.to_string())?;
    let arc = Arc::new(result);
    if let Ok(mut s) = state.lock() {
        s.folder_compare_cache = Some(FolderCompareCache {
            left: left_path.to_path_buf(),
            right: right_path.to_path_buf(),
            options: options.clone(),
            result: Arc::clone(&arc),
        });
    }
    Ok(arc)
}

/// `/folder/query?left=&right=&search=&types=&offset=&limit=&state=&sort=&descending=&group_by=` — compare two
/// folders and return the entries filtered/paged through the core `FolderQuery`,
/// so the GUI folder table can search + type-filter + paginate via the core API.
/// Reuses the cached `FolderCompareResult` when paths + options match the active
/// folder tab, avoiding a full re-walk on every page/sort/filter request.
pub(crate) fn folder_query_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let (Some(left), Some(right)) = (query_value(&params, "left"), query_value(&params, "right"))
    else {
        return bridge_error(400, "Bad Request", "missing left or right path");
    };
    let folder_options = match resolve_folder_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let result = match get_or_cache_folder_compare(left, right, &folder_options, state) {
        Ok(r) => r,
        Err(err) => return bridge_error(500, "Internal Server Error", &err),
    };
    let folder_query = folder_query_from_params(&params);
    let page = result.query(&folder_query);
    let filtered: Vec<FolderEntryDiff> = page
        .groups
        .iter()
        .flat_map(|group| group.entries.iter().map(|entry| (*entry).clone()))
        .collect();
    let entries = folder_entries_for_gui(&filtered);
    let body = serde_json::json!({
        "entries": entries,
        "totalMatched": page.total_matched,
        "offset": page.offset,
        "returned": page.returned,
        "hasMore": page.has_more,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Report compile-time capabilities so the QML can hide modes the binary can't
/// serve (e.g. webpage rendered/screenshot, which need the `web-engine` build).
///
/// `web_renderer` adds the runtime dimension: which backend rendered/screenshot
/// would actually use on this host — `"qml"` (Qt WebEngine), `"chromium"`
/// (headless Chromium fallback), or `"none"` (web-engine build but no usable
/// renderer binary, or a non-web-engine build).
pub(crate) fn capabilities_bridge_response() -> Vec<u8> {
    #[cfg(feature = "web-engine")]
    let web_renderer = linsync_core::active_renderer_kind();
    #[cfg(not(feature = "web-engine"))]
    let web_renderer = "none";
    let body = serde_json::json!({
        "web_engine": cfg!(feature = "web-engine"),
        "web_renderer": web_renderer,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Return a windowed slice of a text comparison
/// (`?left=&right=&offset=&limit=` + the same option params `/compare` accepts),
/// so the GUI can extend a large diff window-by-window instead of loading every
/// row into the view. The rows are built exactly as `/compare` builds them —
/// the same `left_rows`/`right_rows` split, honoring render mode / ignore flags
/// / syntax / find — so a fetched window appends seamlessly onto the first
/// window the compare response embedded. `totalRows`/`hasMore` drive paging.
pub(crate) fn text_window_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let (Some(left), Some(right)) = (query_value(&params, "left"), query_value(&params, "right"))
    else {
        return bridge_error(400, "Bad Request", "missing left or right path");
    };
    let offset = query_value(&params, "offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    // 0 (or absent) means "to the end".
    let limit = query_value(&params, "limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);

    let options = match resolve_text_options_for_request(paths, &params) {
        Ok(options) => options,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let result = match linsync_core::compare_text_files(Path::new(left), Path::new(right), &options)
    {
        Ok(result) => result,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    let (total_rows, left_window, right_window) =
        if options.render_mode == linsync_core::TextRenderMode::SideBySide {
            // Windowed core build: expensive per-row work (syntax highlighting,
            // find-match marking) runs only for the requested rows instead of
            // the whole file on every scroll fetch.
            let page = result.view_rows_window(&options, offset, limit);
            let (left, right) = gui_rows_from_view_rows(page.rows);
            (page.total_rows, left, right)
        } else {
            // Rendered (unified/context/normal) modes have no windowed core
            // equivalent; they do no per-row syntax work, so full-build-then-
            // slice stays cheap.
            let (left_rows, right_rows) = text_rows_for_gui_with_options(&result, &options);
            let total_rows = left_rows.len().max(right_rows.len());
            let end = offset.saturating_add(limit).min(total_rows);
            let window = |rows: Vec<GuiLineRow>| -> Vec<GuiLineRow> {
                if offset >= rows.len() {
                    Vec::new()
                } else {
                    rows[offset..end.min(rows.len())].to_vec()
                }
            };
            (total_rows, window(left_rows), window(right_rows))
        };
    let returned = left_window.len().max(right_window.len());
    let body = serde_json::json!({
        "totalRows": total_rows,
        "offset": offset,
        "returned": returned,
        "hasMore": offset + returned < total_rows,
        "left_rows": left_window,
        "right_rows": right_window,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Serve a window of table rows from the active table tab. The canonical tab
/// holds the full `table_cells`; this endpoint slices it without re-running the
/// compare so large tables page efficiently.
pub(crate) fn table_window_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let offset = query_value(&params, "offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query_value(&params, "limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);

    let (total, rows) = match state.lock() {
        Ok(s) => {
            let Some(tab) = s
                .session
                .tabs
                .iter()
                .find(|t| t.id == s.session.active_tab_id)
            else {
                return bridge_error(404, "Not Found", "no active tab");
            };
            if tab.mode != "Table" {
                return bridge_error(400, "Bad Request", "active tab is not a table compare");
            }
            let Some(all_rows) = tab.table_cells.as_ref() else {
                return serde_json::json!({
                    "rows": [],
                    "offset": offset,
                    "limit": limit,
                    "total": 0,
                    "hasMore": false,
                })
                .to_string()
                .into_bytes();
            };
            let total = all_rows.len();
            let end = offset.saturating_add(limit).min(total);
            let window = all_rows
                .get(offset..end)
                .map(<[linsync_core::TableRowDiff]>::to_vec)
                .unwrap_or_default();
            (total, window)
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let body = serde_json::json!({
        "rows": rows,
        "offset": offset,
        "limit": limit,
        "total": total,
        "hasMore": offset + rows.len() < total,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn folder_open_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let key = query_value(&params, "key").unwrap_or("config");
    let target = match key {
        "config" => paths.config_dir.clone(),
        "data" => paths.data_dir.clone(),
        "cache" => paths.cache_dir.clone(),
        "state" => paths.state_dir.clone(),
        "filters" => paths.filters_file(),
        "settings" => paths.settings_file(),
        other => {
            return bridge_error(400, "Bad Request", &format!("unknown folder key '{other}'"));
        }
    };

    if !target.exists()
        && let Some(parent) = target.parent()
        && parent != target
    {
        let _ = fs::create_dir_all(&target);
    }

    match open_with_xdg(&target) {
        Ok(_) => {
            let body =
                serde_json::json!({ "ok": true, "path": target.display().to_string() }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

pub(crate) fn open_with_xdg(target: &Path) -> Result<(), String> {
    let opener = env::var_os("LINSYNC_OPENER")
        .map(PathBuf::from)
        .or_else(|| find_command_in_path("xdg-open"));
    let opener = opener.ok_or_else(|| "could not find xdg-open; set LINSYNC_OPENER".to_owned())?;
    let mut command = Command::new(opener);
    command.arg(target);
    let status = command
        .status()
        .map_err(|err| format!("failed to launch opener: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("opener exited with status {status}"))
    }
}

pub(crate) fn reveal_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let path = PathBuf::from(percent_decode(path_str));
    if !path.exists() {
        return bridge_error(
            404,
            "Not Found",
            &format!("path does not exist: {}", path.display()),
        );
    }
    let revealer = env::var_os("LINSYNC_REVEAL").map(PathBuf::from);
    let result = if let Some(ref cmd) = revealer {
        Command::new(cmd).arg(&path).status()
    } else {
        let fm1 = find_command_in_path("filemanager");
        if let Some(fm) = fm1 {
            Command::new(fm).arg(&path).status()
        } else {
            let parent = if path.is_dir() {
                path.clone()
            } else {
                path.parent().map(|p| p.to_owned()).unwrap_or(path.clone())
            };
            Command::new("xdg-open").arg(&parent).status()
        }
    };
    match result {
        Ok(status) if status.success() => {
            let body = serde_json::json!({"ok":true}).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Ok(status) => bridge_error(
            500,
            "Internal Server Error",
            &format!("revealer exited with status {status}"),
        ),
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to launch revealer: {err}"),
        ),
    }
}

pub(crate) fn open_external_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let path = PathBuf::from(percent_decode(path_str));
    if !path.exists() {
        return bridge_error(
            404,
            "Not Found",
            &format!("path does not exist: {}", path.display()),
        );
    }
    match open_with_xdg(&path) {
        Ok(_) => {
            let body = serde_json::json!({"ok":true}).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

pub(crate) fn copy_clipboard_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(text) = query_value(&params, "text") else {
        return bridge_error(400, "Bad Request", "missing text");
    };
    let text = percent_decode(text);
    let clipboard_cmd = if env::var_os("WAYLAND_DISPLAY").is_some() {
        find_command_in_path("wl-copy")
    } else {
        find_command_in_path("xclip").filter(|_| env::var_os("DISPLAY").is_some())
    };
    match clipboard_cmd {
        Some(cmd) => match Command::new(&cmd)
            .args(if cmd.file_name().map(|f| f == "xclip").unwrap_or(false) {
                vec!["-selection", "clipboard"]
            } else {
                vec![]
            })
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                match child.wait() {
                    Ok(status) if status.success() => {
                        let body = serde_json::json!({"ok":true}).to_string();
                        http_response(200, "OK", "application/json", body.into_bytes())
                    }
                    Ok(status) => bridge_error(
                        500,
                        "Internal Server Error",
                        &format!("clipboard command exited with {status}"),
                    ),
                    Err(err) => bridge_error(
                        500,
                        "Internal Server Error",
                        &format!("clipboard command wait failed: {err}"),
                    ),
                }
            }
            Err(err) => bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to launch clipboard command: {err}"),
            ),
        },
        None => bridge_error(
            500,
            "Internal Server Error",
            "no clipboard command found (need xclip or wl-copy)",
        ),
    }
}

pub(crate) fn archive_can_edit_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let path = PathBuf::from(percent_decode(path_str));
    let editable = linsync_core::ArchiveFormat::detect(&path).is_some();
    let body = serde_json::json!({ "editable": editable }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn archive_member_edit_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(archive_str) = query_value(&params, "archive") else {
        return bridge_error(400, "Bad Request", "missing archive");
    };
    let Some(member_str) = query_value(&params, "member") else {
        return bridge_error(400, "Bad Request", "missing member");
    };
    let archive = PathBuf::from(percent_decode(archive_str));
    let member = percent_decode(member_str);

    if linsync_core::ArchiveFormat::detect(&archive).is_none() {
        return bridge_error(
            400,
            "UnsupportedArchive",
            "unsupported archive format for member editing",
        );
    }

    if !archive.exists() {
        return bridge_error(
            404,
            "Not Found",
            &format!("archive does not exist: {}", archive.display()),
        );
    }

    let canonical = match archive.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return bridge_error(500, "Internal Server Error", &e.to_string());
        }
    };

    // Reject a second concurrent edit for the same archive.
    {
        let state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return bridge_error(500, "Internal Server Error", "state lock poisoned"),
        };
        if state_guard
            .archive_edit_tokens
            .values()
            .any(|ctx| ctx.archive() == canonical)
        {
            return bridge_error(
                409,
                "Conflict",
                "an edit is already in progress for this archive",
            );
        }
    }

    // Generate the token first so the staging dir is unpredictable and unique.
    let token = match bridge_token() {
        Ok(t) => t,
        Err(e) => {
            return bridge_error(500, "Internal Server Error", &e.to_string());
        }
    };
    let staging_root = paths.cache_dir.join("archive-edits").join(&token);
    let portal_bak = paths
        .state_dir
        .join("archive-edit")
        .join(&token)
        .with_extension("bak");

    let ctx = match linsync_core::extract_member_for_edit(
        &archive,
        &member,
        &staging_root,
        Some(&portal_bak),
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging_root);
            // Extraction may have written the portal backup (possibly
            // partially) before failing — reclaim it too.
            let _ = fs::remove_file(&portal_bak);
            return bridge_error(archive_write_error_status(&e), "Error", &e.to_string());
        }
    };

    // Re-check under the lock after extraction to close the race window.
    {
        let mut state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                let _ = fs::remove_dir_all(&staging_root);
                let _ = fs::remove_file(&portal_bak);
                return bridge_error(500, "Internal Server Error", "state lock poisoned");
            }
        };
        if state_guard
            .archive_edit_tokens
            .values()
            .any(|ctx| ctx.archive() == canonical)
        {
            let _ = fs::remove_dir_all(&staging_root);
            let _ = fs::remove_file(&portal_bak);
            return bridge_error(
                409,
                "Conflict",
                "an edit is already in progress for this archive",
            );
        }
        let staged_path = ctx.staged_path().to_path_buf();
        let atomic = ctx.atomic();
        state_guard.archive_edit_tokens.insert(token.clone(), ctx);
        let body = serde_json::json!({
            "ok": true,
            "token": token,
            "staged_path": staged_path,
            "atomic": atomic,
        })
        .to_string();
        http_response(200, "OK", "application/json", body.into_bytes())
    }
}

/// Single source for the `ArchiveWriteError` → HTTP status contract
/// (documented on the error type in `linsync-core::archive_write`). Both the
/// edit and commit endpoints must serve the same status for the same failure.
pub(crate) fn archive_write_error_status(e: &linsync_core::ArchiveWriteError) -> u16 {
    match e {
        linsync_core::ArchiveWriteError::InvalidMemberName { .. }
        | linsync_core::ArchiveWriteError::MemberNameEncoding { .. }
        | linsync_core::ArchiveWriteError::NonRegularMember { .. }
        | linsync_core::ArchiveWriteError::NonRegularStagedFile { .. }
        | linsync_core::ArchiveWriteError::CapsExceeded { .. }
        | linsync_core::ArchiveWriteError::UnsupportedArchive { .. } => 400,
        linsync_core::ArchiveWriteError::ArchiveNotFound { .. }
        | linsync_core::ArchiveWriteError::MemberNotFound { .. } => 404,
        linsync_core::ArchiveWriteError::StaleArchive { .. }
        | linsync_core::ArchiveWriteError::LockContention { .. } => 409,
        _ => 500,
    }
}

pub(crate) fn archive_member_commit_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(token) = query_value(&params, "token") else {
        return bridge_error(400, "Bad Request", "missing token");
    };
    let keep_backup = query_bool(&params, "keep_backup");

    let ctx = {
        let mut state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return bridge_error(500, "Internal Server Error", "state lock poisoned"),
        };
        match state_guard.archive_edit_tokens.remove(token) {
            Some(ctx) => ctx,
            None => return bridge_error(400, "Bad Request", "invalid or expired token"),
        }
    };

    let options = linsync_core::CommitOptions { keep_backup };
    match linsync_core::commit_member_edit(&ctx, &options) {
        Ok(outcome) => {
            let _ = fs::remove_dir_all(ctx.staging_root());
            let mut response = serde_json::json!({"ok": true});
            if let Some(bak) = &outcome.bak_path {
                response["bak_path"] = serde_json::json!(bak);
            }
            if let Some(warn) = &outcome.bak_cleanup_warning {
                response["bak_cleanup_warning"] = serde_json::json!(warn);
            }
            http_response(
                200,
                "OK",
                "application/json",
                response.to_string().into_bytes(),
            )
        }
        Err(e) => {
            // Staging holds the user's only copy of their edit — never delete
            // it on failure. Re-register the token so the edit stays owned:
            // the user can retry (meaningful for RenameFailed) or discard,
            // which cleans staging and the portal backup. The only case the
            // token is not re-registered is the rare race where another edit
            // for the same archive started during the unlocked commit; the
            // staged file is then orphaned but its path is reported below
            // (and the startup sweep eventually reclaims it).
            let is_retryable = matches!(e, linsync_core::ArchiveWriteError::RenameFailed { .. });
            let mut token_retained = false;
            if let Ok(mut state_guard) = state.lock() {
                let canonical = ctx.archive();
                if !state_guard
                    .archive_edit_tokens
                    .values()
                    .any(|c| c.archive() == canonical)
                {
                    state_guard
                        .archive_edit_tokens
                        .insert(token.to_owned(), ctx.clone());
                    token_retained = true;
                }
            }
            let mut error_body = serde_json::json!({
                "error": e.to_string(),
                "retryable": is_retryable,
                "staged_path": ctx.staged_path(),
                "token_retained": token_retained,
            });
            if let linsync_core::ArchiveWriteError::PortalReadOnly {
                backup: Some(backup),
                ..
            } = &e
            {
                error_body["backup_path"] = serde_json::json!(backup);
            }
            bridge_error_json(archive_write_error_status(&e), "Error", error_body)
        }
    }
}

pub(crate) fn archive_member_discard_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(token) = query_value(&params, "token") else {
        return bridge_error(400, "Bad Request", "missing token");
    };

    let (staging_root, portal_backup) = {
        let mut state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return bridge_error(500, "Internal Server Error", "state lock poisoned"),
        };
        match state_guard.archive_edit_tokens.remove(token) {
            Some(ctx) => (
                ctx.staging_root().to_path_buf(),
                ctx.portal_backup().map(|p| p.to_path_buf()),
            ),
            None => return bridge_error(400, "Bad Request", "invalid or expired token"),
        }
    };

    let _ = fs::remove_dir_all(&staging_root);
    if let Some(bak) = portal_backup {
        let _ = fs::remove_file(&bak);
    }
    let body = serde_json::json!({"ok": true}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn sessions_recent_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent: RecentSessions = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to load recent sessions: {err}"),
            );
        }
    };
    // Hide any leftover internal test-fixture sessions from the Sessions page list
    // (and from being re-opened). Prevents dev/smoke pollution from showing up.
    prune_internal_fixture_sessions(&mut recent);
    let entries: Vec<serde_json::Value> = recent
        .sessions
        .iter()
        .enumerate()
        .map(|(index, file)| {
            serde_json::json!({
                "index": index,
                "title": file.session.title,
                "left": file.session.left.display().to_string(),
                "right": file.session.right.display().to_string(),
                "mode": compare_view_mode_label(file.selected_view),
                "lastResult": file.last_result.as_ref().map(|r| serde_json::json!({
                    "equal": r.equal,
                    "differenceCount": r.difference_count,
                })),
            })
        })
        .collect();
    let body = serde_json::json!({ "sessions": entries }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn sessions_reopen_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(index) = query_value(&params, "index").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing index");
    };
    let recent_store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent = match recent_store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    prune_internal_fixture_sessions(&mut recent);
    let Some(session_file) = recent.sessions.get(index) else {
        return bridge_error(404, "Not Found", "recent session index out of range");
    };

    // The recent-sessions reopen flow has no per-request profile
    // selection. Resolve from the active profile and tolerate a
    // missing/invalid pointer by falling back to defaults; the session's own
    // saved text options still win (build_tab_for_session_file overlays them).
    let base = resolve_compare_options_for_request(paths, &[])
        .unwrap_or_else(|_| GuiCompareOptions::default());
    let multi_tab = restore_multi_tab_context(session_file);
    let single_tab = if multi_tab.is_none() {
        Some(build_tab_for_session_file(session_file, &base))
    } else {
        None
    };
    let context = match state.lock() {
        Ok(mut state) => match multi_tab {
            // A multi-tab workspace snapshot: re-add every saved tab to the
            // live session, then activate the tab that was active when the
            // workspace was recorded (ids are reassigned on insert).
            Some(snapshot) => {
                let snapshot_active_id = snapshot.session.active_tab_id;
                let mut mapped_active_id = None;
                for tab in snapshot.session.tabs {
                    let old_id = tab.id;
                    let inserted = state.apply_compare(tab, true);
                    if old_id == snapshot_active_id {
                        mapped_active_id = Some(inserted.session.active_tab_id);
                    }
                }
                match mapped_active_id {
                    Some(id) => state.activate_tab(id).unwrap_or_else(|_| state.context()),
                    None => state.context(),
                }
            }
            None => {
                state.apply_compare(single_tab.expect("single tab built when no snapshot"), true)
            }
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    record_recent_context(paths, &context);
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

/// `/sessions/delete?index=X` — remove a recent session by its index.
pub(crate) fn sessions_delete_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(index) = query_value(&params, "index").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing index");
    };
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    // Prune fixture entries first: the index the Sessions page sends counts
    // within the pruned list (/sessions/recent), not the raw on-disk one.
    prune_internal_fixture_sessions(&mut recent);
    if index >= recent.sessions.len() {
        return bridge_error(404, "Not Found", "session index out of range");
    }
    recent.sessions.remove(index);
    if let Err(err) = store.save(&recent) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({"ok": true, "removed": index}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// `/sessions/rename?index=X&title=Y` — rename a recent session.
pub(crate) fn sessions_rename_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(index) = query_value(&params, "index").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing index");
    };
    let Some(title) = query_value(&params, "title") else {
        return bridge_error(400, "Bad Request", "missing title");
    };
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    // Prune fixture entries first: the index the Sessions page sends counts
    // within the pruned list (/sessions/recent), not the raw on-disk one.
    prune_internal_fixture_sessions(&mut recent);
    let Some(session) = recent.sessions.get_mut(index) else {
        return bridge_error(404, "Not Found", "session index out of range");
    };
    session.session.title = title.to_owned();
    if let Err(err) = store.save(&recent) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({"ok": true, "index": index, "title": title}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Save the currently open tabs as a named project file at `?path=` (with an
/// optional `?name=`). Each persistable tab becomes a `SessionFile` entry.
pub(crate) fn project_save_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
    paths: &AppPaths,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing project path");
    };
    let name = query_value(&params, "name").unwrap_or("Untitled project");

    let (sessions, active_index) = match state.lock() {
        Ok(s) => {
            let sessions: Vec<linsync_core::SessionFile> = s
                .session
                .tabs
                .iter()
                .filter(|tab| tab_has_persistable_paths(tab))
                .map(session_file_from_tab)
                .collect();
            let active_index = s
                .session
                .tabs
                .iter()
                .filter(|tab| tab_has_persistable_paths(tab))
                .position(|tab| tab.id == s.session.active_tab_id);
            (sessions, active_index)
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    if sessions.is_empty() {
        return bridge_error(404, "Not Found", "no comparable tabs to save");
    }

    let mut project = linsync_core::ProjectFile::new(name);
    project.active_session_index = active_index;
    project.sessions = sessions;

    // Store paths relative to the project file's directory when they live under
    // it, so the project travels with its folder; load() resolves them back.
    if let Some(base) = Path::new(path).parent() {
        for session in &mut project.sessions {
            linsync_core::relativize_session_paths_against(&mut session.session, base);
        }
    }

    match linsync_core::ProjectFileStore::new(PathBuf::from(path)).save(&project) {
        Ok(()) => {
            record_recent_project(paths, path);
            let body = serde_json::json!({
                "ok": true,
                "name": name,
                "sessions": project.sessions.len(),
                "path": path,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Record a project file path in the recent-workspaces list (best-effort).
pub(crate) fn record_recent_project(paths: &AppPaths, path: &str) {
    let store = RecentPathStore::new(paths.recent_projects_file(), recent_limit(paths));
    if let Err(err) = store.add(PathBuf::from(path)) {
        tracing::warn!(path, error = %err, "failed to record recent project");
    }
}

/// List recent project files (most-recent first), skipping any that no longer
/// exist on disk.
pub(crate) fn project_recent_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = RecentPathStore::new(paths.recent_projects_file(), recent_limit(paths));
    let recent = store.load_or_default().unwrap_or_default();
    let projects: Vec<serde_json::Value> = recent
        .paths
        .iter()
        .filter(|p| p.exists())
        .map(|p| {
            serde_json::json!({
                "path": p.display().to_string(),
                "name": p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            })
        })
        .collect();
    let body = serde_json::json!({ "projects": projects }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Open a project file at `?path=` and return it as a launch-context JSON the
/// QML can apply (`applySessionContextJson`): one tab per saved session.
pub(crate) fn project_open_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing project path");
    };
    let project = match linsync_core::ProjectFileStore::new(PathBuf::from(path)).load() {
        Ok(p) => p,
        Err(err) => return bridge_error(400, "Bad Request", &err.to_string()),
    };
    if project.sessions.is_empty() {
        return bridge_error(404, "Not Found", "project has no comparisons");
    }
    record_recent_project(paths, path);

    let base = resolve_compare_options_for_request(paths, &[])
        .unwrap_or_else(|_| GuiCompareOptions::default());
    let mut tabs: Vec<GuiCompareTab> = Vec::with_capacity(project.sessions.len());
    for (index, session) in project.sessions.iter().enumerate() {
        let mut tab = build_tab_for_session_file(session, &base);
        tab.id = (index as u64) + 1;
        tabs.push(tab);
    }
    let active_tab_id = (project.active_session_index.unwrap_or(0) as u64) + 1;
    let context = GuiLaunchContext::from_tabs(tabs, active_tab_id);

    match serde_json::to_value(&context) {
        Ok(mut value) => {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("ok".to_owned(), serde_json::json!(true));
                obj.insert("name".to_owned(), serde_json::json!(project.name));
            }
            http_response(
                200,
                "OK",
                "application/json",
                value.to_string().into_bytes(),
            )
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn report_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
    paths: &AppPaths,
) -> Vec<u8> {
    let params = query_params(query);
    let format = query_value(&params, "format").unwrap_or("json");
    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    match format {
        "summary" => {
            let mut blocks: Vec<serde_json::Value> = Vec::new();
            let mut i = 0;
            while i < tab.left_rows.len().max(tab.right_rows.len()) {
                let left = tab.left_rows.get(i);
                let right = tab.right_rows.get(i);
                let state_str = left
                    .map(|r| r.state.as_str())
                    .or(right.map(|r| r.state.as_str()))
                    .unwrap_or("equal");
                if state_str != "equal" {
                    blocks.push(serde_json::json!({
                        "kind": "difference",
                        "left_start": left.and_then(|r| r.number).unwrap_or(0),
                        "right_start": right.and_then(|r| r.number).unwrap_or(0),
                        "left_len": if left.is_some() { 1 } else { 0 },
                        "right_len": if right.is_some() { 1 } else { 0 },
                    }));
                }
                i += 1;
            }

            let mut summary = serde_json::json!({
                "schema_version": 1,
                "mode": tab.mode.to_lowercase(),
                "left_path": tab.left_path,
                "right_path": tab.right_path,
                "equal": tab.difference_count == 0,
                "differences": tab.difference_count,
                "blocks": blocks,
            });

            if tab.mode == "Folder" {
                let mut identical = 0usize;
                let mut different = 0usize;
                let mut left_only = 0usize;
                let mut right_only = 0usize;
                for entry in &tab.folder_entries {
                    match entry.state.as_str() {
                        "equal" => identical += 1,
                        "changed" => different += 1,
                        "left_only" => left_only += 1,
                        "right_only" => right_only += 1,
                        _ => {}
                    }
                }
                summary["folder_summary"] = serde_json::json!({
                    "identical": identical,
                    "different": different,
                    "left_only": left_only,
                    "right_only": right_only,
                });
            }

            http_response(
                200,
                "OK",
                "application/json",
                summary.to_string().into_bytes(),
            )
        }
        "folder-plan" => {
            if tab.mode != "Folder" {
                return bridge_error(
                    400,
                    "Bad Request",
                    "folder-plan format requires a folder compare tab",
                );
            }
            let mut entries: Vec<serde_json::Value> = Vec::new();
            let mut total = 0usize;
            let mut identical = 0usize;
            let mut different = 0usize;
            let mut left_only = 0usize;
            let mut right_only = 0usize;
            for entry in &tab.folder_entries {
                total += 1;
                match entry.state.as_str() {
                    "equal" => identical += 1,
                    "changed" => different += 1,
                    "left_only" => left_only += 1,
                    "right_only" => right_only += 1,
                    _ => {}
                }
                entries.push(serde_json::json!({
                    "path": entry.path,
                    "state": entry.state,
                    "left_size": entry.left_size,
                    "right_size": entry.right_size,
                }));
            }
            let body = serde_json::json!({
                "schema_version": 1,
                "entries": entries,
                "summary": {
                    "total": total,
                    "identical": identical,
                    "different": different,
                    "left_only": left_only,
                    "right_only": right_only,
                }
            });
            http_response(200, "OK", "application/json", body.to_string().into_bytes())
        }
        "full-json" => {
            let mut artifact_entries: Vec<serde_json::Value> = Vec::new();
            for a in &tab.artifacts {
                artifact_entries.push(serde_json::to_value(a).unwrap_or_default());
            }
            let tab_json = serde_json::to_value(&tab).unwrap_or_default();
            let body = serde_json::json!({
                "schema_version": 1,
                "mode": tab.mode.to_lowercase(),
                "tab": tab_json,
                "artifacts": artifact_entries,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "unified" => {
            let mut lines = Vec::new();
            lines.push(format!("--- {}", tab.left_path));
            lines.push(format!("+++ {}", tab.right_path));
            for i in 0..tab.left_rows.len().max(tab.right_rows.len()) {
                let left = tab.left_rows.get(i);
                let right = tab.right_rows.get(i);
                let state = left
                    .map(|r| r.state.as_str())
                    .or(right.map(|r| r.state.as_str()))
                    .unwrap_or("equal");
                match state {
                    "equal" => {
                        if let Some(r) = left {
                            lines.push(format!(" {}", r.text));
                        }
                    }
                    "left_only" => {
                        if let Some(r) = left {
                            lines.push(format!("-{}", r.text));
                        }
                    }
                    "right_only" => {
                        if let Some(r) = right {
                            lines.push(format!("+{}", r.text));
                        }
                    }
                    "changed" => {
                        if let Some(r) = left {
                            lines.push(format!("-{}", r.text));
                        }
                        if let Some(r) = right {
                            lines.push(format!("+{}", r.text));
                        }
                    }
                    _ => {
                        if let Some(r) = left.or(right) {
                            lines.push(format!(" {}", r.text));
                        }
                    }
                }
            }
            let report_text = lines.join("\n");
            let saved_path = save_artifact(paths, "report-unified", report_text.as_bytes()).ok();
            let mut artifact_entries: Vec<serde_json::Value> = Vec::new();
            if let Some(ref p) = saved_path {
                artifact_entries.push(serde_json::json!({
                    "type": "report_file",
                    "path": p.to_string_lossy(),
                    "format": "unified"
                }));
            }
            for a in &tab.artifacts {
                artifact_entries.push(serde_json::to_value(a).unwrap_or_default());
            }
            let mut body_map = serde_json::json!({
                "content": report_text,
                "artifacts": artifact_entries,
            });
            if let Some(p) = saved_path {
                body_map["artifact_path"] = serde_json::json!(p.to_string_lossy().as_ref());
            }
            http_response(
                200,
                "OK",
                "application/json",
                body_map.to_string().into_bytes(),
            )
        }
        _ => {
            let mut artifact_entries: Vec<serde_json::Value> = Vec::new();
            for a in &tab.artifacts {
                artifact_entries.push(serde_json::to_value(a).unwrap_or_default());
            }
            let body = serde_json::json!({
                "tab": {
                    "mode": tab.mode,
                    "left_path": tab.left_path,
                    "right_path": tab.right_path,
                    "status": tab.status,
                    "difference_count": tab.difference_count,
                    "summary": tab.summary,
                },
                "artifacts": artifact_entries,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

pub(crate) fn artifacts_list_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    let body = serde_json::json!({
        "artifacts": tab.artifacts,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn artifacts_cleanup_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let max_age_seconds: u64 = query_value(&params, "max_age_seconds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(86400);
    match cleanup_artifacts(paths, Duration::from_secs(max_age_seconds)) {
        Ok(removed) => {
            let body = serde_json::json!({
                "removed": removed,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn sessions_save_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let title = query_value(&params, "title").unwrap_or("Untitled Session");
    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    // Refuse rather than save an entry the /sessions/recent responder would
    // filter straight back out (internal test fixtures) or that has no usable
    // paths — a 200 followed by nothing appearing reads as data loss.
    if !tab_has_persistable_paths(&tab) {
        return bridge_error(
            400,
            "Bad Request",
            "active tab's paths cannot be saved as a session",
        );
    }
    let mut session_file = SessionFile::new(CompareSession {
        title: title.to_owned(),
        left: PathBuf::from(&tab.left_path),
        base: None,
        right: PathBuf::from(&tab.right_path),
        options: CompareOptions {
            text: tab_text_options(&tab),
        },
    });
    session_file.selected_view = compare_view_mode(&tab.mode);
    persist_tab_snapshot(&mut session_file, &tab);
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    match store.add(session_file) {
        Ok(_) => {
            let body = serde_json::json!({"ok":true}).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to save session: {err}"),
        ),
    }
}

pub(crate) fn filters_list_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = FilterStore::new(paths.filters_file());
    let filters: NamedFilters = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    match serde_json::to_string(&filters) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn filters_save_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(body) = query_value(&params, "body") else {
        return bridge_error(400, "Bad Request", "missing filter body");
    };
    let parsed = match FileFilter::parse(body) {
        Ok(filter) => filter,
        Err(err) => {
            return bridge_error(400, "Bad Request", &format!("filter parse failed: {err}"));
        }
    };
    if parsed.name.is_none() {
        return bridge_error(
            400,
            "Bad Request",
            "filter body must include a 'name:' header",
        );
    }
    let store = FilterStore::new(paths.filters_file());
    match store.upsert(parsed) {
        Ok(filters) => match serde_json::to_string(&filters) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        },
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn filters_delete_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(name) = query_value(&params, "name") else {
        return bridge_error(400, "Bad Request", "missing filter name");
    };
    let store = FilterStore::new(paths.filters_file());
    let mut filters = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    filters.filters.retain(|f| f.name.as_deref() != Some(name));
    match store.save(&filters) {
        Ok(_) => match serde_json::to_string(&filters) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        },
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

pub(crate) fn filters_validate_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(body) = query_value(&params, "body") else {
        return bridge_error(400, "Bad Request", "missing filter body");
    };
    match FileFilter::parse(body) {
        Ok(filter) => {
            let response = serde_json::json!({
                "ok": true,
                "name": filter.name,
                "rules": filter.rules.len(),
            });
            http_response(
                200,
                "OK",
                "application/json",
                response.to_string().into_bytes(),
            )
        }
        Err(err) => {
            let response = serde_json::json!({
                "ok": false,
                "line": err.line,
                "message": err.message,
                "kind": format!("{:?}", err.kind),
                "migration_hint": err.is_migration_hint(),
            });
            http_response(
                200,
                "OK",
                "application/json",
                response.to_string().into_bytes(),
            )
        }
    }
}

pub(crate) fn filters_migrate_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    // Accept either `body` (raw text content) or `path` (file path to read).
    let body_owned: Option<String> = query_value(&params, "body").map(str::to_owned);
    let path_owned: Option<String> = query_value(&params, "path").map(str::to_owned);
    let text = if let Some(body) = body_owned {
        body
    } else if let Some(path) = path_owned {
        match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                return bridge_error(
                    400,
                    "Bad Request",
                    &format!("failed to read file '{path}': {err}"),
                );
            }
        }
    } else {
        return bridge_error(400, "Bad Request", "missing 'body' or 'path' parameter");
    };
    let result = linsync_core::migrate_filter_text(&text);
    let response = serde_json::json!({
        "ok": true,
        "migrated": result.migrated,
        "warnings": result.warnings,
    });
    http_response(
        200,
        "OK",
        "application/json",
        response.to_string().into_bytes(),
    )
}

pub(crate) fn walk_options_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = SettingsStore::new(paths.settings_file());
    let settings = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    let body = serde_json::json!({
        "respect_gitignore": settings.respect_gitignore,
        "follow_symlinks": settings.follow_symlinks,
        "max_walk_depth": settings.max_walk_depth,
        "includes": settings.session_includes,
        "excludes": settings.session_excludes,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn walk_options_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(key) = query_value(&params, "key") else {
        return bridge_error(400, "Bad Request", "missing walk option key");
    };
    let value = query_value(&params, "value").unwrap_or("");
    let store = SettingsStore::new(paths.settings_file());
    let mut settings = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    match key {
        "respect_gitignore" => match parse_bool_setting(key, value) {
            Ok(b) => settings.respect_gitignore = b,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        "follow_symlinks" => match parse_bool_setting(key, value) {
            Ok(b) => settings.follow_symlinks = b,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        "max_walk_depth" => match value.parse::<u32>() {
            Ok(n) => settings.max_walk_depth = n.min(256),
            Err(_) => {
                return bridge_error(
                    400,
                    "Bad Request",
                    &format!("invalid max_walk_depth: {value}"),
                );
            }
        },
        "includes" => {
            settings.session_includes = split_csv_list(value);
        }
        "excludes" => {
            settings.session_excludes = split_csv_list(value);
        }
        other => {
            return bridge_error(
                400,
                "Bad Request",
                &format!("unknown walk option '{other}'"),
            );
        }
    }
    if let Err(err) = store.save(&settings) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    walk_options_bridge_response(paths)
}

pub(crate) fn split_csv_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn plugins_list_bridge_response(
    paths: &AppPaths,
    plugin_enabled: &Arc<Mutex<HashMap<String, bool>>>,
) -> Vec<u8> {
    // Read through the in-memory lock so list and toggle share the same view.
    let enabled_map = match plugin_enabled.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return bridge_error(500, "Internal Server Error", "plugin state unavailable"),
    };
    let discovery = discover_installed_plugins(paths);
    let user_plugins_dir = paths.user_plugins_dir();
    let trusted_map = linsync_core::load_plugin_trusted_map(paths);
    let plugins: Vec<serde_json::Value> = discovery
        .plugins
        .iter()
        .map(|p| plugin_to_json(p, &enabled_map, &trusted_map, &user_plugins_dir))
        .collect();
    let errors: Vec<serde_json::Value> = discovery
        .errors
        .iter()
        .map(|err| {
            serde_json::json!({
                "path": err.path.display().to_string(),
                "message": err.message,
            })
        })
        .collect();
    let roots: Vec<String> = linsync_core::plugin_discovery_roots(paths)
        .iter()
        .map(|root| root.display().to_string())
        .collect();
    // Surface the sandbox confinement that helpers run under, so the Plugins
    // page can show whether plugin execution is confined or degraded.
    let sandbox = linsync_core::active_sandbox_status();
    // The active profile's prediffer chain + whether it can be edited (user
    // profiles only), so the page can show a per-prediffer "in profile" toggle.
    let active_profile = active_profile_prediffer_info(paths);
    let body = serde_json::json!({
        "plugins": plugins,
        "errors": errors,
        "roots": roots,
        "sandbox": { "label": sandbox.label, "confined": sandbox.confined },
        "active_profile": active_profile,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Describe the active profile for the Plugins page: its id, whether it is an
/// editable (user) profile, and the prediffer plugin ids it currently routes.
pub(crate) fn active_profile_prediffer_info(paths: &AppPaths) -> serde_json::Value {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let Ok(Some(active_id)) = store.load_active_pointer() else {
        return serde_json::json!({ "id": null, "editable": false, "prediffers": [] });
    };
    let editable = find_builtin(&active_id).is_none();
    let (prediffers, plugin_enablement) = if editable {
        store
            .load(&active_id)
            .map(|p| (p.text.prediffer_plugins, p.plugin_enablement))
            .unwrap_or_default()
    } else {
        find_builtin(&active_id)
            .map(|p| {
                (
                    p.text.prediffer_plugins.clone(),
                    p.plugin_enablement.clone(),
                )
            })
            .unwrap_or_default()
    };
    serde_json::json!({
        "id": active_id.to_string(),
        "editable": editable,
        "prediffers": prediffers,
        "plugin_enablement": plugin_enablement,
    })
}

pub(crate) fn plugins_toggle_bridge_response(
    query: &str,
    paths: &AppPaths,
    plugin_enabled: &Arc<Mutex<HashMap<String, bool>>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    let enabled_str = query_value(&params, "enabled").unwrap_or("true");
    let enabled = matches!(enabled_str, "true" | "1" | "yes");
    // Acquire the lock for the full load-modify-save sequence so concurrent
    // toggles cannot interleave and produce a partial write.
    let text = {
        let mut guard = match plugin_enabled.lock() {
            Ok(g) => g,
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "plugin state unavailable");
            }
        };
        guard.insert(id.to_owned(), enabled);
        serde_json::to_string_pretty(&*guard)
    };
    let text = match text {
        Ok(t) => t,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    let file = paths.plugins_enabled_file();
    if let Some(parent) = file.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    match fs::write(&file, text) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// `/plugins/diagnostic?id=X` — probe a discovered plugin's helper and report a
/// structured health verdict (exit/timeout/stdout/stderr + parsed response) plus
/// the active sandbox confinement. Backs the Plugins page "Diagnose" action.
pub(crate) fn plugins_diagnostic_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }
    let discovery = discover_installed_plugins(paths);
    let Some(plugin) = discovery.plugins.iter().find(|p| p.manifest.id == id) else {
        return bridge_error(404, "Not Found", "no installed plugin with that id");
    };
    let sandbox = linsync_core::active_sandbox_status();
    let sandbox_json = serde_json::json!({ "label": sandbox.label, "confined": sandbox.confined });
    match linsync_core::probe_plugin(
        &plugin.root,
        &plugin.manifest,
        Vec::new(),
        &linsync_core::PluginExecutionOptions::default(),
    ) {
        Ok(outcome) => {
            let response = outcome.response.as_ref().map(|r| {
                serde_json::json!({
                    "status": format!("{:?}", r.status).to_lowercase(),
                    "diagnostics": r
                        .diagnostics
                        .iter()
                        .map(|d| serde_json::json!({"severity": d.severity, "message": d.message}))
                        .collect::<Vec<_>>(),
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
                "sandbox": sandbox_json,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => {
            let body = serde_json::json!({
                "id": id,
                "healthy": false,
                "error": err.to_string(),
                "sandbox": sandbox_json,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

/// Record a plugin's trusted state (`?id=&trusted=true|false`). The GUI calls
/// this before enabling a discovered plugin for the first time, so that an
/// enabled plugin is always one the user has authorized to run.
pub(crate) fn plugins_trust_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }
    // Default to trusting; an explicit `?trusted=false` revokes.
    let trusted = query_value(&params, "trusted")
        .map(|v| v != "false")
        .unwrap_or(true);
    match linsync_core::set_plugin_trusted(paths, id, trusted) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true, "id": id, "trusted": trusted }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Install a plugin from a local directory (`?path=`) into the user plugin
/// directory. 409 if an id is already installed, 400 on a bad manifest/path.
pub(crate) fn plugins_install_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    use linsync_core::PluginStoreError;
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing plugin source path");
    };
    match linsync_core::install_plugin(paths, std::path::Path::new(path)) {
        Ok(installed) => {
            let body = serde_json::json!({
                "ok": true,
                "id": installed.manifest.id,
                "name": installed.manifest.name,
                "root": installed.root.to_string_lossy(),
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(PluginStoreError::AlreadyInstalled(id)) => bridge_error(
            409,
            "Conflict",
            &format!("a plugin with id '{id}' is already installed"),
        ),
        Err(e @ (PluginStoreError::InvalidManifest(_) | PluginStoreError::InvalidId(_))) => {
            bridge_error(400, "Bad Request", &e.to_string())
        }
        Err(e) => bridge_error(500, "Internal Server Error", &e.to_string()),
    }
}

/// Remove a user-installed plugin (`?id=`). 404 if not installed in the user
/// directory; system plugin directories are never touched.
pub(crate) fn plugins_remove_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    use linsync_core::PluginStoreError;
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    match linsync_core::remove_plugin(paths, id) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true, "id": id }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(PluginStoreError::UnknownPlugin(_)) => {
            bridge_error(404, "Not Found", "no installed plugin with that id")
        }
        Err(PluginStoreError::InvalidId(_)) => {
            bridge_error(400, "Bad Request", "invalid plugin id")
        }
        Err(e) => bridge_error(500, "Internal Server Error", &e.to_string()),
    }
}

pub(crate) fn plugins_options_get_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }

    // Look up the schema from the discovered manifest (empty if plugin not found).
    let discovery = discover_installed_plugins(paths);
    let schema: Vec<serde_json::Value> = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .map(|p| {
            p.manifest
                .options_schema
                .iter()
                .map(|opt| {
                    serde_json::json!({
                        "key": opt.key,
                        "label": opt.label,
                        "kind": format!("{:?}", opt.kind).to_lowercase(),
                        "default": opt.default,
                        "choices": opt.choices,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let values = linsync_core::load_plugin_options(paths, id);
    let body = serde_json::json!({
        "schema": schema,
        "values": values,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn plugins_options_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }
    let Some(key) = query_value(&params, "key") else {
        return bridge_error(400, "Bad Request", "missing option key");
    };
    let Some(raw) = query_value(&params, "value") else {
        return bridge_error(400, "Bad Request", "missing option value");
    };

    // Parse the value as JSON so `true`/`7`/`"x"` get the right type; fall back
    // to a plain string. The core store validates it against the plugin's
    // manifest schema before persisting.
    let value: serde_json::Value =
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()));
    match linsync_core::set_plugin_option(paths, id, key, value) {
        Ok(_) => {
            let body = serde_json::json!({ "ok": true }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => {
            let body = serde_json::json!({ "ok": false, "error": err.to_string() }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

pub(crate) fn plugin_to_json(
    plugin: &DiscoveredPlugin,
    enabled_map: &std::collections::HashMap<String, bool>,
    trusted_map: &std::collections::HashMap<String, bool>,
    user_plugins_dir: &Path,
) -> serde_json::Value {
    let manifest = &plugin.manifest;
    // The plugin root is the per-plugin sub-directory; its parent is the
    // containing plugins directory.  Compare that parent to the user plugins
    // directory to distinguish user-installed plugins from system ones.
    let source = plugin
        .root
        .parent()
        .map(|parent| {
            if parent == user_plugins_dir {
                "user"
            } else {
                "system"
            }
        })
        .unwrap_or("user");
    let enabled = *enabled_map.get(&manifest.id).unwrap_or(&true);
    serde_json::json!({
        "id": manifest.id,
        "name": manifest.name,
        "version": manifest.version,
        "license": manifest.license,
        "classes": manifest.classes.iter().map(|class| format!("{class:?}").to_lowercase()).collect::<Vec<_>>(),
        "extensions": manifest.extensions.clone(),
        "mime_types": manifest.mime_types.clone(),
        "deterministic": manifest.deterministic,
        "directory": plugin.root.display().to_string(),
        "source": source,
        "enabled": enabled,
        "trusted": trusted_map.get(&manifest.id).copied().unwrap_or(false),
        "has_options": !manifest.options_schema.is_empty(),
    })
}

pub(crate) fn folder_op_plan_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(kind) = query_value(&params, "kind") else {
        return bridge_error(400, "Bad Request", "missing op kind");
    };
    // Each selected entry arrives as its own `entries=` param (percent-encoded),
    // so paths containing commas survive intact.
    let entries: Vec<PathBuf> = query_values(&params, "entries")
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();

    let active = match state.lock() {
        Ok(state) => state.active_tab().cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(tab) = active else {
        return bridge_error(400, "Bad Request", "no active compare tab");
    };
    if tab.mode != "Folder" {
        return bridge_error(
            400,
            "Bad Request",
            "folder ops require a folder compare tab",
        );
    }

    let folder_options = match resolve_folder_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let compare = match get_or_cache_folder_compare(
        &tab.left_path,
        &tab.right_path,
        &folder_options,
        state,
    ) {
        Ok(r) => r,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("folder compare failed: {err}"),
            );
        }
    };

    let Some(op_kind) = parse_folder_op_kind(kind, &params) else {
        return bridge_error(400, "Bad Request", "unsupported op kind");
    };
    let delete_side = folder_op_delete_side(&op_kind);
    let mut plan = plan_folder_operation(&compare, op_kind, &entries);
    let left_base = Path::new(&tab.left_path);
    let right_base = Path::new(&tab.right_path);
    if let Err(err) = linsync_core::assess_operation_risks(&mut plan, left_base, right_base) {
        return bridge_error(
            500,
            "Internal Server Error",
            &format!("risk assessment failed: {err}"),
        );
    }
    let permanent_delete = plan.contains_deletes && !use_trash_for_deletes(paths);
    let mut body = folder_plan_to_json(&plan);
    body["permanent_delete"] = serde_json::Value::Bool(permanent_delete);
    if permanent_delete {
        body["permanent_warning"] = serde_json::Value::String(permanent_delete_warning(
            delete_side,
            plan.counts.delete_count,
        ));
    }
    http_response(200, "OK", "application/json", body.to_string().into_bytes())
}

/// True when the user's settings route deletes to the freedesktop trash;
/// false means folder-op deletes are permanent and require confirmation.
pub(crate) fn use_trash_for_deletes(paths: &AppPaths) -> bool {
    SettingsStore::new(paths.settings_file())
        .load_or_default()
        .map(|settings| settings.delete_preference == DeletePreference::MoveToTrash)
        .unwrap_or(true)
}

pub(crate) fn folder_op_delete_side(kind: &FolderOperationKind) -> Option<CompareSide> {
    match kind {
        FolderOperationKind::DeleteLeft => Some(CompareSide::Left),
        FolderOperationKind::DeleteRight => Some(CompareSide::Right),
        _ => None,
    }
}

pub(crate) fn folder_op_execute_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(kind) = query_value(&params, "kind") else {
        return bridge_error(400, "Bad Request", "missing op kind");
    };
    // Each selected entry arrives as its own `entries=` param (percent-encoded),
    // so paths containing commas survive intact.
    let entries: Vec<PathBuf> = query_values(&params, "entries")
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();

    let active = match state.lock() {
        Ok(state) => state.active_tab().cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(tab) = active else {
        return bridge_error(400, "Bad Request", "no active compare tab");
    };
    if tab.mode != "Folder" {
        return bridge_error(
            400,
            "Bad Request",
            "folder ops require a folder compare tab",
        );
    }

    let folder_options = match resolve_folder_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let compare = match get_or_cache_folder_compare(
        &tab.left_path,
        &tab.right_path,
        &folder_options,
        state,
    ) {
        Ok(r) => r,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("folder compare failed: {err}"),
            );
        }
    };

    let Some(op_kind) = parse_folder_op_kind(kind, &params) else {
        return bridge_error(400, "Bad Request", "unsupported op kind");
    };
    let delete_side = folder_op_delete_side(&op_kind);
    let plan = plan_folder_operation(&compare, op_kind, &entries);

    let use_trash = use_trash_for_deletes(paths);
    let confirm_permanent = query_bool(&params, "confirm_permanent");
    if plan.contains_deletes && !use_trash && !confirm_permanent {
        // Refuse before touching the filesystem: permanent deletes are
        // unrecoverable, so the caller must resend with confirm_permanent=1.
        return bridge_error(
            409,
            "Conflict",
            &permanent_delete_warning(delete_side, plan.counts.delete_count),
        );
    }
    let confirmation = if confirm_permanent {
        linsync_core::PermanentDeleteConfirmation::Confirmed
    } else {
        linsync_core::PermanentDeleteConfirmation::NotConfirmed
    };
    let outcomes = execute_folder_operation_plan(&plan, &paths.data_dir, use_trash, confirmation);
    // The filesystem changed — invalidate the cached folder compare so the
    // next /folder/query doesn't serve pre-operation results. (The GUI
    // normally calls requestCompare after execute, which also clears it,
    // but this is defense-in-depth against stale-cache reads.)
    if let Ok(mut state) = state.lock() {
        state.folder_compare_cache = None;
    }
    let body = folder_outcomes_to_json(&plan, &outcomes).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn parse_folder_op_kind(
    kind: &str,
    params: &[(String, String)],
) -> Option<FolderOperationKind> {
    let new_name = query_value(params, "new_name").map(|name| name.to_owned());
    Some(match kind {
        "copy_left_to_right" => FolderOperationKind::CopyLeftToRight,
        "copy_right_to_left" => FolderOperationKind::CopyRightToLeft,
        "delete_left" => FolderOperationKind::DeleteLeft,
        "delete_right" => FolderOperationKind::DeleteRight,
        "rename_left" => FolderOperationKind::RenameLeft {
            new_name: new_name?,
        },
        "rename_right" => FolderOperationKind::RenameRight {
            new_name: new_name?,
        },
        "create_missing_left" => FolderOperationKind::CreateMissingLeft,
        "create_missing_right" => FolderOperationKind::CreateMissingRight,
        "refresh" => FolderOperationKind::Refresh,
        _ => return None,
    })
}

pub(crate) fn folder_plan_to_json(plan: &linsync_core::FolderOperationPlan) -> serde_json::Value {
    let risk = plan.risk_summary();
    serde_json::json!({
        "operations": plan
            .operations
            .iter()
            .map(|op| serde_json::json!({
                "kind": format!("{:?}", op.kind),
                "relative_path": op.relative_path.display().to_string(),
                "source": op.source.as_ref().map(|p| p.display().to_string()),
                "target": op.target.as_ref().map(|p| p.display().to_string()),
                "overwrites_existing": op.overwrites_existing,
            }))
            .collect::<Vec<_>>(),
        "counts": {
            "copy_count": plan.counts.copy_count,
            "delete_count": plan.counts.delete_count,
            "rename_count": plan.counts.rename_count,
            "create_folder_count": plan.counts.create_folder_count,
            "refresh_count": plan.counts.refresh_count,
            "overwrite_warning_count": plan.counts.overwrite_warning_count,
            "permission_warning_count": plan.counts.permission_warning_count,
            "conflict_warning_count": plan.counts.conflict_warning_count,
        },
        "warnings": plan
            .warnings
            .iter()
            .map(|w| serde_json::json!({
                "relative_path": w.relative_path.display().to_string(),
                "kind": format!("{:?}", w.kind),
                "message": w.message,
            }))
            .collect::<Vec<_>>(),
        "risk_summary": {
            "total_operations": risk.total_operations,
            "overwrite_count": risk.overwrite_count,
            "delete_count": risk.delete_count,
            "high_risk_count": risk.high_risk_count,
        },
    })
}

pub(crate) fn folder_outcomes_to_json(
    plan: &linsync_core::FolderOperationPlan,
    outcomes: &[FolderOperationOutcome],
) -> serde_json::Value {
    let succeeded = outcomes
        .iter()
        .filter(|o| matches!(o.status, FolderOperationStatus::Succeeded))
        .count();
    let failed = outcomes
        .iter()
        .filter(|o| matches!(o.status, FolderOperationStatus::Failed))
        .count();
    serde_json::json!({
        "plan": folder_plan_to_json(plan),
        "outcomes": outcomes
            .iter()
            .map(|outcome| serde_json::json!({
                "kind": format!("{:?}", outcome.operation.kind),
                "relative_path": outcome.operation.relative_path.display().to_string(),
                "status": match outcome.status {
                    FolderOperationStatus::Succeeded => "succeeded",
                    FolderOperationStatus::Skipped => "skipped",
                    FolderOperationStatus::Failed => "failed",
                },
                "message": outcome.message,
            }))
            .collect::<Vec<_>>(),
        "summary": {
            "succeeded": succeeded,
            "failed": failed,
            "total": outcomes.len(),
        },
    })
}

pub(crate) fn image_regions_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let regions = match state.lock() {
        Ok(s) => s.last_image_result.as_ref().map(|r| r.diff_regions.clone()),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(regions) = regions else {
        return bridge_error(404, "Not Found", "no image compare result available");
    };
    let total = regions.len();
    let body = serde_json::json!({
        "regions": regions,
        "total": total,
    });
    let json = serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization error"}"#.to_owned());
    http_response(200, "OK", "application/json", json.into_bytes())
}

pub(crate) fn image_save_overlay_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let destination = PathBuf::from(path_str);
    if destination.as_os_str().is_empty() {
        return bridge_error(400, "Bad Request", "empty path");
    }
    if destination.is_dir() {
        return bridge_error(400, "Bad Request", "path points to a directory");
    }

    let source = match state.lock() {
        Ok(s) => s.last_image_overlay_path.clone(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(source) = source else {
        return bridge_error(404, "Not Found", "no image overlay available");
    };
    if !source.exists() {
        return bridge_error(
            404,
            "Not Found",
            "image overlay artifact is no longer available",
        );
    }

    if let Some(parent) = destination.parent()
        && !parent.as_os_str().is_empty()
        && let Err(err) = fs::create_dir_all(parent)
    {
        return bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to create destination directory: {err}"),
        );
    }

    match fs::copy(&source, &destination) {
        Ok(bytes) => {
            let body = serde_json::json!({
                "ok": true,
                "path": destination.display().to_string(),
                "bytes": bytes,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to save overlay: {err}"),
        ),
    }
}

pub(crate) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

pub(crate) fn binary_interpret_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let offset = match query_value(&params, "offset").and_then(|v| v.parse::<usize>().ok()) {
        Some(o) => o,
        None => return bridge_error(400, "Bad Request", "missing or invalid offset"),
    };
    let kind_str = match query_value(&params, "kind") {
        Some(k) => k,
        None => return bridge_error(400, "Bad Request", "missing kind"),
    };
    let kind = match parse_typed_value_kind(kind_str) {
        Some(k) => k,
        None => return bridge_error(400, "Bad Request", &format!("unknown kind '{kind_str}'")),
    };

    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    if tab.mode != "Hex" {
        return bridge_error(400, "Bad Request", "active tab is not a binary compare");
    }

    let left_bytes = match fs::read(&tab.left_path) {
        Ok(b) => b,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to read left file: {err}"),
            );
        }
    };
    let right_bytes = match fs::read(&tab.right_path) {
        Ok(b) => b,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to read right file: {err}"),
            );
        }
    };

    let result = compare_binary(
        &tab.left_path,
        &left_bytes,
        &tab.right_path,
        &right_bytes,
        &BinaryCompareOptions {
            compare_content: false,
            ..BinaryCompareOptions::default()
        },
    );

    let interpretation = match result.interpret_at(offset, kind) {
        Some(i) => i,
        None => return bridge_error(400, "Bad Request", "offset out of bounds"),
    };

    let body = serde_json::to_string(&interpretation)
        .unwrap_or_else(|_| r#"{"error":"serialization error"}"#.to_owned());
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// `/binary/window?offset=&limit=` — return a slice of hex rows for a binary
/// compare tab, so the GUI can page through large files without loading all
/// rows at once.
pub(crate) fn binary_window_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let offset = query_value(&params, "offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query_value(&params, "limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);

    // Extract only the requested window while holding the lock — cloning the
    // whole tab (every hex row of both sides) per page request would defeat
    // the point of windowing.
    let (total_rows, left_window, right_window) = match state.lock() {
        Ok(s) => {
            let Some(tab) = s
                .session
                .tabs
                .iter()
                .find(|t| t.id == s.session.active_tab_id)
            else {
                return bridge_error(404, "Not Found", "no active tab");
            };
            if tab.mode != "Hex" {
                return bridge_error(400, "Bad Request", "active tab is not a binary compare");
            }
            let total_rows = tab.left_rows.len().max(tab.right_rows.len());
            let end = offset.saturating_add(limit).min(total_rows);
            let window = |rows: &[GuiLineRow]| -> Vec<GuiLineRow> {
                rows.get(offset..end.min(rows.len()))
                    .map(<[GuiLineRow]>::to_vec)
                    .unwrap_or_default()
            };
            (total_rows, window(&tab.left_rows), window(&tab.right_rows))
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let returned = left_window.len().max(right_window.len());
    let body = serde_json::json!({
        "totalRows": total_rows,
        "offset": offset,
        "returned": returned,
        "hasMore": offset + returned < total_rows,
        "left_rows": left_window,
        "right_rows": right_window,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

pub(crate) fn parse_typed_value_kind(s: &str) -> Option<TypedValueKind> {
    match s {
        "u8" => Some(TypedValueKind::U8),
        "i8" => Some(TypedValueKind::I8),
        "u16_le" => Some(TypedValueKind::U16Le),
        "u16_be" => Some(TypedValueKind::U16Be),
        "i16_le" => Some(TypedValueKind::I16Le),
        "i16_be" => Some(TypedValueKind::I16Be),
        "u32_le" => Some(TypedValueKind::U32Le),
        "u32_be" => Some(TypedValueKind::U32Be),
        "i32_le" => Some(TypedValueKind::I32Le),
        "i32_be" => Some(TypedValueKind::I32Be),
        "u64_le" => Some(TypedValueKind::U64Le),
        "u64_be" => Some(TypedValueKind::U64Be),
        "i64_le" => Some(TypedValueKind::I64Le),
        "i64_be" => Some(TypedValueKind::I64Be),
        "f32_le" => Some(TypedValueKind::F32Le),
        "f32_be" => Some(TypedValueKind::F32Be),
        "f64_le" => Some(TypedValueKind::F64Le),
        "f64_be" => Some(TypedValueKind::F64Be),
        _ => None,
    }
}

pub(crate) fn merge_conflicts_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let active = match state.lock() {
        Ok(state) => state.active_tab().cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(tab) = active else {
        return bridge_error(400, "Bad Request", "no active compare tab");
    };
    if tab.mode != "Text" {
        return bridge_error(
            400,
            "Bad Request",
            "conflict navigation requires a text tab",
        );
    }
    let compare = compare_tab_text_rows(&tab);
    let conflicts: Vec<serde_json::Value> = compare
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| matches!(block.kind, DiffBlockKind::Difference))
        .map(|(index, block)| {
            serde_json::json!({
                "index": index,
                "left_start": block.left_start.unwrap_or_default(),
                "left_len": block.left_len,
                "right_start": block.right_start.unwrap_or_default(),
                "right_len": block.right_len,
            })
        })
        .collect();
    let body = serde_json::json!({
        "conflicts": conflicts,
        "total": compare.blocks.len(),
        "differences": compare.summary.diff_blocks,
        "can_save": tab.left_dirty || tab.right_dirty,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

// ── Three-way merge bridge handlers ──────────────────────────────────────────

/// Shared logic: read three files, create a `ThreeWayMergeState`, store it in
/// `state`, and return a JSON summary of the conflicts + current output text.
pub(crate) fn start_three_way_merge_session(
    base_path: &str,
    left_path: &str,
    right_path: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<String, String> {
    let base_doc = TextDocument::from_path(std::path::Path::new(base_path))
        .map_err(|err| format!("failed to read base '{}': {err}", base_path))?;
    let left_doc = TextDocument::from_path(std::path::Path::new(left_path))
        .map_err(|err| format!("failed to read left '{}': {err}", left_path))?;
    let right_doc = TextDocument::from_path(std::path::Path::new(right_path))
        .map_err(|err| format!("failed to read right '{}': {err}", right_path))?;

    let session = ThreeWayMergeState::new(base_doc, left_doc, right_doc);
    let conflicts_json = three_way_conflicts_json(&session);
    let output_text = session.output().text();

    match state.lock() {
        Ok(mut s) => {
            s.three_way_session = Some(session);
        }
        Err(_) => return Err("session state unavailable".to_owned()),
    }

    let body = serde_json::json!({
        "ok": true,
        "conflicts": conflicts_json,
        // At start nothing is resolved yet, so every conflict is unresolved.
        "unresolved_count": conflicts_json.len(),
        "output_text": output_text,
    })
    .to_string();
    Ok(body)
}

pub(crate) fn merge3_start_bridge_response(
    query: &str,
    _paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(base) = query_value(&params, "base") else {
        return bridge_error(400, "Bad Request", "missing base path");
    };
    let Some(left) = query_value(&params, "left") else {
        return bridge_error(400, "Bad Request", "missing left path");
    };
    let Some(right) = query_value(&params, "right") else {
        return bridge_error(400, "Bad Request", "missing right path");
    };

    match start_three_way_merge_session(base, left, right, state) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(400, "Bad Request", &err),
    }
}

/// Shared logic: resolve a conflict in the current `ThreeWayMergeState`.
pub(crate) fn resolve_three_way_conflict(
    id: u32,
    choice_str: &str,
    custom_text: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<String, String> {
    let choice = match choice_str {
        "left" => MergeChoice::Left,
        "right" => MergeChoice::Right,
        "base" => MergeChoice::Base,
        "custom" => MergeChoice::Custom(custom_text.to_owned()),
        other => return Err(format!("unsupported choice '{other}'")),
    };

    let mut guard = state
        .lock()
        .map_err(|_| "session state unavailable".to_owned())?;
    let session = guard
        .three_way_session
        .as_mut()
        .ok_or_else(|| "no active three-way merge session".to_owned())?;

    session
        .resolve(ConflictId(id), choice)
        .map_err(|err| err.to_string())?;

    let conflicts_json = three_way_conflicts_json(session);
    let output_text = session.output().text();
    // `conflicts` is the stable full list (it never shrinks as conflicts are
    // resolved), so the GUI must use `unresolved_count` for "remaining".
    let body = serde_json::json!({
        "ok": true,
        "conflicts": conflicts_json,
        "unresolved_count": session.unresolved_count(),
        "output_text": output_text,
    })
    .to_string();
    Ok(body)
}

pub(crate) fn merge3_resolve_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(id_str) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing conflict id");
    };
    let Ok(id) = id_str.parse::<u32>() else {
        return bridge_error(
            400,
            "Bad Request",
            "conflict id must be a non-negative integer",
        );
    };
    let Some(choice) = query_value(&params, "choice") else {
        return bridge_error(400, "Bad Request", "missing choice");
    };
    let text = query_value(&params, "text").unwrap_or("");

    match resolve_three_way_conflict(id, choice, text, state) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(400, "Bad Request", &err),
    }
}

/// Shared logic: write the current three-way merge output to a file.
pub(crate) fn save_three_way_merge_output(
    path: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<(), String> {
    let text = {
        let guard = state
            .lock()
            .map_err(|_| "session state unavailable".to_owned())?;
        let session = guard
            .three_way_session
            .as_ref()
            .ok_or_else(|| "no active three-way merge session".to_owned())?;
        session.output().text()
    };
    std::fs::write(path, text).map_err(|err| format!("failed to save merged output: {err}"))
}

pub(crate) fn validate_merge_session(session: &ThreeWayMergeState) -> Result<(), usize> {
    let unresolved = session.unresolved_count();
    if unresolved == 0 {
        Ok(())
    } else {
        Err(unresolved)
    }
}

pub(crate) fn merge3_save_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };

    {
        let guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "session state unavailable");
            }
        };
        let Some(session) = guard.three_way_session.as_ref() else {
            return bridge_error(400, "Bad Request", "no active three-way merge session");
        };
        if let Err(count) = validate_merge_session(session) {
            return http_response(
                409,
                "Conflict",
                "application/json",
                serde_json::json!({
                    "ok": false,
                    "error": format!("{count} unresolved conflict(s) remain"),
                    "unresolved_count": count,
                })
                .to_string()
                .into_bytes(),
            );
        }
    }

    match save_three_way_merge_output(path, state) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => {
            let body = serde_json::json!({ "ok": false, "error": err }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

pub(crate) fn three_way_conflicts_json(session: &ThreeWayMergeState) -> Vec<serde_json::Value> {
    session
        .conflicts()
        .into_iter()
        .map(|conflict| {
            serde_json::json!({
                "id": conflict.id.0,
                "start_line": conflict.start_line,
                "end_line": conflict.end_line,
                "base_lines": conflict.base_lines,
                "left_lines": conflict.left_lines,
                "right_lines": conflict.right_lines,
            })
        })
        .collect()
}

pub(crate) fn query_params(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

pub(crate) fn query_value<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

/// All values for a repeated query key, in order (each already percent-decoded
/// by [`query_params`]). Used for multi-valued params like `entries`, where one
/// param per item avoids an in-band separator that would split values
/// containing that separator (e.g. a path with a comma).
pub(crate) fn query_values<'a>(params: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    params
        .iter()
        .filter(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
        .collect()
}

pub(crate) fn query_bool(params: &[(String, String)], key: &str) -> bool {
    query_value(params, key).is_some_and(|value| {
        value.eq_ignore_ascii_case("1")
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
    })
}

pub(crate) fn bridge_error(status: u16, reason: &str, message: &str) -> Vec<u8> {
    let body = serde_json::json!({ "error": message })
        .to_string()
        .into_bytes();
    http_response(status, reason, "application/json", body)
}

pub(crate) fn bridge_error_json(status: u16, reason: &str, body: serde_json::Value) -> Vec<u8> {
    http_response(
        status,
        reason,
        "application/json",
        body.to_string().into_bytes(),
    )
}

pub(crate) fn http_response(
    status: u16,
    reason: &str,
    content_type: &str,
    body: Vec<u8>,
) -> Vec<u8> {
    let body = version_json_response_body(content_type, body);
    // No `Access-Control-Allow-Origin` header: the bridge is intended for the local
    // QML host. Allowing a wildcard origin would let any web page on the user's
    // machine read files via /compare and overwrite them via /copy-all + /save.
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

pub(crate) fn version_json_response_body(content_type: &str, body: Vec<u8>) -> Vec<u8> {
    if !content_type
        .split(';')
        .next()
        .is_some_and(|ty| ty.trim().eq_ignore_ascii_case("application/json"))
    {
        return body;
    }
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };
    if !value.is_object() {
        return body;
    }
    insert_response_schema_version(&mut value);
    serde_json::to_vec(&value).unwrap_or(body)
}

#[cfg(feature = "cxxqt-app")]
pub(crate) fn use_cxxqt_host() -> bool {
    // Default to the in-process Qt host. It sets the Wayland xdg_toplevel
    // app_id to "com.visorcraft.LinSync" before the window maps, which is
    // what KDE Plasma needs to associate the running window with the
    // pinned launcher (com.visorcraft.LinSync.desktop). The external qml6
    // runner can't do this because it stamps its own app_id
    // ("org.qt-project.qml") onto every window it creates.
    //
    // Set LINSYNC_QML_HOST=external to force the legacy qml6 spawn.
    !matches!(
        env::var("LINSYNC_QML_HOST"),
        Ok(value) if value.eq_ignore_ascii_case("external")
    )
}

#[cfg(feature = "cxxqt-app")]
pub(crate) fn run_cxxqt_host(
    paths: &AppPaths,
    qml_file: &Path,
    launch_context_path: Option<&Path>,
    launch_context: Option<GuiLaunchContext>,
) -> Result<ExitCode, String> {
    use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

    let qml_root = qml_file
        .parent()
        .ok_or_else(|| format!("invalid QML file path '{}'", qml_file.display()))?;

    // Set QML/Qt environment variables BEFORE spawning any threads.
    // Rust 2024 requires `unsafe` for env::set_var because POSIX setenv is
    // not guaranteed thread-safe vs concurrent getenv. Setting these here
    // (before start_bridge_server spawns the listener thread) avoids the race.
    unsafe {
        env::set_var("QML_XHR_ALLOW_FILE_READ", "1");
        if env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
            env::set_var("QT_QUICK_CONTROLS_STYLE", "Fusion");
        }
    }

    // Start the HTTP bridge so Main.qml can read bridgeUrl as soon as
    // Component.onCompleted fires. Both the external qml6 host and the
    // in-process cxx-qt host drive the UI over this single HTTP transport.
    let bridge = start_bridge_server(paths.clone(), launch_context)?;
    let bridge_info = serde_json::json!({
        "bridge_url": &bridge.base_url,
        "version": env!("CARGO_PKG_VERSION"),
        "context_path": launch_context_path.map(|p| p.display().to_string()),
        "section": env::var("LINSYNC_STARTUP_SECTION").ok().filter(|s| !s.is_empty()),
    });
    let payload = serde_json::to_string(&bridge_info).unwrap();
    let bridge_info_path = write_bridge_info_file(payload.as_bytes());
    if bridge_info_path.is_none() {
        tracing::warn!("bridge info sidecar not written; GUI will run without the HTTP bridge");
    }
    // SAFETY: LINSYNC_BRIDGE_INFO is only read by the QML layer (which runs
    // later), never by bridge worker threads, so there is no concurrent
    // getenv on this variable. The other env vars were already set above,
    // before any threads were spawned.
    unsafe {
        if let Some(ref path) = bridge_info_path {
            env::set_var("LINSYNC_BRIDGE_INFO", path.display().to_string());
        }
    }

    let mut app = QGuiApplication::new();
    // setDesktopFileName must run before any QWindow is mapped — Qt reads it
    // once in QWaylandWindow::initWindow() to set xdg_toplevel.app_id, which
    // is what KDE Plasma matches against the .desktop file basename for
    // taskbar grouping.
    QGuiApplication::set_desktop_file_name(&QString::from("com.visorcraft.LinSync"));
    app.pin_mut()
        .set_application_name(&QString::from("LinSync"));
    app.pin_mut()
        .set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    app.pin_mut()
        .set_organization_name(&QString::from("VisorCraft"));
    app.pin_mut()
        .set_organization_domain(&QString::from("visorcraft.com"));

    // Install a UI translation catalog for the active locale, if one ships
    // alongside the QML (sibling `i18n/` dir holding `linsync_<locale>.qm`).
    // No-op when no catalog matches the locale, so the English source strings
    // remain. Must run before the engine loads QML so qsTr() resolves.
    if let Some(i18n_dir) = qml_root.parent().map(|p| p.join("i18n")) {
        let loaded = crate::cxxqt_translator::ffi::linsync_install_translator(&QString::from(
            i18n_dir.to_string_lossy().as_ref(),
        ));
        if loaded {
            tracing::info!(dir = %i18n_dir.display(), "installed UI translation catalog");
        }
    }

    // AppImage builds bundle Breeze but cannot rely on the host's icon theme
    // search paths, so point Qt at the bundled AppDir/share/icons tree.
    icon_theme::set_icon_theme("breeze");

    let mut engine = QQmlApplicationEngine::new();
    engine
        .pin_mut()
        .add_import_path(&QString::from(qml_root.display().to_string()));
    let qml_url = QUrl::from_local_file(&QString::from(qml_file.display().to_string()));
    engine.pin_mut().load(&qml_url);

    let code = app.pin_mut().exec();
    Ok(ExitCode::from(code.clamp(0, u8::MAX as i32) as u8))
}

pub(crate) fn print_help() {
    println!(
        "LinSync GUI\n\nUsage: linsync [--print-qml-path] [--] [PATH...]\n\nEnvironment:\n  LINSYNC_QML_ROOT    Directory containing Main.qml\n  LINSYNC_QML_RUNNER  Qt QML runner command, defaulting to qml6/qml\n  LINSYNC_QML_HOST    Set to external to force the fallback QML runner when cxxqt-app is enabled"
    );
}

pub(crate) fn resolve_qml_file() -> Result<PathBuf, String> {
    qml_file_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "could not find LinSync QML resources; set LINSYNC_QML_ROOT".to_owned())
}

pub(crate) fn qml_file_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(root) = env::var_os("LINSYNC_QML_ROOT") {
        candidates.push(PathBuf::from(root).join("Main.qml"));
    }

    if let Ok(exe) = env::current_exe()
        && let Some(bin_dir) = exe.parent()
    {
        candidates.push(bin_dir.join("../share/linsync/qml/Main.qml"));
        candidates.push(bin_dir.join("../../share/linsync/qml/Main.qml"));
    }

    candidates.push(PathBuf::from("/app/share/linsync/qml/Main.qml"));
    candidates.push(PathBuf::from("/usr/local/share/linsync/qml/Main.qml"));
    candidates.push(PathBuf::from("/usr/share/linsync/qml/Main.qml"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml"));
    candidates
}

pub(crate) fn resolve_window_icon_file(qml_file: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(qml_root) = qml_file.parent() {
        candidates.push(qml_root.join("assets/com.visorcraft.LinSync.png"));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join("qml/assets/com.visorcraft.LinSync.png"));
    candidates.push(
        manifest_dir.join("../../packaging/icons/hicolor/512x512/apps/com.visorcraft.LinSync.png"),
    );
    candidates.push(
        manifest_dir.join("../../packaging/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg"),
    );

    candidates.into_iter().find(|path| path.is_file())
}

pub(crate) fn resolve_qml_runner() -> Option<PathBuf> {
    if let Some(value) = env::var_os("LINSYNC_QML_RUNNER")
        && !value.is_empty()
    {
        return Some(PathBuf::from(value));
    }

    ["qml6", "qml"].into_iter().find_map(find_command_in_path)
}

pub(crate) fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(command))
            .find(|candidate| candidate.is_file())
    })
}
