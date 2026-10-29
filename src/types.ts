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
  if (scope.type === "project") return scope.path.split("/").filter(Boolean).pop() ?? scope.path;
  return scope.type;
}
