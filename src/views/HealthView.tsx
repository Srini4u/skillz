import { useEffect, useState } from "react";
import { copyAsset, createAsset, health } from "../api";
import { Agent, Check, Fix, Health, JournalEntry, OpenItem, agentLabel, openAsset } from "../types";

/** Each agent's score and checklist, with a one-click fix wherever one is safe. */
export default function HealthView({
  agents,
  version,
  onOpen,
  onChanged,
  onCoverage,
}: {
  agents: Agent[];
  version: number;
  onOpen: (i: OpenItem) => void;
  onChanged: (e: JournalEntry[]) => void;
  onCoverage: () => void;
}) {
  const [report, setReport] = useState<Health[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    health().then(setReport).catch((e) => setError(String(e)));
  }, [agents, version]);

  const apply = async (fix: Fix) => {
    setError(null);
    try {
      if (fix.type === "coverage") return onCoverage();
      if (fix.type === "open") {
        for (const a of agents) {
          const x = a.assets.find((y) => y.path === fix.path);
          if (x) return onOpen(openAsset(a, x));
        }
        return;
      }
      const e = fix.type === "copy" ? await copyAsset(fix.request) : await createAsset(fix.agent_id, fix.kind, fix.project, fix.agent_id === "codex" ? "AGENTS" : "CLAUDE", "");
      onChanged([e]);
    } catch (err) {
      setError(String(err));
    }
  };

  if (report.length === 0) return <p className="muted">No installed agents to check.</p>;

  return (
    <div className="health">
      <p className="muted">A well-rounded agent knows your rules in every project, has the skills and MCP servers you rely on, and has nothing broken. Each check below has a fix where one is safe to apply; every fix can be undone from History.</p>
      {error && <p className="error">{error}</p>}
      <div className="health-grid">
        {report.map((h) => (
          <section key={h.agent_id} className="health-card">
            <div className="score-row">
              <h3>{agentLabel(h.agent_id)}</h3>
              <span className={`score ${h.score >= 80 ? "good" : h.score >= 50 ? "mid" : "low"}`}>{h.score}</span>
            </div>
            <div className="meter">
              <span style={{ width: `${h.score}%` }} />
            </div>
            <ul className="checks">
              {[...h.checks].sort((a, b) => Number(a.ok) - Number(b.ok)).map((c) => (
                <CheckRow key={c.id} check={c} onFix={apply} />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}

function fixLabel(fix: Fix): string {
  switch (fix.type) {
    case "copy":
      return `Copy from ${agentLabel(fix.request.fromAgent)}`;
    case "create":
      return "Create it";
    case "open":
      return "Open";
    case "coverage":
      return "See coverage";
  }
}

function CheckRow({ check, onFix }: { check: Check; onFix: (f: Fix) => void }) {
  return (
    <li className={check.ok ? "ok" : `sev-${check.severity}`}>
      <span className="mark">{check.ok ? "✓" : check.severity === "warn" ? "!" : "○"}</span>
      <div>
        <div>{check.title}</div>
        <div className="muted small">{check.detail}</div>
      </div>
      {check.fix && <button onClick={() => onFix(check.fix!)}>{fixLabel(check.fix)}</button>}
    </li>
  );
}
