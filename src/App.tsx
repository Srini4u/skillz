import { useCallback, useEffect, useMemo, useState } from "react";
import { inApp, onAgentsChanged, saveProjects, savedProjects, scanAgents, search, undo } from "./api";
import Inspector from "./components/Inspector";
import NewAsset from "./components/NewAsset";
import History from "./views/History";
import AgentMap from "./views/AgentMap";
import Coverage from "./views/Coverage";
import LoadOrder from "./views/LoadOrder";
import Overview from "./views/Overview";
import SearchResults from "./views/SearchResults";
import { Agent, JournalEntry, KIND_LABELS, KIND_ORDER, OpenItem, SearchHit, openAsset } from "./types";

type Tab = "overview" | "map" | "coverage" | "load" | "history";
const TABS: { id: Tab; label: string }[] = [
  { id: "map", label: "Map" },
  { id: "overview", label: "List" },
  { id: "coverage", label: "Coverage" },
  { id: "load", label: "Load order" },
  { id: "history", label: "History" },
];

export default function App() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("map");
  const [open, setOpen] = useState<OpenItem | null>(null);
  const [extraProjects, setExtraProjects] = useState<string[]>(savedProjects);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [changedAt, setChangedAt] = useState<Date | null>(null);
  const [creating, setCreating] = useState(false);
  const [pendingPath, setPendingPath] = useState<string | null>(null);
  const [toast, setToast] = useState<{ text: string; entries: JournalEntry[] } | null>(null);
  const [version, setVersion] = useState(0);

  const refresh = useCallback(async () => {
    if (!inApp()) {
      setError("Open Skillz with `npm run tauri dev` to scan this machine.");
      return;
    }
    setLoading(true);
    try {
      const found = await scanAgents(extraProjects);
      setAgents(found);
      setSelected((s) => s ?? found.find((a) => a.installed)?.id ?? found[0]?.id ?? null);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [extraProjects]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!inApp()) return;
    return onAgentsChanged(() => {
      setChangedAt(new Date());
      refresh();
    });
  }, [refresh]);

  useEffect(() => {
    const q = query.trim();
    if (!q || !inApp()) {
      setHits([]);
      return;
    }
    const t = setTimeout(() => search(q).then(setHits).catch(() => setHits([])), 200);
    return () => clearTimeout(t);
  }, [query, agents]);

  // After a rescan, swap the open item for its fresh copy so the inspector re-reads the
  // file and shows the new enabled flag or MCP config; open something just created once
  // the scan has picked it up.
  useEffect(() => {
    if (pendingPath) {
      for (const a of agents) {
        const found = a.assets.find((x) => x.path === pendingPath);
        if (found) {
          setOpen(openAsset(a, found));
          setPendingPath(null);
          return;
        }
      }
      return;
    }
    setOpen((o) => {
      if (!o) return o;
      for (const a of agents) {
        const found = a.assets.find((x) => x.path === o.path && x.kind === o.kind && x.name === o.name);
        if (found) return { ...openAsset(a, found), line: o.line };
      }
      return o.kind === "mcp_server" ? o : { ...o };
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agents]);

  const changed = (entries: JournalEntry[]) => {
    if (entries.length === 0) return;
    setToast({ text: entries[entries.length - 1].summary, entries });
    setVersion((v) => v + 1);
    refresh();
  };

  const undoToast = async () => {
    if (!toast) return;
    const done: JournalEntry[] = [];
    for (const e of [...toast.entries].reverse()) done.push(await undo(e.id));
    setToast({ text: "Undone", entries: [] });
    setVersion((v) => v + 1);
    refresh();
  };

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 8000);
    return () => clearTimeout(t);
  }, [toast]);

  const projects = useMemo(() => [...new Set([...extraProjects, ...agents.flatMap((a) => a.known_projects)])].sort(), [agents, extraProjects]);

  const addProject = (path: string) => {
    const p = path.trim().replace(/\/+$/, "");
    if (!p || extraProjects.includes(p)) return;
    const next = [...extraProjects, p];
    setExtraProjects(next);
    saveProjects(next);
  };

  const agent = agents.find((a) => a.id === selected);
  const searching = query.trim().length > 0;

  return (
    <div className="app">
      <header>
        <h1>Skillz</h1>
        <input className="search" type="search" placeholder="Search skills, memory, instructions, MCP servers…" value={query} onChange={(e) => setQuery(e.target.value)} />
        <span className="muted live">{changedAt ? `Updated from disk ${changedAt.toLocaleTimeString()}` : "Watching for changes"}</span>
        <button onClick={refresh} disabled={loading}>
          {loading ? "Scanning…" : "Rescan"}
        </button>
      </header>
      {error && <p className="error">{error}</p>}

      {!searching && (
        <nav className="tabs">
          {TABS.map((t) => (
            <button key={t.id} className={tab === t.id ? "active" : ""} onClick={() => setTab(t.id)}>
              {t.label}
            </button>
          ))}
        </nav>
      )}

      {!searching && (tab === "map" || tab === "overview") && (
        <section className="cards">
          {agents.map((a) => (
            <AgentCard key={a.id} agent={a} active={a.id === selected} onClick={() => setSelected(a.id)} />
          ))}
        </section>
      )}

      <main className="body">
        <div className="pane">
          {searching ? (
            <SearchResults query={query.trim()} hits={hits} agents={agents} onOpen={setOpen} />
          ) : tab === "map" ? (
            agent && <AgentMap agent={agent} onOpen={setOpen} />
          ) : tab === "overview" ? (
            agent && (
              <>
                {creating ? (
                  <NewAsset
                    agent={agent}
                    projects={projects}
                    onCancel={() => setCreating(false)}
                    onCreated={(e) => {
                      setCreating(false);
                      setPendingPath(e.path);
                      changed([e]);
                    }}
                  />
                ) : (
                  <button className="new-btn" onClick={() => setCreating(true)} disabled={!agent.installed}>
                    + New skill, instruction, memory, MCP server…
                  </button>
                )}
                <Overview agent={agent} open={open} onOpen={setOpen} />
              </>
            )
          ) : tab === "history" ? (
            <History version={version} onChanged={changed} />
          ) : tab === "coverage" ? (
            <Coverage agents={agents} onOpen={setOpen} />
          ) : (
            <>
              <AddProject onAdd={addProject} />
              <LoadOrder agents={agents} projects={projects} onOpen={setOpen} />
            </>
          )}
        </div>
        <Inspector item={open} onClose={() => setOpen(null)} onChanged={changed} />
      </main>
      {toast && (
        <div className="toast" role="status">
          {toast.text}
          {toast.entries.length > 0 && <button onClick={undoToast}>Undo</button>}
        </div>
      )}
    </div>
  );
}

function AgentCard({ agent, active, onClick }: { agent: Agent; active: boolean; onClick: () => void }) {
  return (
    <button className={`card ${active ? "active" : ""} ${agent.installed ? "" : "missing"}`} onClick={onClick}>
      <div className="card-title">{agent.name}</div>
      <div className="muted">{agent.installed ? (agent.version ?? "installed") : "not found"}</div>
      <ul className="counts">
        {KIND_ORDER.map((k) => {
          const n = agent.assets.filter((a) => a.kind === k).length;
          return n > 0 ? (
            <li key={k}>
              <strong>{n}</strong> {KIND_LABELS[k]}
            </li>
          ) : null;
        })}
      </ul>
      {agent.warnings.length > 0 && <div className="warn">{agent.warnings.length} file(s) could not be read</div>}
    </button>
  );
}

function AddProject({ onAdd }: { onAdd: (path: string) => void }) {
  const [path, setPath] = useState("");
  return (
    <form
      className="add-project"
      onSubmit={(e) => {
        e.preventDefault();
        onAdd(path);
        setPath("");
      }}
    >
      <input placeholder="/home/you/code/my-repo" value={path} onChange={(e) => setPath(e.target.value)} />
      <button type="submit">Add project folder</button>
    </form>
  );
}
