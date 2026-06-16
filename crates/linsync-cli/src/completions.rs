use super::*;

pub(crate) fn completions_command(args: &[String]) -> Result<ExitCode, String> {
    if args.len() != 1 {
        return Err("usage: linsync-cli completions SHELL".to_owned());
    }

    let completions = match args[0].as_str() {
        "bash" => bash_completions(),
        "zsh" => zsh_completions(),
        "fish" => fish_completions(),
        other => {
            return Err(format!(
                "unsupported completion shell '{other}'; expected bash, zsh, or fish"
            ));
        }
    };

    print!("{completions}");
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn bash_completions() -> String {
    format!(
        r#"# bash completion for linsync-cli
_linsync_cli() {{
    local cur prev cmd
    COMPREPLY=()
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    cmd="${{COMP_WORDS[1]}}"

    if [[ $COMP_CWORD -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "{}" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--method" ]]; then
        COMPREPLY=( $(compgen -W "full quick binary modified-date date-size size existence hash-blake3 normalized-text" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--type" ]]; then
        COMPREPLY=( $(compgen -W "auto text binary hex folder table image document" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--diff-algorithm" ]]; then
        COMPREPLY=( $(compgen -W "lcs patience myers" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--inline-granularity" ]]; then
        COMPREPLY=( $(compgen -W "char word grapheme" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--regex-rule-set" ]]; then
        COMPREPLY=( $(compgen -W "{}" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--render" ]]; then
        COMPREPLY=( $(compgen -W "side-by-side unified context normal html" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--syntax" ]]; then
        COMPREPLY=( $(compgen -W "plain auto rust json html markdown shell toml yaml c cpp python javascript typescript go java css" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--encoding" ]]; then
        COMPREPLY=( $(compgen -W "auto utf8 utf8-bom utf16le utf16be lossy-utf8" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--image-mode" ]]; then
        COMPREPLY=( $(compgen -W "exact tolerance perceptual" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--document-mode" ]]; then
        COMPREPLY=( $(compgen -W "text ocr_text rendered" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--ignore-line-regex" || "$prev" == "--substitute-regex" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--timestamp-tolerance-ms" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--large-file-threshold-bytes" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--large-file-method" ]]; then
        COMPREPLY=( $(compgen -W "quick binary" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--hash-algorithm" ]]; then
        COMPREPLY=( $(compgen -W "blake3 sha256 crc32" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--table-skip-blank" ]]; then
        COMPREPLY=( $(compgen -W "true false" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--symlinks" ]]; then
        COMPREPLY=( $(compgen -W "target follow special" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--format" ]]; then
        COMPREPLY=( $(compgen -W "unified context normal" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--columns" ]]; then
        COMPREPLY=( $(compgen -W "name path state extension left-size right-size left-modified right-modified type method error" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--tree-state" ]]; then
        COMPREPLY=( $(compgen -W "expanded collapsed" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--context" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--preset" ]]; then
        COMPREPLY=( $(compgen -W "{}" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--filter" || "$prev" == "--filter-name" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--state" ]]; then
        COMPREPLY=( $(compgen -W "all differences identical different left-only right-only errors skipped aborted" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--auto-resolve" ]]; then
        COMPREPLY=( $(compgen -W "left right base" -- "$cur") )
        return 0
    fi

    case "$cmd" in
        archive) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        cache) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        compare) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        compare3) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        conflict) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        completions) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        filter) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        folders) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        hex) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        launch) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        man|manpage) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        mergetool) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        plugin) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        profile) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        project) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        report) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        open-external) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        patch) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        reveal) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        self-compare) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        session) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        table) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        webpage) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
    esac
}}
complete -F _linsync_cli linsync-cli
"#,
        CLI_COMMANDS.join(" "),
        builtin_text_regex_rule_sets()
            .into_iter()
            .map(|rule_set| rule_set.id)
            .collect::<Vec<_>>()
            .join(" "),
        OPEN_EXTERNAL_PRESETS.join(" "),
        ARCHIVE_FLAGS.join(" "),
        CACHE_FLAGS.join(" "),
        COMPARE_FLAGS.join(" "),
        COMPARE3_FLAGS.join(" "),
        CONFLICT_FLAGS.join(" "),
        COMPLETION_SHELLS.join(" "),
        FILTER_FLAGS.join(" "),
        FOLDER_FLAGS.join(" "),
        HEX_FLAGS.join(" "),
        LAUNCH_FLAGS.join(" "),
        OUTPUT_FLAGS.join(" "),
        MERGETOOL_FLAGS.join(" "),
        PLUGIN_FLAGS.join(" "),
        PROFILE_FLAGS.join(" "),
        PROJECT_FLAGS.join(" "),
        REPORT_FLAGS.join(" "),
        OPEN_EXTERNAL_FLAGS.join(" "),
        PATCH_FLAGS.join(" "),
        REVEAL_FLAGS.join(" "),
        SELF_COMPARE_FLAGS.join(" "),
        SESSION_FLAGS.join(" "),
        TABLE_FLAGS.join(" "),
        WEBPAGE_FLAGS.join(" ")
    )
}

pub(crate) fn zsh_completions() -> String {
    format!(
        r#"#compdef linsync-cli

_linsync_cli() {{
    local -a commands
    commands=(
        {}
    )

    _arguments -C \
        '1:command:->command' \
        '*::arg:->args'

    case $state in
        command)
            _describe 'command' commands
            ;;
        args)
            case $words[2] in
                archive) _values 'archive option' {} ;;
                cache) _values 'cache option' {} ;;
                compare) _values 'compare option' {} ;;
                compare3) _values 'compare3 option' {} ;;
                conflict) _values 'conflict option' {} ;;
                completions) _values 'shell' {} ;;
                filter) _values 'filter option' {} ;;
                folders) _values 'folder option' {} ;;
                hex) _values 'hex option' {} ;;
                launch) _values 'launch option' {} ;;
                man|manpage) _values 'output option' {} ;;
                mergetool) _values 'mergetool option' {} ;;
                plugin) _values 'plugin option' {} ;;
                profile) _values 'profile option' {} ;;
                project) _values 'project option' {} ;;
                report) _values 'report option' {} ;;
                open-external) _values 'open-external option' {} ;;
                patch) _values 'patch option' {} ;;
                reveal) _values 'reveal option' {} ;;
                self-compare) _values 'self-compare option' {} ;;
                session) _values 'session option' {} ;;
                table) _values 'table option' {} ;;
                webpage) _values 'webpage option' {} ;;
            esac
            ;;
    esac
}}

_linsync_cli "$@"
"#,
        CLI_COMMANDS
            .iter()
            .map(|command| format!("'{command}:{command}'"))
            .collect::<Vec<_>>()
            .join("\n        "),
        zsh_values(ARCHIVE_FLAGS),
        zsh_values(CACHE_FLAGS),
        zsh_values(COMPARE_FLAGS),
        zsh_values(COMPARE3_FLAGS),
        zsh_values(CONFLICT_FLAGS),
        zsh_values(COMPLETION_SHELLS),
        zsh_values(FILTER_FLAGS),
        zsh_values(FOLDER_FLAGS),
        zsh_values(HEX_FLAGS),
        zsh_values(LAUNCH_FLAGS),
        zsh_values(OUTPUT_FLAGS),
        zsh_values(MERGETOOL_FLAGS),
        zsh_values(PLUGIN_FLAGS),
        zsh_values(PROFILE_FLAGS),
        zsh_values(PROJECT_FLAGS),
        zsh_values(REPORT_FLAGS),
        zsh_values(OPEN_EXTERNAL_FLAGS),
        zsh_values(PATCH_FLAGS),
        zsh_values(REVEAL_FLAGS),
        zsh_values(SELF_COMPARE_FLAGS),
        zsh_values(SESSION_FLAGS),
        zsh_values(TABLE_FLAGS),
        zsh_values(WEBPAGE_FLAGS)
    )
}

pub(crate) fn zsh_values(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn fish_completions() -> String {
    let mut output = String::new();
    for command in CLI_COMMANDS {
        output.push_str(&format!("complete -c linsync-cli -f -a {command}\n"));
    }
    for flag in COMPARE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from compare' {}\n",
            fish_option(flag)
        ));
    }
    for flag in FOLDER_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from folders' {}\n",
            fish_option(flag)
        ));
    }
    for flag in COMPARE3_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from compare3' {}\n",
            fish_option(flag)
        ));
    }
    for flag in CONFLICT_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from conflict' {}\n",
            fish_option(flag)
        ));
    }
    for flag in HEX_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from hex' {}\n",
            fish_option(flag)
        ));
    }
    for flag in PATCH_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from patch' {}\n",
            fish_option(flag)
        ));
    }
    for flag in REPORT_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from report' {}\n",
            fish_option(flag)
        ));
    }
    for flag in SELF_COMPARE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from self-compare' {}\n",
            fish_option(flag)
        ));
    }
    for flag in TABLE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from table' {}\n",
            fish_option(flag)
        ));
    }
    for flag in LAUNCH_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from launch' {}\n",
            fish_option(flag)
        ));
    }
    for flag in OPEN_EXTERNAL_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from open-external' {}\n",
            fish_option(flag)
        ));
    }
    for flag in REVEAL_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from reveal' {}\n",
            fish_option(flag)
        ));
    }
    for flag in MERGETOOL_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from mergetool' {}\n",
            fish_option(flag)
        ));
    }
    for flag in ARCHIVE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from archive' {}\n",
            fish_option(flag)
        ));
    }
    for flag in CACHE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from cache' {}\n",
            fish_option(flag)
        ));
    }
    for flag in FILTER_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from filter' {}\n",
            fish_option(flag)
        ));
    }
    for flag in PLUGIN_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from plugin' {}\n",
            fish_option(flag)
        ));
    }
    for flag in PROFILE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from profile' {}\n",
            fish_option(flag)
        ));
    }
    for flag in PROJECT_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from project' {}\n",
            fish_option(flag)
        ));
    }
    for flag in SESSION_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from session' {}\n",
            fish_option(flag)
        ));
    }
    for flag in WEBPAGE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from webpage' {}\n",
            fish_option(flag)
        ));
    }
    output.push_str(
        "complete -c linsync-cli -n '__fish_seen_subcommand_from completions' -a 'bash zsh fish'\n",
    );
    output
}

pub(crate) fn fish_option(flag: &str) -> String {
    if let Some(long) = flag.strip_prefix("--") {
        format!("-l {long}")
    } else if let Some(short) = flag.strip_prefix('-') {
        format!("-s {short}")
    } else {
        format!("-a {flag}")
    }
}
