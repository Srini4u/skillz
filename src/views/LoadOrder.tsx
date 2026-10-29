import { useEffect, useState } from "react";
import { copyAsset, createAsset, loadOrder } from "../api";
import { Agent, JournalEntry, LoadStack, OpenItem, agentLabel, fileName, formatBytes } from "../types";

/** For one repo: the files each agent reads into context at startup, top to bottom. */
export default function LoadOrder({ agents, projects, onOpen, onChanged }: { agents: Agent[]; projects: string[]; onOpen: (i: OpenItem) => void; onChanged: (e: JournalEntry[]) => void }) {
  const [project, setProject] = useState(projects[0] ?? "");
  const [stacks, setStacks] = useState<LoadStack[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Project instructions, including ones inherited from a parent folder of the repo.
  const inProject = (s: LoadStack) => s.files.filter((f) => f.kind === "instruction" && f.scope.type === "project");
  const fileFor = (id: string) => (id === "codex" ? "AGENTS.md" : "CLAUDE.md");

  /** Gives `target` a project instruction file: a copy of another agent's, or a blank one. */
  const scaffold = async (target: string, donor: LoadStack | undefined) => {
    setError(null);
    try {
      const src = donor && inProject(donor).at(-1);
      const e = src
        ? await copyAsset({ fromAgent: donor.agent_id, kind: "instruction", name: fileName(src.path), path: src.path, project, toAgent: target, toProject: project })
        : await createAsset(target, "instruction", project, target === "codex" ? "AGENTS" : "CLAUDE", "");
      onChanged([e]);
    } catch (err) {
      setError(String(err));
    }
  };

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
      {error && <p className="error">{error}</p>}
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
              {inProject(s).length === 0 &&
                (() => {
                  const donor = stacks.find((o) => o.agent_id !== s.agent_id && inProject(o).length > 0);
                  return (
                    <div className="scaffold">
                      <p className="warn">No {fileFor(s.agent_id)} in this project, so it doesn't know this repo's rules.</p>
                      <button onClick={() => scaffold(s.agent_id, donor)}>
                        {donor ? `Create ${fileFor(s.agent_id)} from ${agentLabel(donor.agent_id)}'s ${fileName(inProject(donor).at(-1)!.path)}` : `Create ${fileFor(s.agent_id)}`}
                      </button>
                    </div>
                  );
                })()}
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
