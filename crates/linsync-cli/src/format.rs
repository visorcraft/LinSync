use super::*;

pub(crate) fn folder_state(state: FolderEntryState) -> &'static str {
    match state {
        FolderEntryState::Identical => "identical",
        FolderEntryState::Different => "different",
        FolderEntryState::LeftOnly => "left-only",
        FolderEntryState::RightOnly => "right-only",
        FolderEntryState::Skipped => "skipped",
        FolderEntryState::Error => "error",
        FolderEntryState::Aborted => "aborted",
    }
}

pub(crate) fn folder_status(status: linsync_core::FolderCompareStatus) -> &'static str {
    match status {
        linsync_core::FolderCompareStatus::Complete => "complete",
        linsync_core::FolderCompareStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn folder_options_metadata_json(
    options: &FolderCompareOptions,
    effective_profile: Option<&str>,
    state_filter: FolderEntryFilter,
) -> serde_json::Value {
    let timestamp_tolerance_ms =
        u64::try_from(options.timestamp_tolerance.as_millis()).unwrap_or(u64::MAX);
    serde_json::json!({
        "profile": effective_profile,
        "recursive": options.recursive,
        "compare_method": options.compare_method.as_str(),
        "timestamp_tolerance_ms": timestamp_tolerance_ms,
        "symlink_policy": symlink_policy_cli_value(options.symlink_policy),
        "large_file_threshold_bytes": options.large_file_threshold,
        "large_file_fallback_method": options.large_file_fallback_method.as_str(),
        "hash_algorithm": hash_algorithm_cli_value(options.hash_algorithm),
        "compare_permissions": options.compare_permissions,
        "compare_ownership": options.compare_ownership,
        "compare_xattrs": options.compare_xattrs,
        "include_skipped": options.include_skipped,
        "state_filter": folder_entry_filter_value(state_filter),
        "filter_match_options": options.filter_match_options,
        "filters": &options.filters,
    })
}

pub(crate) fn symlink_policy_cli_value(policy: SymlinkPolicy) -> &'static str {
    match policy {
        SymlinkPolicy::CompareTarget => "target",
        SymlinkPolicy::Follow => "follow",
        SymlinkPolicy::SpecialFile => "special",
    }
}

pub(crate) fn hash_algorithm_cli_value(algorithm: HashAlgorithm) -> &'static str {
    match algorithm {
        HashAlgorithm::Blake3 => "blake3",
        HashAlgorithm::Sha256 => "sha256",
        HashAlgorithm::Crc32 => "crc32",
    }
}

pub(crate) fn folder_entry_filter_value(filter: FolderEntryFilter) -> &'static str {
    match filter {
        FolderEntryFilter::All => "all",
        FolderEntryFilter::Differences => "differences",
        FolderEntryFilter::Identical => "identical",
        FolderEntryFilter::Different => "different",
        FolderEntryFilter::LeftOnly => "left-only",
        FolderEntryFilter::RightOnly => "right-only",
        FolderEntryFilter::Errors => "errors",
        FolderEntryFilter::Skipped => "skipped",
        FolderEntryFilter::Aborted => "aborted",
    }
}

pub(crate) fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

pub(crate) fn system_time_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| duration.as_millis().try_into().ok())
}

pub(crate) fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
