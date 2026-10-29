import { useState } from "react";
import { createAsset, saveMcp } from "../api";
import { Agent, AssetKind, CREATABLE, JournalEntry, KIND_LABELS } from "../types";

/** Creates a skill, instruction file, memory file, subagent, command or MCP server for one agent. */
export default function NewAsset({ agent, projects, onCreated, onCancel }: { agent: Agent; projects: string[]; onCreated: (e: JournalEntry, kind: AssetKind) => void; onCancel: () => void }) {
  const kinds = CREATABLE[agent.id] ?? [];
  const [kind, setKind] = useState<AssetKind | "mcp_server">(kinds[0]?.kind ?? "skill");
  const [where, setWhere] = useState<string>("user");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [command, setCommand] = useState("");
  const [error, setError] = useState<string | null>(null);

  const rule = kinds.find((k) => k.kind === kind);
  const mcpUser = agent.id === "claude-code" || agent.id === "codex";
  const places = kind === "mcp_server" ? (agent.id === "claude-code" ? ["user", ...projects] : ["user"]) : [...(rule?.user ? ["user"] : []), ...(rule?.project ? projects : [])];
  const place = places.includes(where) ? where : (places[0] ?? "user");
  const needsName = kind !== "instruction";

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    const project = place === "user" ? null : place;
    try {
      if (kind === "mcp_server") {
        const [cmd, ...args] = command.trim().split(/\s+/);
        if (!cmd) return setError("Give the command that starts the server, e.g. npx -y @modelcontextprotocol/server-github");
        const config = cmd.startsWith("http") ? { type: "http", url: cmd } : { command: cmd, args };
        const home = agent.roots.find((r) => r.endsWith(".claude.json") || r.endsWith(".codex"));
        const file = agent.id === "codex" ? `${home}/config.toml` : project ? `${project}/.mcp.json` : home!;
        onCreated(await saveMcp(file, null, name, config, null, null), "mcp_server");
      } else {
        onCreated(await createAsset(agent.id, kind, project, needsName ? name : kind === "instruction" ? (agent.id === "codex" ? "AGENTS" : "CLAUDE") : name, description), kind);
      }
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <form className="new-asset" onSubmit={submit}>
      <h3>New for {agent.name}</h3>
      <div className="row">
        <label>
          Kind{" "}
          <select value={kind} onChange={(e) => setKind(e.target.value as AssetKind)}>
            {kinds.map((k) => (
              <option key={k.kind} value={k.kind}>
                {KIND_LABELS[k.kind]}
              </option>
            ))}
            {mcpUser && <option value="mcp_server">{KIND_LABELS.mcp_server}</option>}
          </select>
        </label>
        <label>
          Where{" "}
          <select value={place} onChange={(e) => setWhere(e.target.value)}>
            {places.map((p) => (
              <option key={p} value={p}>
                {p === "user" ? "All projects (user)" : p}
              </option>
            ))}
          </select>
        </label>
      </div>
      {needsName && <input placeholder="Name, e.g. write-tests" value={name} onChange={(e) => setName(e.target.value)} required />}
      {kind === "mcp_server" ? (
        <input placeholder="Command or URL, e.g. npx -y @modelcontextprotocol/server-github" value={command} onChange={(e) => setCommand(e.target.value)} />
      ) : (
        <input placeholder={kind === "memory" ? "First thing to remember" : "Description: when should the agent use this?"} value={description} onChange={(e) => setDescription(e.target.value)} />
      )}
      {places.length === 0 && <p className="warn">Add a project folder first (Load order tab).</p>}
      {error && <p className="error">{error}</p>}
      <div className="row">
        <button className="primary" type="submit" disabled={places.length === 0}>
          Create
        </button>
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
