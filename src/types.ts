// Mirrors crates/skillz-core/src/model.rs.

export type AssetKind = "skill" | "instruction" | "memory" | "mcp_server" | "settings" | "subagent" | "command";

export type Scope = { type: "user" } | { type: "system" } | { type: "project"; path: string };

export interface Asset {
  kind: AssetKind;
  name: string;
  scope: Scope;
  path: string;
  description: string | null;
  enabled: boolean;
  bytes: number | null;
  details?: unknown;
}

export interface Agent {
  id: string;
  name: string;
  installed: boolean;
  binary: string | null;
  version: string | null;
  roots: string[];
  assets: Asset[];
  known_projects: string[];
  warnings: string[];
}

export const KIND_LABELS: Record<AssetKind, string> = {
  instruction: "Instructions",
  memory: "Memory",
  skill: "Skills",
  mcp_server: "MCP servers",
  subagent: "Subagents",
  command: "Commands",
  settings: "Settings",
};

export const KIND_ORDER = Object.keys(KIND_LABELS) as AssetKind[];

export function scopeLabel(scope: Scope): string {
  return scope.type === "project" ? fileName(scope.path) : scope.type;
}

export interface LoadedFile {
  path: string;
  kind: AssetKind;
  scope: Scope;
  bytes: number;
  reason: string;
}

export interface LoadStack {
  agent_id: string;
  files: LoadedFile[];
}

export interface SearchHit {
  agent_id: string;
  asset: number;
  line: number | null;
  snippet: string;
}

/** Anything the inspector can show: an asset, or a file from a load stack. */
export interface OpenItem {
  agentId: string;
  agentName: string;
  name: string;
  kind: AssetKind;
  path: string;
  scope?: Scope;
  enabled?: boolean;
  description?: string | null;
  details?: unknown;
  line?: number | null;
}

export function openAsset(agent: Agent, a: Asset, line?: number | null): OpenItem {
  return { agentId: agent.id, agentName: agent.name, name: a.name, kind: a.kind, path: a.path, scope: a.scope, enabled: a.enabled, description: a.description, details: a.details, line };
}

export interface FileState {
  content: string;
  hash: string;
  exists: boolean;
}

export interface DiffLine {
  tag: "same" | "add" | "remove";
  text: string;
}

export interface JournalEntry {
  id: string;
  time: number;
  action: "create" | "write" | "delete" | "rename" | "undo";
  path: string;
  to: string | null;
  backup: string | null;
  summary: string;
  undone: boolean;
}

/** Kinds each agent can create, and whether they can live at user level and in a project. */
export const CREATABLE: Record<string, { kind: AssetKind; user: boolean; project: boolean }[]> = {
  "claude-code": [
    { kind: "skill", user: true, project: true },
    { kind: "instruction", user: true, project: true },
    { kind: "memory", user: false, project: true },
    { kind: "subagent", user: true, project: true },
    { kind: "command", user: true, project: true },
  ],
  codex: [
    { kind: "skill", user: true, project: true },
    { kind: "instruction", user: true, project: true },
    { kind: "command", user: true, project: false },
  ],
};

/** The `project` argument for an MCP server stored per project inside ~/.claude.json. */
export function mcpProject(item: OpenItem): string | null {
  return item.path.endsWith("/.claude.json") && item.scope?.type === "project" ? item.scope.path : null;
}

export function fileName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

export function formatBytes(n: number): string {
  return n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`;
}
