# Git Integration

LinSync can be used from Git today through `linsync-cli`. The CLI is well suited
for scripted checks and terminal reports, and `linsync-cli mergetool` (without
`--auto-resolve`) launches the GUI's three-pane Merge workspace for interactive
conflict resolution with result writing back to `$MERGED`.

## Diff Tool

Use `linsync-cli compare` as a text difftool:

```sh
git config --global difftool.linsync.cmd 'linsync-cli compare "$LOCAL" "$REMOTE"'
git config --global difftool.linsync.trustExitCode false
```

Run it for the whole worktree or a selected path:

```sh
git difftool --tool linsync
git difftool --tool linsync -- src/main.rs
```

`linsync-cli compare` exits with `1` when differences are found. That is useful
for scripts, but it is not an execution failure. Keeping
`difftool.linsync.trustExitCode` false lets Git continue through changed files.

For script-friendly output outside `git difftool`, use JSON, count, or quiet
mode:

```sh
linsync-cli compare --json old.rs new.rs
linsync-cli compare --count old.rs new.rs
linsync-cli compare --quiet old.rs new.rs
```

## Merge Tool

LinSync ships a `mergetool` subcommand that writes a resolved `$MERGED` file,
making `linsync-cli` a fully functional Git mergetool.

### Opt-in setup

The package installs a ready-made Git config snippet at
`/usr/share/linsync/git-mergetool.gitconfig`. Include it globally and set it
as the default mergetool:

```sh
git config --global include.path /usr/share/linsync/git-mergetool.gitconfig
git config --global merge.tool linsync
```

After that, any `git merge` conflict is resolved with:

```sh
git mergetool
```

Git calls `linsync-cli mergetool` for each conflicted file, passing the four
standard paths through `$BASE`, `$LOCAL`, `$REMOTE`, and `$MERGED`.

### Auto-resolve flag

For scripted or CI use cases, pass `--auto-resolve` to pick a side without
launching any UI:

```sh
# Accept all local (ours) changes
linsync-cli mergetool \
  --base "$BASE" --local "$LOCAL" --remote "$REMOTE" --merged "$MERGED" \
  --auto-resolve left

# Accept all remote (theirs) changes
linsync-cli mergetool \
  --base "$BASE" --local "$LOCAL" --remote "$REMOTE" --merged "$MERGED" \
  --auto-resolve right

# Revert all conflicting hunks to the common ancestor
linsync-cli mergetool \
  --base "$BASE" --local "$LOCAL" --remote "$REMOTE" --merged "$MERGED" \
  --auto-resolve base
```

Valid choices for `--auto-resolve` are `left`, `right`, and `base`.

### Interactive GUI mode

Running `mergetool` without `--auto-resolve` launches the GUI's three-pane
Merge workspace on the base/local/remote inputs and writes the resolved output
to `$MERGED` on save. `linsync-cli` waits for the GUI to exit and then verifies
that a fully-resolved file (no remaining conflict markers) was written.

### Manual conflict inspection

For read-only inspection without writing the merge result, the existing
commands remain available:

```sh
linsync-cli compare3 "$LOCAL" "$BASE" "$REMOTE"
linsync-cli compare3 --markers "$LOCAL" "$BASE" "$REMOTE" > conflict-preview.txt
linsync-cli conflict path/to/conflicted-file
git diff --name-only --diff-filter=U | xargs -r -n1 linsync-cli conflict
```

## Exit Codes

`linsync-cli` uses these exit codes:

- `0`: no differences, or generator command completed successfully.
- `1`: differences or conflicts were found.
- `2`: command error, invalid arguments, unreadable files, or unsupported input.

Use `--quiet` when only the exit code matters:

```sh
if linsync-cli compare --quiet old.rs new.rs; then
    echo "no differences"
else
    case "$?" in
        1) echo "differences found" ;;
        2) echo "comparison failed" ;;
    esac
fi
```
