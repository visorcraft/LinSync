fn main() {
    let ok = matches!(
        linsync_sandbox::SandboxStrategy::detect(),
        linsync_sandbox::SandboxStrategy::Degraded
    );
    std::process::exit(if ok { 0 } else { 1 });
}
