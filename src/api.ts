import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Agent, LoadStack, SearchHit } from "./types";

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
