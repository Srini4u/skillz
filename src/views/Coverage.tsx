import { useState } from "react";
import { copyTo } from "../api";
import { Agent, AssetKind, COPYABLE, JournalEntry, KIND_LABELS, OpenItem, agentLabel, openAsset, scopeLabel } from "../types";

const KINDS: AssetKind[] = ["skill", "mcp_server", "subagent", "command"];

interface Row {
  kind: AssetKind;
  name: string;
  /** Per agent, in `agents` order. */
  cells: { agent: Agent; matches: Agent["assets"] }[];
}

/** One row per skill / MCP server / subagent / command name, one column per agent. */
export default function Coverage({ agents, onOpen, onChanged }: { agents: Agent[]; onOpen: (i: OpenItem) => void; onChanged: (e: JournalEntry[]) => void }) {
  const [gapsOnly, setGapsOnly] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  // Codex has no subagents, so there is nowhere to copy one to.
  const canHold = (agent: Agent, kind: AssetKind) => COPYABLE.includes(kind) && !(kind === "subagent" && agent.id !== "claude-code");

  const copy = async (row: Row, to: Agent) => {
    const source = row.cells.find((c) => c.matches.length > 0);
    if (!source) return;
    const key = `${row.kind}:${row.name}:${to.id}`;
    setBusy(key);
    setError(null);
    try {
      onChanged([await copyTo(source.agent, source.matches[0], to.id)]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };
  const cols = agents.filter((a) => a.installed);
  const rows: Row[] = [];
  for (const kind of KINDS) {
    const names = [...new Set(cols.flatMap((a) => a.assets.filter((x) => x.kind === kind).map((x) => x.name)))].sort();
    for (const name of names) {
      rows.push({ kind, name, cells: cols.map((agent) => ({ agent, matches: agent.assets.filter((x) => x.kind === kind && x.name === name) })) });
    }
  }
  const isGap = (r: Row) => r.cells.some((c) => c.matches.length === 0);
  const gaps = rows.filter(isGap).length;
  const shown = gapsOnly ? rows.filter(isGap) : rows;
  const instr = (a: Agent, k: AssetKind) => a.assets.filter((x) => x.kind === k).length;

  if (cols.length === 0) return <p className="muted">No agents installed.</p>;

  return (
    <div className="coverage">
      <div className="coverage-head">
        <p>
          <strong>{gaps}</strong> of {rows.length} skills, MCP servers, subagents and commands are missing from at least one agent.
        </p>
        {error && <p className="error">{error}</p>}
        <label>
          <input type="checkbox" checked={gapsOnly} onChange={(e) => setGapsOnly(e.target.checked)} /> Gaps only
        </label>
      </div>
      <table className="heat">
        <thead>
          <tr>
            <th />
            {cols.map((a) => (
              <th key={a.id}>{a.name}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          <tr className="summary">
            <td>Instruction files</td>
            {cols.map((a) => (
              <td key={a.id} className={instr(a, "instruction") ? "have" : "gap"}>
                {instr(a, "instruction")}
              </td>
            ))}
          </tr>
          <tr className="summary">
            <td>Memory files</td>
            {cols.map((a) => (
              <td key={a.id} className={instr(a, "memory") ? "have" : "na"}>
                {a.id === "codex" ? "n/a" : instr(a, "memory")}
              </td>
            ))}
          </tr>
          {shown.map((r, i) => (
            <tr key={`${r.kind}:${r.name}`} className={i === 0 || shown[i - 1].kind !== r.kind ? "first-of-kind" : ""}>
              <td>
                <span className="kind-chip">{KIND_LABELS[r.kind]}</span> {r.name}
              </td>
              {r.cells.map(({ agent, matches }) => (
                <td
                  key={agent.id}
                  className={matches.length === 0 ? "gap" : matches.every((m) => !m.enabled) ? "off" : "have"}
                  onClick={matches[0] ? () => onOpen(openAsset(agent, matches[0])) : undefined}
                  title={matches.map((m) => `${scopeLabel(m.scope)}: ${m.path}`).join("\n") || `Not in ${agent.name}`}
                >
                  {matches.length > 0 ? (
                    matches.map((m) => scopeLabel(m.scope)).join(", ")
                  ) : canHold(agent, r.kind) ? (
                    <button className="copy-btn" disabled={busy !== null} onClick={() => copy(r, agent)}>
                      {busy === `${r.kind}:${r.name}:${agent.id}` ? "Copying…" : `Copy from ${agentLabel(r.cells.find((c) => c.matches.length > 0)!.agent.id)}`}
                    </button>
                  ) : (
                    "missing"
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
