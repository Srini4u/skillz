import { useEffect, useRef, useState } from "react";
import { readAsset } from "../api";
import { KIND_LABELS, OpenItem } from "../types";

export default function Viewer({ item, onClose }: { item: OpenItem | null; onClose: () => void }) {
  const [content, setContent] = useState("");
  const hit = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!item || item.kind === "mcp_server") return;
    let live = true;
    readAsset(item.path)
      .then((t) => live && setContent(t))
      .catch((e) => live && setContent(String(e)));
    return () => {
      live = false;
    };
  }, [item]);

  useEffect(() => {
    hit.current?.scrollIntoView({ block: "center" });
  }, [content, item?.line]);

  if (!item) return <aside className="viewer empty muted">Pick anything to see its file.</aside>;

  const lines = item.kind === "mcp_server" ? JSON.stringify(item.details, null, 2).split("\n") : content.split("\n");
  return (
    <aside className="viewer">
      <div className="viewer-head">
        <div>
          <strong>{item.name}</strong>
          <span className="muted">
            {" "}
            · {KIND_LABELS[item.kind]} · {item.agentName}
          </span>
        </div>
        <button className="icon" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      {item.description && <p className="muted desc">{item.description}</p>}
      <div className="muted path">{item.path}</div>
      <div className="code">
        {lines.map((l, i) => (
          <div key={i} ref={item.line === i + 1 ? hit : undefined} className={item.line === i + 1 ? "hl" : ""}>
            <span className="ln">{i + 1}</span>
            {l || " "}
          </div>
        ))}
      </div>
    </aside>
  );
}
