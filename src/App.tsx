import { useCallback, useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Agent, Asset, KIND_LABELS, KIND_ORDER, scopeLabel } from "./types";

export default function App() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!isTauri()) {
      setError("Open Skillz with `npm run tauri dev` to scan this machine.");
      return;
    }
    setLoading(true);
    try {
      const found = await invoke<Agent[]>("scan_agents", { projects: [] });
      setAgents(found);
      setSelected((s) => s ?? found.find((a) => a.installed)?.id ?? found[0]?.id ?? null);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const agent = agents.find((a) => a.id === selected);

  return (
    <div className="app">
      <header>
        <h1>Skillz</h1>
        <button onClick={refresh} disabled={loading}>
          {loading ? "Scanning…" : "Rescan"}
        </button>
      </header>
      {error && <p className="error">{error}</p>}
      <section className="cards">
        {agents.map((a) => (
          <AgentCard key={a.id} agent={a} active={a.id === selected} onClick={() => setSelected(a.id)} />
        ))}
      </section>
      {agent && <AgentDetail agent={agent} />}
    </div>
  );
}

function AgentCard({ agent, active, onClick }: { agent: Agent; active: boolean; onClick: () => void }) {
  return (
    <button className={`card ${active ? "active" : ""} ${agent.installed ? "" : "missing"}`} onClick={onClick}>
      <div className="card-title">{agent.name}</div>
      <div className="muted">{agent.installed ? agent.version ?? "installed" : "not found"}</div>
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

function AgentDetail({ agent }: { agent: Agent }) {
  const [open, setOpen] = useState<Asset | null>(null);
  const [content, setContent] = useState("");

  useEffect(() => setOpen(null), [agent.id]);

  const view = async (asset: Asset) => {
    setOpen(asset);
    try {
      setContent(await invoke<string>("read_asset", { path: asset.path }));
    } catch (e) {
      setContent(String(e));
    }
  };

  return (
    <section className="detail">
      <div className="list">
        {KIND_ORDER.map((kind) => {
          const items = agent.assets.filter((a) => a.kind === kind);
          if (items.length === 0) return null;
          return (
            <div key={kind}>
              <h3>{KIND_LABELS[kind]}</h3>
              {items.map((a) => (
                <button
                  key={`${a.path}#${a.name}`}
                  className={`item ${open === a ? "active" : ""} ${a.enabled ? "" : "disabled"}`}
                  onClick={() => view(a)}
                  title={a.path}
                >
                  <span>{a.name}</span>
                  <span className="tag">{scopeLabel(a.scope)}</span>
                </button>
              ))}
            </div>
          );
        })}
        {agent.assets.length === 0 && <p className="muted">Nothing found for {agent.name}.</p>}
        {agent.warnings.map((w) => (
          <p key={w} className="warn">
            {w}
          </p>
        ))}
      </div>
      <div className="viewer">
        {open ? (
          <>
            <div className="viewer-head">
              <strong>{open.name}</strong>
              {open.description && <span className="muted"> · {open.description}</span>}
              <div className="muted path">{open.path}</div>
            </div>
            {open.kind === "mcp_server" ? (
              <pre>{JSON.stringify(open.details, null, 2)}</pre>
            ) : (
              <pre>{content}</pre>
            )}
          </>
        ) : (
          <p className="muted">Pick an item to view it.</p>
        )}
      </div>
    </section>
  );
}
