# cairn

English | [简体中文](README.zh-CN.md)

cairn is a work-continuation memory for coding agents running in the terminal. It records where the last session stopped, what has observably changed since, and the suggested next step, so that a new Claude Code or Codex session sees it right at the start. Every normally finished turn also gets one chance to save what the next session needs.

cairn runs on its own and does not depend on Saddle, Corral or Drover. The first version supports Claude Code and Codex, and only takes effect in projects you explicitly adopt.

## Install

Requires Rust stable. Install from source:

```sh
cargo install --path crates/cairn
```

Before wiring up an agent, preview the changes:

```sh
cairn install --agent claude --dry-run
cairn install --agent codex --dry-run
```

Then install:

```sh
cairn install --agent claude --yes
cairn install --agent codex --yes
```

Installing creates a stable command path `${XDG_DATA_HOME:-$HOME/.local/share}/cairn/bin/cairn` and updates the agent configuration. For Claude it updates `~/.claude/settings.json` by default (or the directory given by `CLAUDE_CONFIG_DIR`); for Codex it updates `~/.codex/hooks.json` by default (or the directory given by `CODEX_HOME`). Existing files are backed up first, and only cairn's own entries are added. After installing for Codex, open `/hooks` in Codex and review and trust the cairn hooks; trusting the project directory or running with `--yolo` does not replace this step.

## Adopt a project

In a Git project where you want cairn enabled:

```sh
cairn adopt
```

To stop:

```sh
cairn unadopt
```

Only adopted projects receive and inject records; everything else is left alone.

## Everyday commands

- `cairn show`: show what would be injected into the agent for the current work line.
- `cairn list [--json]`: list the visible records of the current project (`--line` for the current work line).
- `cairn show <ID>`: show one record in full.
- `cairn correct <ID>`: append a correction to a record; the body is read from stdin.
- `cairn retract <ID>`: append a retraction so the record is hidden by default.
- `cairn restore <ID>`: undo a supersession or retraction of a record.
- `cairn delete <ID>`: erase a record's body and keep only a tombstone; add `--yes` to skip the prompt.
- `cairn export`: export the current work line's records to stdout, or to a new file path.
- `cairn status`: show agent installation, project adoption, spool status and when each agent's hooks last fired in this project.

Agents save records by passing the body to `cairn save` on stdin, for example:

```sh
printf '%s\n' '## 停点' 'Finished one milestone.' | cairn save
```

The record body uses fixed Markdown section headings (DESIGN §6.3); `## 停点` ("stopping point") is required.

## Data and disabling

The main database lives at `${XDG_STATE_HOME:-$HOME/.local/state}/cairn/cairn.db`. The agent's restricted save entry first writes to `cairn-spool/` in the user's private temporary directory, and hooks then ingest it into the database; `cairn status` reports the spool path and any files not yet ingested.

Set `CAIRN_DISABLE=1` to temporarily disable the cairn hooks: they let the agent proceed, inject nothing, and do not save for that turn.

## Uninstall and delete data

Remove an agent's integration:

```sh
cairn uninstall --agent claude --yes
cairn uninstall --agent codex --yes
```

Uninstalling does not delete the database. Once you no longer need the data, remove the state directory:

```sh
rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/cairn"
```

If there are spool files that were never ingested, run `cairn status` first and delete the `cairn-spool` contents at the path it reports.

Records are history with provenance, not current instructions or authorization. Check the actual state before acting on them; when they conflict with what the user asks in the current turn, the user wins.

## Isolated walkthrough from source

The commands below install cairn into a fresh temporary environment and run the basic flow without touching your real user configuration. Note: the spool lives in the system's per-user temporary directory (`getconf DARWIN_USER_TEMP_DIR`), which `TMPDIR` does not redirect; after the walkthrough an empty directory `cairn-spool/<hash>/` named after the database path is left there, and you can delete it at the path `cairn status` reports.

```sh
test_root="$(mktemp -d /tmp/cairn-readme.XXXXXX)"
export HOME="$test_root/home"
export XDG_STATE_HOME="$test_root/state"
export XDG_DATA_HOME="$test_root/data"
export CODEX_HOME="$test_root/codex"
export CLAUDE_CONFIG_DIR="$test_root/claude"
export TMPDIR="$test_root/tmp"
mkdir -p "$HOME" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$CODEX_HOME" "$CLAUDE_CONFIG_DIR" "$TMPDIR"

cargo install --path crates/cairn --root "$test_root/cargo"
export PATH="$test_root/cargo/bin:$PATH"

cairn install --agent claude --dry-run
cairn install --agent codex --dry-run
cairn install --agent claude --yes
cairn install --agent codex --yes

mkdir "$test_root/project"
cd "$test_root/project"
git init
cairn adopt
printf '%s\n' '## 停点' 'Isolated walkthrough example.' | cairn save
cairn show
cairn list
cairn status

cairn uninstall --agent claude --yes
cairn uninstall --agent codex --yes
```

## Status and documentation

Development of the core, hook adapters and installer is complete; the end-to-end trial with real agents has not been run yet. The authoritative design is [docs/DESIGN.md](docs/DESIGN.md) (Chinese); the implementation plan, decision record, capability test report and per-task review records are under [docs/](docs/).
