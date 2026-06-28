use super::*;

pub(crate) fn launch_command(args: &[String]) -> Result<ExitCode, String> {
    let mut wait = false;
    let mut gui_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--wait" => {
                wait = true;
                index += 1;
            }
            "--" => {
                gui_args.extend(args[index + 1..].iter().cloned());
                break;
            }
            value => {
                gui_args.push(value.to_owned());
                index += 1;
            }
        }
    }

    let gui = resolve_gui_binary();
    let mut command = Command::new(&gui);
    command.args(&gui_args);

    if wait {
        let status = command
            .status()
            .map_err(|err| format!("failed to launch GUI '{}': {err}", gui.display()))?;
        return Ok(if status.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        });
    }

    command
        .spawn()
        .map_err(|err| format!("failed to launch GUI '{}': {err}", gui.display()))?;
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn resolve_gui_binary() -> PathBuf {
    if let Some(value) = env::var_os("LINSYNC_GUI") {
        return PathBuf::from(value);
    }

    if let Ok(current_exe) = env::current_exe()
        && let Some(directory) = current_exe.parent()
    {
        let sibling = directory.join("linsync");
        if sibling.exists() {
            return sibling;
        }
    }

    PathBuf::from("linsync")
}

pub(crate) fn open_external_command(args: &[String]) -> Result<ExitCode, String> {
    let external_args = split_open_external_args(args)?;
    if external_args.paths.is_empty() {
        return Err(
            "usage: linsync-cli open-external [--wait] [--preset PRESET] PATH...".to_owned(),
        );
    }

    let opener = resolve_external_opener(external_args.preset)?;
    let mut exit_code = ExitCode::SUCCESS;

    for path in external_args.paths {
        let mut command = Command::new(&opener.program);
        command.args(&opener.args);
        command.arg(path);
        if external_args.wait {
            let status = command.status().map_err(|err| {
                format!(
                    "failed to open '{}' with '{}': {err}",
                    path,
                    opener.program.display()
                )
            })?;
            if !status.success() {
                // Exit code 1 means "differences found" per the documented
                // contract. A failed/signalled external opener is a runtime
                // error and must surface as 2.
                exit_code = ExitCode::from(2);
            }
        } else {
            command.spawn().map_err(|err| {
                format!(
                    "failed to open '{}' with '{}': {err}",
                    path,
                    opener.program.display()
                )
            })?;
        }
    }

    Ok(exit_code)
}

#[derive(Debug, Clone)]
pub(crate) struct OpenExternalArgs<'a> {
    pub(crate) wait: bool,
    pub(crate) preset: Option<&'a str>,
    pub(crate) paths: Vec<&'a str>,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandTemplate {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
}

pub(crate) fn split_open_external_args(args: &[String]) -> Result<OpenExternalArgs<'_>, String> {
    let mut wait = false;
    let mut preset = None;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--wait" => {
                wait = true;
                index += 1;
            }
            "--preset" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("open-external --preset requires a preset name".to_owned());
                };
                preset = Some(value.as_str());
                index += 2;
            }
            value if let Some(value) = value.strip_prefix("--preset=") => {
                if value.is_empty() {
                    return Err("open-external --preset requires a preset name".to_owned());
                }
                preset = Some(value);
                index += 1;
            }
            value => {
                paths.push(value);
                index += 1;
            }
        }
    }

    Ok(OpenExternalArgs {
        wait,
        preset,
        paths,
    })
}

pub(crate) fn resolve_external_opener(preset: Option<&str>) -> Result<CommandTemplate, String> {
    if let Some(preset) = preset {
        return external_opener_preset(preset);
    }

    Ok(CommandTemplate {
        program: env::var_os("LINSYNC_OPEN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("xdg-open")),
        args: Vec::new(),
    })
}

pub(crate) fn external_opener_preset(preset: &str) -> Result<CommandTemplate, String> {
    let template = match preset {
        "xdg-open" => CommandTemplate {
            program: PathBuf::from("xdg-open"),
            args: Vec::new(),
        },
        "kate" => CommandTemplate {
            program: PathBuf::from("kate"),
            args: Vec::new(),
        },
        "kwrite" => CommandTemplate {
            program: PathBuf::from("kwrite"),
            args: Vec::new(),
        },
        "vscode" => CommandTemplate {
            program: PathBuf::from("code"),
            args: Vec::new(),
        },
        "vscodium" => CommandTemplate {
            program: PathBuf::from("codium"),
            args: Vec::new(),
        },
        "gnome-text-editor" => CommandTemplate {
            program: PathBuf::from("gnome-text-editor"),
            args: Vec::new(),
        },
        "sublime" => CommandTemplate {
            program: PathBuf::from("subl"),
            args: Vec::new(),
        },
        "nvim-terminal" => CommandTemplate {
            program: env::var_os("LINSYNC_TERMINAL")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("x-terminal-emulator")),
            args: vec!["-e".to_owned(), "nvim".to_owned()],
        },
        "jetbrains-idea" => jetbrains_template("idea"),
        "jetbrains-pycharm" => jetbrains_template("pycharm"),
        "jetbrains-webstorm" => jetbrains_template("webstorm"),
        "jetbrains-clion" => jetbrains_template("clion"),
        "jetbrains-rider" => jetbrains_template("rider"),
        "jetbrains-goland" => jetbrains_template("goland"),
        "jetbrains-phpstorm" => jetbrains_template("phpstorm"),
        "jetbrains-rubymine" => jetbrains_template("rubymine"),
        "jetbrains-datagrip" => jetbrains_template("datagrip"),
        _ => {
            return Err(format!(
                "unknown external editor preset '{preset}'; expected one of: {}",
                OPEN_EXTERNAL_PRESETS.join(", ")
            ));
        }
    };

    Ok(template)
}

pub(crate) fn jetbrains_template(program: &str) -> CommandTemplate {
    CommandTemplate {
        program: PathBuf::from(program),
        args: Vec::new(),
    }
}

pub(crate) fn reveal_command(args: &[String]) -> Result<ExitCode, String> {
    let mut wait = false;
    let mut paths = Vec::new();

    for arg in args {
        if arg == "--wait" {
            wait = true;
        } else {
            paths.push(arg);
        }
    }

    if paths.is_empty() {
        return Err("usage: linsync-cli reveal [--wait] PATH...".to_owned());
    }

    let configured_revealer = env::var_os("LINSYNC_REVEAL").map(PathBuf::from);
    let mut exit_code = ExitCode::SUCCESS;

    for path in paths {
        let code = if let Some(revealer) = configured_revealer.as_ref() {
            reveal_with_command(path, revealer, wait)?
        } else if reveal_with_file_manager1(path)? {
            ExitCode::SUCCESS
        } else {
            reveal_containing_folder(path, wait)?
        };
        if code != ExitCode::SUCCESS {
            exit_code = code;
        }
    }

    Ok(exit_code)
}

pub(crate) fn reveal_with_command(
    path: &str,
    revealer: &Path,
    wait: bool,
) -> Result<ExitCode, String> {
    let target = PathBuf::from(path);
    let mut command = Command::new(revealer);
    command.arg(&target);
    if wait {
        let status = command.status().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                revealer.display()
            )
        })?;
        Ok(if status.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        })
    } else {
        command.spawn().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                revealer.display()
            )
        })?;
        Ok(ExitCode::SUCCESS)
    }
}

pub(crate) fn reveal_with_file_manager1(path: &str) -> Result<bool, String> {
    let uri = file_uri(Path::new(path))?;
    let status = Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{uri}"),
            "string:",
        ])
        .status();

    Ok(status.is_ok_and(|status| status.success()))
}

pub(crate) fn reveal_containing_folder(path: &str, wait: bool) -> Result<ExitCode, String> {
    let target = containing_folder(path);
    let opener = PathBuf::from("xdg-open");
    let mut command = Command::new(&opener);
    command.arg(&target);
    if wait {
        let status = command.status().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                opener.display()
            )
        })?;
        Ok(if status.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        })
    } else {
        command.spawn().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                opener.display()
            )
        })?;
        Ok(ExitCode::SUCCESS)
    }
}

pub(crate) fn file_uri(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|err| format!("cannot resolve current directory for reveal: {err}"))?
            .join(path)
    };
    Ok(path_to_file_uri(&absolute))
}

pub(crate) fn path_to_file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(*byte as char)
            }
            byte => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

pub(crate) fn containing_folder(path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_dir() {
        return path.to_path_buf();
    }

    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
