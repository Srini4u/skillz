# Skillz

See and customize every AI coding agent on your machine (Claude Code, Codex CLI, and more) from one desktop app: their skills, instructions, memory, MCP servers and settings.

Skillz reads and writes the agents' own files (`~/.claude`, `~/.claude.json`, `~/.codex`, `~/.agents`, and `CLAUDE.md` / `AGENTS.md` / `.mcp.json` in your repos). It keeps no hidden copy, needs no AI or API key, and works offline.

**Status:** Phase 1. The app scans Claude Code and Codex CLI and shows what each has, read-only, in four views:

- **Map:** each agent in the middle with its instructions, memory, skills, MCP servers, subagents, commands and settings around it, coloured by scope. Click a dot to open its file.
- **List:** the same, grouped by kind.
- **Coverage:** a heatmap of every skill, MCP server, subagent and command against each agent, so gaps stand out.
- **Load order:** for a repo, the instruction and memory files each agent reads at startup, in order, with sizes.

Search covers names, descriptions and file contents. The app watches the agents' folders and refreshes when a file changes. Editing comes in Phase 2.

## What it finds

| | Claude Code | Codex CLI |
| --- | --- | --- |
| Instructions | `~/.claude/CLAUDE.md`, `CLAUDE.md`, `.claude/CLAUDE.md`, `CLAUDE.local.md` | `~/.codex/AGENTS.md`, `AGENTS.md`, `AGENTS.override.md` |
| Memory | `~/.claude/projects/<project>/memory/*.md` | none |
| Skills | `~/.claude/skills/*/SKILL.md`, `.claude/skills/` | `~/.agents/skills/`, `~/.codex/skills/`, `/etc/codex/skills/`, `.agents/skills/`, `.codex/skills/` |
| MCP servers | `~/.claude.json` (user and per project), `.mcp.json` | `[mcp_servers.*]` in `~/.codex/config.toml` |
| Subagents, commands | `agents/*.md`, `commands/**/*.md` | `~/.codex/prompts/*.md` |
| Settings | `settings.json`, `settings.local.json` | `~/.codex/config.toml` |

Projects come from the agents' own records (`projects` in `~/.claude.json` and `~/.codex/config.toml`). Secret values in MCP `env` and auth headers are masked.

## Run it on Ubuntu

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential
# Rust: https://rustup.rs   Node 22+
npm install
npm run tauri dev        # run the app
npx tauri build          # .deb and AppImage in target/release/bundle/
```

## Check what it sees

`skillz-scan` prints everything Skillz finds, so you can verify the paths on your machine:

```sh
npm run scan                      # all agents, user scope + known projects
npm run scan -- ~/code/my-repo    # also scan a specific repo
npm run scan -- --json            # raw data the UI uses
npm run scan -- --load-order ~/code/my-repo   # what each agent loads there
```

## Layout

- `crates/skillz-core`: discovery library, one adapter per agent in `src/adapters/`, plus the `skillz-scan` CLI
- `src-tauri`: the desktop shell; exposes the core to the UI as Tauri commands
- `src`: React + TypeScript UI

## Plan

1. Phase 0 (done): skeleton, Claude Code and Codex discovery, Ubuntu builds
2. Phase 1 (this): agent map, coverage and load-order views, search, file watching
3. Phase 2: create, edit and delete skills, memory, instructions, MCP servers and settings, with diff, backup and undo
4. Phase 3: capability matrix and copying skills and MCP servers between agents
