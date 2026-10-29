import { useEffect, useMemo, useState } from "react";
import { Agent, Asset, AssetKind, KIND_LABELS, KIND_ORDER, OpenItem, openAsset, scopeLabel } from "../types";

/** The most nodes ever drawn around one centre. Bigger sets are grouped or split alphabetically. */
const MAX_RING = 16;
/** Kinds grouped by name family (add-*, config-*); the rest are grouped by project. */
const BY_FAMILY: AssetKind[] = ["skill", "subagent", "command"];

interface MapNode {
  key: string;
  label: string;
  kind?: AssetKind;
  count: number;
  children?: MapNode[];
  asset?: Asset;
}

const short = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);
const family = (name: string) => name.toLowerCase().split(/[-_:.\s]/)[0] || name;

function leafNode(a: Asset, i: number): MapNode {
  return { key: `${a.kind}|${a.path}|${a.name}|${i}`, label: a.name, kind: a.kind, count: 1, asset: a };
}

function group(key: string, label: string, kind: AssetKind, children: MapNode[]): MapNode {
  return { key, label, kind, children, count: children.reduce((n, c) => n + c.count, 0) };
}

const cut = (s: string) => short(s, 9);

/** Keeps a ring at MAX_RING nodes or fewer by splitting it into alphabetical ranges. */
function fit(nodes: MapNode[], kind: AssetKind, key: string): MapNode[] {
  if (nodes.length <= MAX_RING) return nodes;
  const sorted = [...nodes].sort((a, b) => a.label.localeCompare(b.label));
  const parts = Math.ceil(sorted.length / MAX_RING);
  const size = Math.ceil(sorted.length / parts);
  const chunks: MapNode[] = [];
  for (let i = 0; i < sorted.length; i += size) {
    const s = sorted.slice(i, i + size);
    chunks.push(group(`${key}|${i}`, `${cut(s[0].label)} – ${cut(s[s.length - 1].label)}`, kind, s));
  }
  return fit(chunks, kind, `${key}+`);
}

function kindChildren(kind: AssetKind, items: Asset[], base: string): MapNode[] {
  const leaves = items.map(leafNode);
  if (leaves.length <= MAX_RING) return leaves;
  const byFamily = BY_FAMILY.includes(kind);
  const buckets = new Map<string, MapNode[]>();
  for (const l of leaves) {
    const k = byFamily ? family(l.label) : scopeLabel(l.asset!.scope);
    buckets.set(k, [...(buckets.get(k) ?? []), l]);
  }
  const groups: MapNode[] = [];
  const other: MapNode[] = [];
  for (const [k, xs] of buckets) {
    // A family of one or two is noise; those go together under "other".
    if (byFamily && xs.length < 3) other.push(...xs);
    else groups.push(group(`${base}|${k}`, byFamily ? `${k}-*` : k, kind, fit(xs, kind, `${base}|${k}`)));
  }
  groups.sort((a, b) => b.count - a.count);
  // Too many groups for one ring: the smallest ones join "other" so the big ones keep their names.
  while (groups.length + (other.length ? 1 : 0) > MAX_RING) other.push(...groups.pop()!.children!.flatMap((c) => c.children ?? [c]));
  if (other.length) groups.push(group(`${base}|other`, "other", kind, fit(other, kind, `${base}|other`)));
  return groups;
}

function buildTree(agent: Agent): MapNode {
  const kinds = KIND_ORDER.map((kind) => ({ kind, items: agent.assets.filter((a) => a.kind === kind) }))
    .filter((g) => g.items.length > 0)
    .map((g) => group(g.kind, KIND_LABELS[g.kind], g.kind, kindChildren(g.kind, g.items, g.kind)));
  return { key: "root", label: agent.name, count: kinds.reduce((n, k) => n + k.count, 0), children: kinds };
}

/** Walks a path of keys as far as it still exists, so a rescan doesn't throw the view back to the top. */
function walk(root: MapNode, path: string[]): MapNode[] {
  const chain = [root];
  for (const key of path) {
    const next = chain[chain.length - 1].children?.find((c) => c.key === key);
    if (!next?.children) break;
    chain.push(next);
  }
  return chain;
}

function findPath(node: MapNode, match: (n: MapNode) => boolean, trail: MapNode[] = []): MapNode[] | null {
  for (const c of node.children ?? []) {
    if (!c.children && match(c)) return [...trail, c];
    const hit = c.children && findPath(c, match, [...trail, c]);
    if (hit) return hit;
  }
  return null;
}

function arc(r0: number, r1: number, a0: number, a1: number) {
  const p = (r: number, a: number) => `${r * Math.cos(a)},${r * Math.sin(a)}`;
  const large = a1 - a0 > Math.PI ? 1 : 0;
  return `M${p(r1, a0)}A${r1},${r1} 0 ${large} 1 ${p(r1, a1)}L${p(r0, a1)}A${r0},${r0} 0 ${large} 0 ${p(r0, a0)}Z`;
}

/**
 * Focus rings: one node in the middle and only its direct children around it. Clicking a child
 * moves it to the middle; clicking the middle goes back up. Labels are horizontal, beside nodes on
 * the left and right and above or below them at the top and bottom.
 */
export default function AgentMap({ agent, onOpen }: { agent: Agent; onOpen: (i: OpenItem) => void }) {
  const tree = useMemo(() => buildTree(agent), [agent]);
  const [path, setPath] = useState<string[]>([]);
  const [hit, setHit] = useState<string | null>(null);
  const [find, setFind] = useState("");

  useEffect(() => {
    setPath([]);
    setHit(null);
    setFind("");
  }, [agent.id]);

  const chain = walk(tree, path);
  const focus = chain[chain.length - 1];
  const go = (n: number) => {
    setPath(chain.slice(1, n + 1).map((c) => c.key));
    setHit(null);
  };

  const search = (q: string) => {
    setFind(q);
    const s = q.trim().toLowerCase();
    if (s.length < 2) return setHit(null);
    const found = findPath(tree, (n) => n.label.toLowerCase().includes(s));
    if (!found) return setHit(null);
    setPath(found.slice(0, -1).map((c) => c.key));
    setHit(found[found.length - 1].key);
  };

  const kids = focus.children ?? [];
  const n = kids.length;
  const R = n <= 4 ? 150 : n <= 8 ? 175 : 200;
  const maxCount = Math.max(1, ...kids.map((k) => k.count));
  const start = -Math.PI / 2 + (n % 2 ? 0 : Math.PI / n);
  const HW = R + 215;
  const HH = R + 70;
  const CR = 52;

  return (
    <div className="map">
      <div className="map-tools">
        <nav className="crumbs" aria-label="Map path">
          {chain.map((c, i) =>
            i === chain.length - 1 ? (
              <span key={c.key} className="here">
                {c.label}
              </span>
            ) : (
              <span key={c.key}>
                <button onClick={() => go(i)}>{c.label}</button>
                <span className="sep">›</span>
              </span>
            ),
          )}
        </nav>
        <input className="map-find" type="search" placeholder="Find on map" value={find} onChange={(e) => search(e.target.value)} aria-label="Find on map" />
      </div>
      <svg viewBox={`${-HW} ${-HH} ${2 * HW} ${2 * HH}`} role="img" aria-label={`Map of ${focus.label}`}>
        <g key={focus.key} className="ring">
          {kids.map((k, i) => {
            const a = start + (i * 2 * Math.PI) / n;
            const r = k.children ? 15 + 12 * Math.sqrt(k.count / maxCount) : 8;
            return <line key={k.key} className="edge" x1={(CR + 4) * Math.cos(a)} y1={(CR + 4) * Math.sin(a)} x2={(R - r - 10) * Math.cos(a)} y2={(R - r - 10) * Math.sin(a)} />;
          })}
          <g className={`center ${chain.length > 1 ? "up" : ""}`} onClick={() => chain.length > 1 && go(chain.length - 2)}>
            {chain.length > 1 && <title>{`Back to ${chain[chain.length - 2].label}`}</title>}
            <circle r={CR} className={focus.kind ? `fill-${focus.kind}` : "agent-node"} />
            <text y={chain.length > 1 ? -3 : 2} textAnchor="middle" className="center-label">
              {short(focus.label, 13)}
            </text>
            <text y={chain.length > 1 ? 14 : 19} textAnchor="middle" className="center-sub">
              {chain.length > 1 ? `${focus.count} items · ↑ up` : `${focus.count} items`}
            </text>
          </g>
          {kids.map((k, i) => {
            const a = start + (i * 2 * Math.PI) / n;
            const r = k.children ? 15 + 12 * Math.sqrt(k.count / maxCount) : 8;
            const cos = Math.cos(a);
            const sin = Math.sin(a);
            const side = Math.abs(cos) >= 0.35;
            const gx = k.children ? 12 : 8;
            const gy = k.children ? 16 : 12;
            const tx = !side ? 0 : cos > 0 ? r + gx : -(r + gx);
            const ty = side ? 4 : sin < 0 ? -(r + gy) + 4 : r + gy + 4;
                        const label = short(k.label, 30);
            const open = () => {
              if (k.children) {
                setPath([...chain.slice(1).map((c) => c.key), k.key]);
                setHit(null);
              } else if (k.asset) {
                setHit(k.key);
                onOpen(openAsset(agent, k.asset));
              }
            };
            const slices = k.children ?? [];
            let at = 0;
            const pad = slices.length > 1 ? Math.min(0.08, 1.2 / slices.length) : 0;
            return (
              <g
                key={k.key}
                className={`node ${hit === k.key ? "hit" : ""}`}
                transform={`translate(${R * cos},${R * sin})`}
                role="button"
                tabIndex={0}
                aria-label={k.label}
                onClick={open}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    open();
                  }
                }}
              >
                <title>{k.asset ? `${k.asset.name} · ${scopeLabel(k.asset.scope)}${k.asset.enabled ? "" : " · disabled"}\n${k.asset.path}` : `${k.label} · ${k.count}`}</title>
                {slices.map((s) => {
                  const a0 = (at / k.count) * 2 * Math.PI - Math.PI / 2;
                  at += s.count;
                  const a1 = (at / k.count) * 2 * Math.PI - Math.PI / 2;
                  return <path key={s.key} d={arc(r + 3, r + 8, a0 + pad / 2, Math.max(a0 + pad / 2, a1 - pad / 2 - (slices.length === 1 ? 1e-4 : 0)))} className={`slice fill-${k.kind}`} />;
                })}
                <circle
                  r={r}
                  className={`body ${k.asset ? `scope-${k.asset.scope.type}` : `fill-${k.kind}`} ${k.asset && !k.asset.enabled ? "hollow" : ""}`}
                />
                {k.children && (
                  <text y={4} textAnchor="middle" className="count">
                    {k.count}
                  </text>
                )}
                <text x={tx} y={ty} textAnchor={!side ? "middle" : cos > 0 ? "start" : "end"} className={k.children ? "node-label" : "leaf-label"}>
                  {label}
                </text>
              </g>
            );
          })}
          {n === 0 && (
            <text y={CR + 40} textAnchor="middle" className="muted-fill">
              Nothing found for {agent.name} yet
            </text>
          )}
        </g>
      </svg>
      <div className="legend">
        {KIND_ORDER.filter((k) => agent.assets.some((a) => a.kind === k)).map((k) => (
          <span key={k}>
            <i className={`dot bg-${k}`} /> {KIND_LABELS[k]}
          </span>
        ))}
        <span className="legend-sep" />
        <span>
          <i className="dot scope-user" /> user
        </span>
        <span>
          <i className="dot scope-project" /> project
        </span>
        <span>
          <i className="dot hollow" /> disabled
        </span>
        <span className="muted">Click a group to open it, the centre to go back, an item to see its file.</span>
      </div>
    </div>
  );
}
