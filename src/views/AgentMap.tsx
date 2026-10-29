import { Agent, Asset, AssetKind, KIND_LABELS, KIND_ORDER, OpenItem, openAsset, scopeLabel } from "../types";

const W = 1000;
const H = 700;
const CX = W / 2;
const CY = H / 2;
const R_GROUP = 140;
const R_LEAF = [255, 300];
const MAX_LEAVES = 24;

const short = (s: string, n = 22) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

/** A radial map: the agent in the middle, one spoke per kind, every asset at the rim. */
export default function AgentMap({ agent, onOpen }: { agent: Agent; onOpen: (i: OpenItem) => void }) {
  const groups = KIND_ORDER.map((kind) => ({ kind, items: agent.assets.filter((a) => a.kind === kind) })).filter((g) => g.items.length > 0);
  // Each kind gets a slice of the circle sized by how many items it has, so busy kinds don't crowd their neighbours.
  const weights = groups.map((g) => Math.max(Math.min(g.items.length, MAX_LEAVES), 2));
  const total = weights.reduce((a, b) => a + b, 0);
  const starts = weights.map((_, i) => weights.slice(0, i).reduce((a, b) => a + b, 0));

  return (
    <div className="map">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`Map of ${agent.name}`}>
        {groups.map((g, gi) => {
          const sector = (2 * Math.PI * weights[gi]) / total;
          const angle = -Math.PI / 2 + (2 * Math.PI * starts[gi]) / total + sector / 2;
          const gx = CX + R_GROUP * Math.cos(angle);
          const gy = CY + R_GROUP * Math.sin(angle);
          const shown = g.items.slice(0, g.items.length > MAX_LEAVES ? MAX_LEAVES - 1 : MAX_LEAVES);
          const extra = g.items.length - shown.length;
          const slots = shown.length + (extra > 0 ? 1 : 0);
          const spread = sector * 0.8;
          const at = (i: number) => (slots === 1 ? angle : angle - spread / 2 + (spread * i) / (slots - 1));
          return (
            <g key={g.kind} className="map-group">
              <line x1={CX} y1={CY} x2={gx} y2={gy} className="edge" />
              {shown.map((a, i) => (
                <Leaf key={`${a.path}#${a.name}`} asset={a} angle={at(i)} r={R_LEAF[i % 2]} from={[gx, gy]} onClick={() => onOpen(openAsset(agent, a))} />
              ))}
              {extra > 0 && <MoreLeaf count={extra} angle={at(slots - 1)} r={R_LEAF[(slots - 1) % 2]} from={[gx, gy]} />}
              <circle cx={gx} cy={gy} r={34} className={`group-node kind-${g.kind}`} />
              <text x={gx} y={gy - 2} textAnchor="middle" className="group-label">
                {g.items.length}
              </text>
              <text x={gx} y={gy + 12} textAnchor="middle" className="group-sub">
                {short(KIND_LABELS[g.kind as AssetKind], 12)}
              </text>
            </g>
          );
        })}
        <circle cx={CX} cy={CY} r={52} className="agent-node" />
        <text x={CX} y={CY + 5} textAnchor="middle" className="agent-label">
          {agent.name}
        </text>
        {groups.length === 0 && (
          <text x={CX} y={CY + 90} textAnchor="middle" className="muted-fill">
            Nothing found for {agent.name} yet
          </text>
        )}
      </svg>
      <div className="legend">
        <span>
          <i className="dot scope-user" /> user
        </span>
        <span>
          <i className="dot scope-project" /> project
        </span>
        <span>
          <i className="dot scope-system" /> system
        </span>
        <span>
          <i className="dot hollow" /> disabled
        </span>
        <span className="muted">Click any dot to open its file.</span>
      </div>
    </div>
  );
}

function Leaf({ asset, angle, r, from, onClick }: { asset: Asset; angle: number; r: number; from: [number, number]; onClick: () => void }) {
  const x = CX + r * Math.cos(angle);
  const y = CY + r * Math.sin(angle);
  const right = Math.cos(angle) >= 0;
  return (
    <g className="leaf" onClick={onClick}>
      <title>{`${asset.name} · ${scopeLabel(asset.scope)}\n${asset.path}`}</title>
      <line x1={from[0]} y1={from[1]} x2={x} y2={y} className="edge thin" />
      <circle cx={x} cy={y} r={7} className={`scope-${asset.scope.type} ${asset.enabled ? "" : "hollow"}`} />
      <text x={x + (right ? 12 : -12)} y={y + 4} textAnchor={right ? "start" : "end"} className="leaf-label">
        {short(asset.name)}
      </text>
    </g>
  );
}

function MoreLeaf({ count, angle, r, from }: { count: number; angle: number; r: number; from: [number, number] }) {
  const x = CX + r * Math.cos(angle);
  const y = CY + r * Math.sin(angle);
  const right = Math.cos(angle) >= 0;
  return (
    <g>
      <line x1={from[0]} y1={from[1]} x2={x} y2={y} className="edge thin" />
      <circle cx={x} cy={y} r={7} className="hollow" />
      <text x={x + (right ? 12 : -12)} y={y + 4} textAnchor={right ? "start" : "end"} className="leaf-label muted-fill">
        +{count} more (see Overview)
      </text>
    </g>
  );
}
