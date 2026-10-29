import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Agent, Asset, AssetKind, CopyRequest, DiffLine, FileState, Health, JournalEntry, LoadStack, SearchHit } from "./types";

export const inApp = isTauri;

export const scanAgents = (projects: string[]) => invoke<Agent[]>("scan_agents", { projects });
export const loadOrder = (project: string) => invoke<LoadStack[]>("load_order", { project });
export const search = (query: string) => invoke<SearchHit[]>("search", { query });
export const readAsset = (path: string) => invoke<string>("read_asset", { path });

/** Calls `fn` whenever a watched agent file changes on disk. Returns an unsubscribe. */
export function onAgentsChanged(fn: () => void): () => void {
  const pending = listen("agents-changed", fn);
  return () => {
    pending.then((un) => un());
  };
}

const PROJECTS_KEY = "skillz.projects";

/** Extra project folders the user added, kept per machine. */
export function savedProjects(): string[] {
  try {
    return JSON.parse(localStorage.getItem(PROJECTS_KEY) ?? "[]");
  } catch {
    return [];
  }
}

export function saveProjects(projects: string[]) {
  try {
    localStorage.setItem(PROJECTS_KEY, JSON.stringify(projects));
  } catch {
    // Storage can be unavailable; the list then lasts for this session only.
  }
}

// Phase 2: editing. Every write is backed up and recorded so it can be undone.
export const readFile = (path: string) => invoke<FileState>("read_file", { path });
export const diffText = (old: string, next: string) => invoke<DiffLine[]>("diff_text", { old, new: next });
export const saveFile = (path: string, content: string, hash: string | null) => invoke<JournalEntry>("save_file", { path, content, hash });
export const deletePath = (path: string, hash: string | null) => invoke<JournalEntry>("delete_path", { path, hash });
export const createAsset = (agentId: string, kind: AssetKind, project: string | null, name: string, description: string) =>
  invoke<JournalEntry>("create_asset", { agentId, kind, project, name, description });
export const saveMcp = (file: string, project: string | null, name: string, config: unknown, renameFrom: string | null) =>
  invoke<JournalEntry>("save_mcp", { file, project, name, config, renameFrom });
export const deleteMcp = (file: string, project: string | null, name: string) => invoke<JournalEntry>("delete_mcp", { file, project, name });
export const setEnabled = (agentId: string, kind: AssetKind, path: string, name: string, enabled: boolean) =>
  invoke<JournalEntry>("set_enabled", { agentId, kind, path, name, enabled });
export const renameSkill = (path: string, newName: string) => invoke<JournalEntry[]>("rename_skill", { path, newName });
export const history = () => invoke<JournalEntry[]>("history");
export const undo = (id: string) => invoke<JournalEntry>("undo", { id });

// Phase 3: making agents well rounded.
export const copyAsset = (request: CopyRequest) => invoke<JournalEntry>("copy_asset", { request });
export const copyTarget = (toAgent: string, kind: AssetKind, project: string | null, name: string) => invoke<string | null>("copy_target", { toAgent, kind, project, name });
export const health = () => invoke<Health[]>("health");

/** Copies `asset` from `from` to agent `toAgent`, at the same project when the target supports it. */
export async function copyTo(from: Agent, asset: Asset, toAgent: string): Promise<JournalEntry> {
  const project = asset.scope.type === "project" ? asset.scope.path : null;
  const toProject = await copyTarget(toAgent, asset.kind, project, asset.name);
  return copyAsset({ fromAgent: from.id, kind: asset.kind, name: asset.name, path: asset.path, project, toAgent, toProject });
}
