# Skillz

See and customize every AI coding agent on your machine (Claude Code, Codex CLI, and more) from one desktop app: their skills, instructions, memory, MCP servers and settings.

Skillz reads and writes the agents' own files (`~/.claude`, `~/.claude.json`, `~/.codex`, `~/.agents`, and `CLAUDE.md` / `AGENTS.md` / `.mcp.json` in your repos). It keeps no hidden copy, needs no AI or API key, and works offline.

**Status:** Phase 3. The app scans Claude Code and Codex CLI, shows what each has, lets you change it, and helps you bring both up to the same level.

## Install (Ubuntu)

Download the AppImage or .deb from [Releases](https://github.com/Srini4u/skillz/releases), then either:

```sh
chmod +x Skillz_*.AppImage && ./Skillz_*.AppImage
# or
sudo apt install ./Skillz_*.deb
```

Every push to `main` also builds both, as the `skillz-linux` artifact of its CI run. To publish a release, push a version tag: `git tag v0.1.0 && git push origin v0.1.0`.

Views:

- **Map:** each agent in the middle with its instructions, memory, skills, MCP servers, subagents, commands and settings around it, coloured by scope. Click a dot to open its file.
- **List:** the same, grouped by kind.
- **Coverage:** a heatmap of every skill, MCP server, subagent and command against each agent, so gaps stand out.
- **Load order:** for a repo, the instruction and memory files each agent reads at startup, in order, with sizes.

- **Health:** a score and checklist per agent (instructions everywhere, skills described, MCP commands installed, nothing the other agent has that this one lacks, and more), with a one-click fix where one is safe.
- **History:** every change Skillz made, with Undo.

Search covers names, descriptions and file contents. The app watches the agents' folders and refreshes when a file changes.

## Editing

Open anything and press **Edit**. You always see a diff before anything is written.

- **Files** (skills, CLAUDE.md / AGENTS.md, memory, subagents, commands, settings) are edited as text. JSON and TOML that would not parse are refused.
- **MCP servers** are edited as their JSON config and written back in the agent's own format: `~/.claude.json` / `.mcp.json` for Claude Code, `[mcp_servers.*]` in `~/.codex/config.toml` for Codex (comments and layout kept). Secrets stay masked; a masked value is kept as it is on disk.
- **New:** skills, instruction files, Claude Code memory files, subagents, commands and MCP servers, at user level or in a project, from a starter template.
- **Rename** a skill (folder and `name:`), **turn off** a Codex skill or MCP server without deleting it, **delete** anything (a skill goes with its whole folder).

## Making agents well rounded

- **Copy between agents:** in Coverage, a missing cell has a *Copy from …* button. Skills are copied with their whole folder; Claude Code commands become Codex prompts and back; MCP servers are translated between `~/.claude.json` / `.mcp.json` and `config.toml` (stdio and HTTP, with the real secret values, never the masked ones). Nothing is overwritten.
- **Project scaffolding:** in Load order, an agent with no instructions for a repo gets *Create AGENTS.md from Claude Code's CLAUDE.md* (or the reverse), or a blank one.
- **Health fixes** reuse the same copies and templates.

Every write is checked against the version you opened (so it never overwrites a change made outside the app), backed up to `~/.local/share/skillz/backups/`, written atomically, and logged in `~/.local/share/skillz/journal.json`. Undo puts the old version back, and an undo can itself be undone. Skillz only writes inside `~/.claude`, `~/.claude.json`, `~/.codex`, `~/.agents` and your project folders.

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
2. Phase 1 (done): agent map, coverage and load-order views, search, file watching
3. Phase 2 (done): create, edit and delete skills, memory, instructions, MCP servers and settings, with diff, backup and undo
4. Phase 3 (this): capability matrix and copying skills and MCP servers between agents
