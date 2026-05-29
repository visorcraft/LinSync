use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use tracing_subscriber::fmt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::paths::AppPaths;

#[derive(Debug)]
pub enum LoggingError {
    Io(io::Error),
    Subscriber(tracing_subscriber::util::TryInitError),
}

impl std::fmt::Display for LoggingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Subscriber(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for LoggingError {}

impl From<io::Error> for LoggingError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tracing_subscriber::util::TryInitError> for LoggingError {
    fn from(value: tracing_subscriber::util::TryInitError) -> Self {
        Self::Subscriber(value)
    }
}

pub fn init_file_logging(paths: &AppPaths) -> Result<(), LoggingError> {
    fs::create_dir_all(&paths.state_dir)?;
    let file = open_owner_only_append(&paths.log_file)?;

    fmt()
        .json()
        .with_ansi(false)
        .with_writer(Mutex::new(file))
        .finish()
        .try_init()?;

    Ok(())
}

pub fn install_panic_log_hook(paths: &AppPaths) -> Result<(), LoggingError> {
    fs::create_dir_all(&paths.state_dir)?;
    let log_file = paths.log_file.clone();
    let previous = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = append_panic(&log_file, &panic_info.to_string());
        previous(panic_info);
    }));

    Ok(())
}

fn append_panic(log_file: &PathBuf, panic_message: &str) -> io::Result<()> {
    let message = serde_json::to_string(panic_message).map_err(io::Error::other)?;
    let mut file = open_owner_only_append(log_file)?;
    writeln!(
        file,
        "{{\"level\":\"ERROR\",\"target\":\"panic\",\"message\":{}}}",
        message
    )
}

fn open_owner_only_append(path: &PathBuf) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_structured_log_file_under_state_dir() {
        let fixture = TempFixture::new();
        let paths = AppPaths::from_base_dirs(
            fixture.path.join("config"),
            fixture.path.join("data"),
            fixture.path.join("cache"),
            fixture.path.join("state"),
        );

        init_file_logging(&paths).unwrap();
        tracing::info!(target: "linsync_test", action = "startup", "logging ready");

        let log = fs::read_to_string(&paths.log_file).unwrap();
        assert!(log.contains(r#""target":"linsync_test""#));
        assert!(log.contains(r#""action":"startup""#));
        assert!(log.contains("logging ready"));
    }

    #[test]
    fn panic_log_message_is_valid_json_string() {
        let fixture = TempFixture::new();
        let log_file = fixture.path.join("panic.log");

        append_panic(&log_file, "bad\u{0007}\"message").unwrap();

        let line = fs::read_to_string(log_file).unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["message"], "bad\u{0007}\"message");
    }

    #[cfg(unix)]
    #[test]
    fn log_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempFixture::new();
        let log_file = fixture.path.join("private.log");

        append_panic(&log_file, "private path").unwrap();

        assert_eq!(
            fs::metadata(log_file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "linsync-logging-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
