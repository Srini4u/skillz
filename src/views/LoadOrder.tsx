import { useEffect, useState } from "react";
import { loadOrder } from "../api";
import { Agent, LoadStack, OpenItem, fileName, formatBytes } from "../types";

/** For one repo: the files each agent reads into context at startup, top to bottom. */
export default function LoadOrder({ agents, projects, onOpen }: { agents: Agent[]; projects: string[]; onOpen: (i: OpenItem) => void }) {
  const [project, setProject] = useState(projects[0] ?? "");
  const [stacks, setStacks] = useState<LoadStack[]>([]);

  useEffect(() => {
    if (!project && projects[0]) setProject(projects[0]);
  }, [projects, project]);

  useEffect(() => {
    if (!project) return;
    loadOrder(project).then(setStacks).catch(() => setStacks([]));
  }, [project, agents]);

  if (projects.length === 0) return <p className="muted">No projects yet. Add a project folder above, or open one in Claude Code or Codex.</p>;

  const max = Math.max(1, ...stacks.flatMap((s) => s.files.map((f) => f.bytes)));
  return (
    <div className="load">
      <label className="picker">
        Project{" "}
        <select value={project} onChange={(e) => setProject(e.target.value)}>
          {projects.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
      </label>
      <div className="stacks">
        {stacks.map((s) => {
          const agent = agents.find((a) => a.id === s.agent_id);
          const total = s.files.reduce((n, f) => n + f.bytes, 0);
          return (
            <div key={s.agent_id} className="stack">
              <h3>{agent?.name ?? s.agent_id}</h3>
              <p className="muted">
                {s.files.length} files · {formatBytes(total)} · about {Math.round(total / 4).toLocaleString()} tokens
              </p>
              {s.files.length === 0 && <p className="warn">Loads no instructions in this project.</p>}
              <ol>
                {s.files.map((f) => (
                  <li key={f.path}>
                    <button
                      className="layer"
                      title={f.path}
                      onClick={() => onOpen({ agentId: s.agent_id, agentName: agent?.name ?? s.agent_id, scope: f.scope, name: fileName(f.path), kind: f.kind, path: f.path })}
                    >
                      <span className="bar" style={{ width: `${Math.max(4, (f.bytes / max) * 100)}%` }} />
                      <span className="layer-text">
                        <strong>{fileName(f.path)}</strong> <span className="muted">{f.reason}</span>
                        <span className="tag">{formatBytes(f.bytes)}</span>
                      </span>
                    </button>
                  </li>
                ))}
              </ol>
            </div>
          );
        })}
      </div>
    </div>
  );
}
