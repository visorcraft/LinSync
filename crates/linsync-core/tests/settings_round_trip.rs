use linsync_core::{DeletePreference, SettingsStore, ThemePreference};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn round_trip_every_settings_key() {
    let dir = TempFixture::new();
    let store = SettingsStore::new(dir.path.join("settings.json"));

    let mut s = store.load_or_default().unwrap();
    s.theme_preference = ThemePreference::Dark;
    s.pane_font_size = 14;
    s.pane_font_family = "Iosevka".into();
    s.pane_tab_width = 8;
    s.show_line_numbers = true;
    s.show_whitespace = true;
    s.word_wrap = true;
    s.ignore_case = true;
    s.ignore_whitespace = true;
    s.ignore_blank_lines = true;
    s.ignore_eol = true;
    s.eol_normalization = "lf".into();
    s.default_compare_mode = "binary".into();
    s.open_last_session = true;
    s.confirm_on_close = true;
    s.persist_recent_paths = true;
    s.recent_limit = 25;
    s.default_recursive_folder_compare = false;
    s.delete_preference = DeletePreference::Permanent;
    s.confirm_permanent_delete = false;
    s.window_size = Some(linsync_core::WindowSize {
        width: 1920,
        height: 1080,
    });
    s.respect_gitignore = false;
    s.follow_symlinks = true;
    s.max_walk_depth = 10;
    s.session_includes = vec!["*.rs".to_string()];
    s.session_excludes = vec!["target".to_string()];
    store.save(&s).unwrap();

    let loaded = store.load_or_default().unwrap();
    assert_eq!(loaded.theme_preference, ThemePreference::Dark);
    assert_eq!(loaded.pane_font_size, 14);
    assert_eq!(loaded.pane_font_family, "Iosevka");
    assert_eq!(loaded.pane_tab_width, 8);
    assert!(loaded.show_line_numbers);
    assert!(loaded.show_whitespace);
    assert!(loaded.word_wrap);
    assert!(loaded.ignore_case);
    assert!(loaded.ignore_whitespace);
    assert!(loaded.ignore_blank_lines);
    assert!(loaded.ignore_eol);
    assert_eq!(loaded.eol_normalization, "lf");
    assert_eq!(loaded.default_compare_mode, "binary");
    assert!(loaded.open_last_session);
    assert!(loaded.confirm_on_close);
    assert!(loaded.persist_recent_paths);
    assert_eq!(loaded.recent_limit, 25);
    assert!(!loaded.default_recursive_folder_compare);
    assert_eq!(loaded.delete_preference, DeletePreference::Permanent);
    assert!(!loaded.confirm_permanent_delete);
    assert_eq!(
        loaded.window_size,
        Some(linsync_core::WindowSize {
            width: 1920,
            height: 1080,
        })
    );
    assert!(!loaded.respect_gitignore);
    assert!(loaded.follow_symlinks);
    assert_eq!(loaded.max_walk_depth, 10);
    assert_eq!(loaded.session_includes, vec!["*.rs"]);
    assert_eq!(loaded.session_excludes, vec!["target"]);
}

struct TempFixture {
    path: PathBuf,
}

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

impl TempFixture {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "linsync-core-settings-round-trip-{}-{suffix}-{sequence}",
            std::process::id()
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
