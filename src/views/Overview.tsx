import { Agent, KIND_LABELS, KIND_ORDER, OpenItem, openAsset, scopeLabel } from "../types";

export default function Overview({ agent, open, onOpen }: { agent: Agent; open: OpenItem | null; onOpen: (i: OpenItem) => void }) {
  return (
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
                className={`item ${open?.path === a.path && open?.name === a.name ? "active" : ""} ${a.enabled ? "" : "disabled"}`}
                onClick={() => onOpen(openAsset(agent, a))}
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
  );
}
