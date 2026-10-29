import { useEffect, useState } from "react";
import { history, undo } from "../api";
import { JournalEntry } from "../types";

/** Every change Skillz made, newest first, each with an Undo. */
export default function History({ version, onChanged }: { version: number; onChanged: (e: JournalEntry[]) => void }) {
  const [entries, setEntries] = useState<JournalEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    history().then(setEntries).catch((e) => setError(String(e)));
  }, [version]);

  if (entries.length === 0) return <p className="muted">No changes yet. Everything you save, create, rename or delete shows up here with an Undo.</p>;

  return (
    <div className="history">
      {error && <p className="error">{error}</p>}
      {entries.map((e) => (
        <div key={e.id} className={`entry ${e.undone ? "undone" : ""}`}>
          <div>
            <strong>{e.summary}</strong>
            <div className="muted small">
              {new Date(e.time * 1000).toLocaleString()} · {e.to ? `${e.path} → ${e.to}` : e.path}
            </div>
          </div>
          <button
            disabled={e.undone}
            onClick={() =>
              undo(e.id)
                .then((u) => onChanged([u]))
                .catch((err) => setError(String(err)))
            }
          >
            {e.undone ? "Undone" : "Undo"}
          </button>
        </div>
      ))}
    </div>
  );
}
