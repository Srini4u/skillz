import { Agent, KIND_LABELS, OpenItem, SearchHit, openAsset, scopeLabel } from "../types";

export default function SearchResults({ query, hits, agents, onOpen }: { query: string; hits: SearchHit[]; agents: Agent[]; onOpen: (i: OpenItem) => void }) {
  return (
    <div className="list">
      <h3>
        {hits.length} result{hits.length === 1 ? "" : "s"} for “{query}”
      </h3>
      {hits.map((h, i) => {
        const agent = agents.find((a) => a.id === h.agent_id);
        const asset = agent?.assets[h.asset];
        if (!agent || !asset) return null;
        return (
          <button key={i} className="item hit" onClick={() => onOpen(openAsset(agent, asset, h.line))}>
            <span>
              <strong>{asset.name}</strong>{" "}
              <span className="muted">
                {KIND_LABELS[asset.kind]} · {agent.name} · {scopeLabel(asset.scope)}
                {h.line ? ` · line ${h.line}` : ""}
              </span>
              {h.snippet && <span className="snippet">{h.snippet}</span>}
            </span>
          </button>
        );
      })}
    </div>
  );
}
