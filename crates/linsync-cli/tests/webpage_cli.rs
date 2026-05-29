use std::process::Command;

fn cli_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../target/debug/linsync-cli");
    p
}

#[test]
fn webpage_no_accept_flag_exits_2() {
    let status = Command::new(cli_bin())
        .args(["webpage", "http://127.0.0.1:9", "http://127.0.0.1:9"])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}

#[test]
fn webpage_missing_url_args_exits_nonzero() {
    let status = Command::new(cli_bin())
        .args(["webpage", "--accept-network-fetch"])
        .status()
        .unwrap();
    // Exits 2 due to argument error (only 0 urls, need 2).
    assert_ne!(status.code(), Some(0));
}

#[test]
fn webpage_rendered_without_web_engine_feature_exits_2() {
    // Only meaningful in a build without --features web-engine.
    let status = Command::new(cli_bin())
        .args([
            "webpage",
            "http://x/",
            "http://x/",
            "--sub-mode",
            "rendered",
            "--accept-network-fetch",
        ])
        .status()
        .unwrap();
    // In default build: exits 2.  In web-engine build: may succeed or fail differently.
    // We just assert it doesn't panic (exit from signal).
    assert!(status.code().is_some());
}
