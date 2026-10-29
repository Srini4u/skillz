import { useCallback, useEffect, useMemo, useState } from "react";
import { inApp, onAgentsChanged, saveProjects, savedProjects, scanAgents, search } from "./api";
import Viewer from "./components/Viewer";
import AgentMap from "./views/AgentMap";
import Coverage from "./views/Coverage";
import LoadOrder from "./views/LoadOrder";
import Overview from "./views/Overview";
import SearchResults from "./views/SearchResults";
import { Agent, KIND_LABELS, KIND_ORDER, OpenItem, SearchHit, openAsset } from "./types";

type Tab = "overview" | "map" | "coverage" | "load";
const TABS: { id: Tab; label: string }[] = [
  { id: "map", label: "Map" },
  { id: "overview", label: "List" },
  { id: "coverage", label: "Coverage" },
  { id: "load", label: "Load order" },
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

  // After a rescan, swap the open item for its fresh copy so the viewer re-reads the file
  // and an MCP server shows its new config.
  useEffect(() => {
    setOpen((o) => {
      if (!o) return o;
      for (const a of agents) {
        const found = a.assets.find((x) => x.path === o.path && x.kind === o.kind && x.name === o.name);
        if (found) return { ...openAsset(a, found), line: o.line };
      }
      return o.kind === "mcp_server" ? o : { ...o };
    });
  }, [agents]);

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
            agent && <Overview agent={agent} open={open} onOpen={setOpen} />
          ) : tab === "coverage" ? (
            <Coverage agents={agents} onOpen={setOpen} />
          ) : (
            <>
              <AddProject onAdd={addProject} />
              <LoadOrder agents={agents} projects={projects} onOpen={setOpen} />
            </>
          )}
        </div>
        <Viewer item={open} onClose={() => setOpen(null)} />
      </main>
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
