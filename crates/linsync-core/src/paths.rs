use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub log_file: PathBuf,
}

impl AppPaths {
    /// Read an environment variable as a directory path, treating an **empty**
    /// value as unset. `env::var_os` returns `Some("")` for an empty variable;
    /// without this an empty `HOME`/`XDG_*` would produce relative paths and
    /// LinSync would write config/data/cache under the process CWD.
    fn env_dir(key: &str) -> Option<PathBuf> {
        let value = env::var_os(key)?;
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    }

    pub fn from_env() -> Self {
        let home = Self::env_dir("HOME").unwrap_or_else(|| PathBuf::from("."));

        Self::from_base_dirs(
            Self::env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            Self::env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
            Self::env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")),
            Self::env_dir("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state")),
        )
    }

    pub fn from_base_dirs(
        config_home: PathBuf,
        data_home: PathBuf,
        cache_home: PathBuf,
        state_home: PathBuf,
    ) -> Self {
        let config_dir = config_home.join("linsync");
        let data_dir = data_home.join("linsync");
        let cache_dir = cache_home.join("linsync");
        let state_dir = state_home.join("linsync");
        let log_file = state_dir.join("linsync.log");

        Self {
            config_dir,
            data_dir,
            cache_dir,
            state_dir,
            log_file,
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn filters_file(&self) -> PathBuf {
        self.config_dir.join("filters.json")
    }

    /// Directory holding user-defined compare profiles, one JSON file per profile:
    /// `$XDG_CONFIG_HOME/linsync/profiles/<profile_id>.json`.
    pub fn profiles_dir(&self) -> PathBuf {
        self.config_dir.join("profiles")
    }

    /// Records the user's active-profile pointer:
    /// `$XDG_CONFIG_HOME/linsync/active-profile.json`.
    pub fn active_profile_pointer_file(&self) -> PathBuf {
        self.config_dir.join("active-profile.json")
    }

    pub fn plugins_enabled_file(&self) -> PathBuf {
        self.config_dir.join("plugins.json")
    }

    /// Per-plugin "trusted" flags, recorded the first time a user authorizes a
    /// discovered plugin to run: `$XDG_CONFIG_HOME/linsync/plugins-trusted.json`.
    pub fn plugins_trusted_file(&self) -> PathBuf {
        self.config_dir.join("plugins-trusted.json")
    }

    /// Directory for per-plugin option files:
    /// `$XDG_CONFIG_HOME/linsync/plugin-options/<plugin_id>.json`
    pub fn plugin_options_dir(&self) -> PathBuf {
        self.config_dir.join("plugin-options")
    }

    /// Path for a specific plugin's options file.
    pub fn plugin_options_file(&self, plugin_id: &str) -> PathBuf {
        self.plugin_options_dir().join(format!("{plugin_id}.json"))
    }

    pub fn recent_paths_file(&self) -> PathBuf {
        self.data_dir.join("recent-paths.json")
    }

    pub fn recent_sessions_file(&self) -> PathBuf {
        self.data_dir.join("recent-sessions.json")
    }

    /// Most-recently saved/opened project files (recent workspaces).
    pub fn recent_projects_file(&self) -> PathBuf {
        self.data_dir.join("recent-projects.json")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.data_dir.join("projects")
    }

    pub fn user_plugins_dir(&self) -> PathBuf {
        self.data_dir.join("plugins")
    }

    pub fn comparison_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("comparisons")
    }

    pub fn system_plugins_dirs() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/usr/local/share/linsync/plugins"),
            PathBuf::from("/usr/share/linsync/plugins"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_xdg_app_paths() {
        let paths = AppPaths::from_base_dirs(
            PathBuf::from("/config"),
            PathBuf::from("/data"),
            PathBuf::from("/cache"),
            PathBuf::from("/state"),
        );

        assert_eq!(paths.config_dir, PathBuf::from("/config/linsync"));
        assert_eq!(paths.data_dir, PathBuf::from("/data/linsync"));
        assert_eq!(paths.cache_dir, PathBuf::from("/cache/linsync"));
        assert_eq!(paths.state_dir, PathBuf::from("/state/linsync"));
        assert_eq!(paths.log_file, PathBuf::from("/state/linsync/linsync.log"));
        assert_eq!(
            paths.settings_file(),
            PathBuf::from("/config/linsync/settings.json")
        );
        assert_eq!(
            paths.filters_file(),
            PathBuf::from("/config/linsync/filters.json")
        );
        assert_eq!(
            paths.plugins_enabled_file(),
            PathBuf::from("/config/linsync/plugins.json")
        );
        assert_eq!(
            paths.recent_paths_file(),
            PathBuf::from("/data/linsync/recent-paths.json")
        );
        assert_eq!(
            paths.recent_sessions_file(),
            PathBuf::from("/data/linsync/recent-sessions.json")
        );
        assert_eq!(
            paths.sessions_dir(),
            PathBuf::from("/data/linsync/sessions")
        );
        assert_eq!(
            paths.projects_dir(),
            PathBuf::from("/data/linsync/projects")
        );
        assert_eq!(
            paths.user_plugins_dir(),
            PathBuf::from("/data/linsync/plugins")
        );
        assert_eq!(
            paths.comparison_cache_dir(),
            PathBuf::from("/cache/linsync/comparisons")
        );
        assert_eq!(
            AppPaths::system_plugins_dirs(),
            vec![
                PathBuf::from("/usr/local/share/linsync/plugins"),
                PathBuf::from("/usr/share/linsync/plugins")
            ]
        );
        assert_eq!(
            paths.plugin_options_dir(),
            PathBuf::from("/config/linsync/plugin-options")
        );
        assert_eq!(
            paths.plugin_options_file("example.zip"),
            PathBuf::from("/config/linsync/plugin-options/example.zip.json")
        );
    }
}
