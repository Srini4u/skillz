import { useEffect, useMemo, useRef, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { json } from "@codemirror/lang-json";
import { StreamLanguage } from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { deleteMcp, deletePath, diffText, readFile, renameSkill, saveFile, saveMcp, setEnabled } from "../api";
import { DiffLine, FileState, JournalEntry, KIND_LABELS, OpenItem, mcpProject } from "../types";

type Mode = "view" | "edit" | "review";

const dark = () => window.matchMedia?.("(prefers-color-scheme: dark)").matches;

function language(path: string, isMcp: boolean) {
  if (isMcp || path.endsWith(".json")) return [json()];
  if (path.endsWith(".toml")) return [StreamLanguage.define(toml)];
  return [markdown()];
}

/**
 * Shows one asset and edits it. Files are edited as text; MCP servers as their JSON config,
 * written back in the agent's own format. Nothing is written until the diff has been seen.
 */
export default function Inspector({ item, onClose, onChanged }: { item: OpenItem | null; onClose: () => void; onChanged: (e: JournalEntry[]) => void }) {
  const isMcp = item?.kind === "mcp_server";
  const [file, setFile] = useState<FileState | null>(null);
  const [mode, setMode] = useState<Mode>("view");
  const [draft, setDraft] = useState("");
  const [draftName, setDraftName] = useState("");
  // The MCP config the draft started from, so a save can tell if it changed on disk since.
  const [opened, setOpened] = useState<unknown>(null);
  const [diff, setDiff] = useState<DiffLine[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);
  const hit = useRef<HTMLDivElement>(null);

  const original = useMemo(() => (isMcp ? JSON.stringify(item?.details ?? {}, null, 2) : (file?.content ?? "")), [isMcp, item, file]);

  const identity = item ? `${item.agentId}|${item.kind}|${item.path}|${item.name}` : "";
  const editing = useRef(false);
  editing.current = mode !== "view";

  // A different item: start over in view mode.
  useEffect(() => {
    setMode("view");
    setError(null);
    setConfirmDelete(false);
    setDraftName(item?.name ?? "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identity]);

  // Same item refreshed from disk: re-read it, unless a draft is open. Saving a draft
  // that went stale is refused by the hash check, so nothing is overwritten.
  useEffect(() => {
    if (!item || isMcp) return setFile(null);
    if (editing.current) return;
    let live = true;
    readFile(item.path)
      .then((f) => live && setFile(f))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [item, isMcp]);

  useEffect(() => {
    if (mode === "view") hit.current?.scrollIntoView({ block: "center" });
  }, [file, mode, item?.line]);

  if (!item) return <aside className="viewer empty muted">Pick anything to see its file.</aside>;

  const run = async (fn: () => Promise<JournalEntry | JournalEntry[]>) => {
    setBusy(true);
    setError(null);
    try {
      const out = await fn();
      onChanged(Array.isArray(out) ? out : [out]);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const startEdit = () => {
    setDraft(original);
    setOpened(item?.details ?? null);
    setMode("edit");
  };

  const review = async () => {
    if (isMcp) {
      try {
        JSON.parse(draft);
      } catch (e) {
        return setError(`Not valid JSON: ${e}`);
      }
    }
    setError(null);
    setDiff(await diffText(original, draft));
    setMode("review");
  };

  const save = async () => {
    const ok = await run(() =>
      isMcp
        ? saveMcp(item.path, mcpProject(item), draftName.trim(), JSON.parse(draft), draftName.trim() !== item.name ? item.name : null, opened)
        : saveFile(item.path, draft, file?.hash ?? null),
    );
    if (ok && isMcp && draftName.trim() !== item.name) return onClose();
    if (ok) {
      setMode("view");
      if (!isMcp) setFile(await readFile(item.path));
    }
  };

  const remove = () =>
    run(() => {
      if (isMcp) return deleteMcp(item.path, mcpProject(item), item.name);
      // A skill is its whole folder, scripts and all.
      const target = item.kind === "skill" ? item.path.replace(/\/SKILL\.md$/, "") : item.path;
      return deletePath(target, item.kind === "skill" ? null : (file?.hash ?? null));
    }).then((ok) => ok && onClose());

  const canToggle = item.agentId === "codex" && (item.kind === "skill" || item.kind === "mcp_server") && item.enabled !== undefined;
  const lines = original.split("\n");
  const changed = mode === "edit" && (draft !== original || (isMcp && draftName.trim() !== item.name));

  return (
    <aside className="viewer">
      <div className="viewer-head">
        <div>
          <strong>{item.name}</strong>
          <span className="muted">
            {" "}
            · {KIND_LABELS[item.kind]} · {item.agentName}
            {item.enabled === false && " · disabled"}
          </span>
        </div>
        <button className="icon" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      {item.description && <p className="muted desc">{item.description}</p>}
      <div className="muted path">{item.path}</div>

      <div className="toolbar">
        {mode === "view" && (
          <>
            <button onClick={startEdit} disabled={!isMcp && !file}>
              Edit
            </button>
            {canToggle && (
              <button disabled={busy} onClick={() => run(() => setEnabled(item.agentId, item.kind, item.path, item.name, !item.enabled))}>
                {item.enabled ? "Turn off" : "Turn on"}
              </button>
            )}
            {item.kind === "skill" && <RenameButton name={item.name} busy={busy} onRename={(n) => run(() => renameSkill(item.path, n)).then((ok) => ok && onClose())} />}
            {confirmDelete ? (
              <>
                <button className="danger" disabled={busy} onClick={remove}>
                  Delete {item.kind === "skill" ? "skill folder" : isMcp ? "server" : "file"}
                </button>
                <button onClick={() => setConfirmDelete(false)}>Keep it</button>
              </>
            ) : (
              <button className="ghost-danger" onClick={() => setConfirmDelete(true)}>
                Delete…
              </button>
            )}
          </>
        )}
        {mode === "edit" && (
          <>
            <button className="primary" disabled={!changed} onClick={review}>
              Review changes
            </button>
            <button onClick={() => setMode("view")}>Cancel</button>
          </>
        )}
        {mode === "review" && (
          <>
            <button className="primary" disabled={busy} onClick={save}>
              {busy ? "Saving…" : "Save"}
            </button>
            <button onClick={() => setMode("edit")}>Back to editing</button>
          </>
        )}
      </div>
      {confirmDelete && <p className="warn">A backup is kept, and you can undo this from History.</p>}
      {error && <p className="error">{error}</p>}

      {mode === "view" && (
        <div className="code">
          {lines.map((l, i) => (
            <div key={i} ref={item.line === i + 1 ? hit : undefined} className={item.line === i + 1 ? "hl" : ""}>
              <span className="ln">{i + 1}</span>
              {l || " "}
            </div>
          ))}
        </div>
      )}
      {mode === "edit" && (
        <>
          {isMcp && (
            <label className="field">
              Server name <input value={draftName} onChange={(e) => setDraftName(e.target.value)} />
            </label>
          )}
          {isMcp && <p className="muted small">Secret values show as •••••• and are kept as they are unless you type a new value.</p>}
          <CodeMirror value={draft} onChange={setDraft} extensions={language(item.path, isMcp)} theme={dark() ? "dark" : "light"} basicSetup={{ foldGutter: false }} className="editor" />
        </>
      )}
      {mode === "review" && (
        <div className="code diff">
          {isMcp && draftName.trim() !== item.name && <div className="add">Rename server to {draftName.trim()}</div>}
          {diff.map((d, i) => (
            <div key={i} className={d.tag}>
              <span className="ln">{d.tag === "add" ? "+" : d.tag === "remove" ? "−" : ""}</span>
              {d.text || " "}
            </div>
          ))}
        </div>
      )}
    </aside>
  );
}

function RenameButton({ name, busy, onRename }: { name: string; busy: boolean; onRename: (n: string) => void }) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(name);
  if (!editing) return <button onClick={() => setEditing(true)}>Rename</button>;
  return (
    <form
      className="inline"
      onSubmit={(e) => {
        e.preventDefault();
        if (value.trim() && value.trim() !== name) onRename(value.trim());
      }}
    >
      <input autoFocus value={value} onChange={(e) => setValue(e.target.value)} />
      <button type="submit" disabled={busy}>
        Rename
      </button>
      <button type="button" onClick={() => setEditing(false)}>
        Cancel
      </button>
    </form>
  );
}
