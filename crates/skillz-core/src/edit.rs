//! Safe writes to the agents' files: every change is checked against what was read,
//! backed up first, written atomically, and recorded in a journal so it can be undone.

use serde::{Deserialize, Serialize};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum EditError {
    /// The file changed on disk since it was read.
    Conflict(PathBuf),
    /// Undo refused: the file was changed again after the change being undone.
    ChangedSince(PathBuf),
    /// The new content of a JSON or TOML file does not parse.
    Invalid(String),
    NotFound(PathBuf),
    Io(io::Error),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(p) => write!(f, "{} changed on disk since you opened it. Reload it and try again.", p.display()),
            Self::ChangedSince(p) => write!(f, "{} was changed again after this, so undoing it would throw that change away. Undo the later change first.", p.display()),
            Self::Invalid(e) => write!(f, "Not saved, the file would be invalid: {e}"),
            Self::NotFound(p) => write!(f, "{} does not exist", p.display()),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for EditError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, EditError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Write,
    Delete,
    Rename,
    Undo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    /// Seconds since the Unix epoch.
    pub time: u64,
    pub action: Action,
    pub path: PathBuf,
    /// For a rename, where it went.
    pub to: Option<PathBuf>,
    /// Copy of what was at `path` before the change; `None` when nothing was there.
    pub backup: Option<PathBuf>,
    pub summary: String,
    pub undone: bool,
    /// What `path` looked like right after the change (see `state_of`), so undo can tell
    /// whether something else has changed it since.
    #[serde(default)]
    pub after: Option<String>,
}

/// Content plus a fingerprint to pass back on save, so a save never clobbers an outside change.
#[derive(Debug, Clone, Serialize)]
pub struct FileState {
    pub content: String,
    pub hash: String,
    pub exists: bool,
}

pub fn fingerprint(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn read_state(path: &Path) -> Result<FileState> {
    match fs::read(path) {
        Ok(bytes) => Ok(FileState { hash: fingerprint(&bytes), content: String::from_utf8_lossy(&bytes).into_owned(), exists: true }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(FileState { content: String::new(), hash: fingerprint(b""), exists: false }),
        Err(e) => Err(e.into()),
    }
}

/// Refuses JSON and TOML that would not parse, so a typo never breaks an agent.
pub fn validate(path: &Path, content: &str) -> Result<()> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => serde_json::from_str::<serde_json::Value>(content).map(|_| ()).map_err(|e| EditError::Invalid(e.to_string())),
        Some("toml") => content.parse::<toml_edit::DocumentMut>().map(|_| ()).map_err(|e| EditError::Invalid(e.to_string())),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// "same", "add" or "remove".
    pub tag: &'static str,
    pub text: String,
}

pub fn diff(old: &str, new: &str) -> Vec<DiffLine> {
    use similar::{ChangeTag, TextDiff};
    TextDiff::from_lines(old, new)
        .iter_all_changes()
        .map(|c| DiffLine {
            tag: match c.tag() {
                ChangeTag::Equal => "same",
                ChangeTag::Insert => "add",
                ChangeTag::Delete => "remove",
            },
            text: c.value().trim_end_matches('\n').to_string(),
        })
        .collect()
}

/// Where backups and the journal live, e.g. `~/.local/share/skillz`.
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `$XDG_DATA_HOME/skillz`, or `~/.local/share/skillz`.
    pub fn default_for(home: &Path) -> Self {
        let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
        Self::new(base.join("skillz"))
    }

    fn journal_path(&self) -> PathBuf {
        self.root.join("journal.json")
    }

    pub fn history(&self) -> Vec<JournalEntry> {
        let mut v: Vec<JournalEntry> = fs::read_to_string(self.journal_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        v.reverse();
        v
    }

    fn record(&self, mut entry: JournalEntry) -> Result<JournalEntry> {
        let mut all = self.history();
        all.reverse();
        entry.id = format!("{}-{}", entry.time, all.len());
        entry.after = state_of(&entry.path);
        all.push(entry.clone());
        atomic_write(&self.journal_path(), serde_json::to_string_pretty(&all).expect("journal serializes").as_bytes())?;
        Ok(entry)
    }

    /// Copies whatever is at `path` (file or folder) into a fresh backup slot.
    fn backup(&self, path: &Path, time: u64, n: usize) -> Result<Option<PathBuf>> {
        if !path.exists() {
            return Ok(None);
        }
        let name = path.to_string_lossy().trim_start_matches('/').replace('/', "%");
        let dest = self.root.join("backups").join(format!("{time}-{n}")).join(name);
        copy_all(path, &dest)?;
        Ok(Some(dest))
    }

    fn entry(&self, action: Action, path: &Path, backup: Option<PathBuf>, time: u64, summary: String) -> JournalEntry {
        JournalEntry { id: String::new(), time, action, path: path.to_path_buf(), to: None, backup, summary, undone: false, after: None }
    }

    /// Writes `content` to `path` if it still matches `expected_hash` (pass `None` to skip the
    /// check, e.g. for a file the caller just created).
    pub fn write(&self, path: &Path, content: &str, expected_hash: Option<&str>, summary: &str) -> Result<JournalEntry> {
        validate(path, content)?;
        let current = read_state(path)?;
        if let Some(h) = expected_hash {
            if h != current.hash {
                return Err(EditError::Conflict(path.to_path_buf()));
            }
        }
        let time = now();
        let backup = self.backup(path, time, self.history().len())?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        atomic_write(path, content.as_bytes())?;
        let action = if current.exists { Action::Write } else { Action::Create };
        self.record(self.entry(action, path, backup, time, summary.to_string()))
    }

    /// Removes a file or a whole folder (a skill with its scripts), keeping a backup.
    pub fn delete(&self, path: &Path, expected_hash: Option<&str>, summary: &str) -> Result<JournalEntry> {
        if !path.exists() {
            return Err(EditError::NotFound(path.to_path_buf()));
        }
        if let (Some(h), true) = (expected_hash, path.is_file()) {
            if read_state(path)?.hash != h {
                return Err(EditError::Conflict(path.to_path_buf()));
            }
        }
        let time = now();
        let backup = self.backup(path, time, self.history().len())?;
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        self.record(self.entry(Action::Delete, path, backup, time, summary.to_string()))
    }

    /// Copies a file or folder (a skill with its scripts) to `to`. Refuses to overwrite.
    pub fn copy_in(&self, from: &Path, to: &Path, summary: &str) -> Result<JournalEntry> {
        if !from.exists() {
            return Err(EditError::NotFound(from.to_path_buf()));
        }
        if to.exists() {
            return Err(EditError::Io(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists", to.display()))));
        }
        copy_all(from, to)?;
        self.record(self.entry(Action::Create, to, None, now(), summary.to_string()))
    }

    /// Moves a file or folder. Refuses to overwrite.
    pub fn rename(&self, from: &Path, to: &Path, summary: &str) -> Result<JournalEntry> {
        if !from.exists() {
            return Err(EditError::NotFound(from.to_path_buf()));
        }
        if to.exists() {
            return Err(EditError::Io(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists", to.display()))));
        }
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::rename(from, to)?;
        let mut e = self.entry(Action::Rename, from, None, now(), summary.to_string());
        e.to = Some(to.to_path_buf());
        self.record(e)
    }

    /// Puts things back the way they were before entry `id`. The undo is itself recorded
    /// (with a backup of the state it replaced), so it can be undone too.
    pub fn undo(&self, id: &str) -> Result<JournalEntry> {
        let mut all = self.history();
        all.reverse();
        let Some(pos) = all.iter().position(|e| e.id == id) else {
            return Err(EditError::Io(io::Error::new(io::ErrorKind::NotFound, format!("no change {id}"))));
        };
        let target = all[pos].clone();
        let time = now();
        let n = all.len();
        let summary = format!("Undo: {}", target.summary);
        let entry = match target.action {
            // Undoing a rename is the inverse rename, recorded as one so it can be undone too.
            Action::Rename => {
                let to = target.to.clone().unwrap_or_default();
                if !to.exists() {
                    return Err(EditError::NotFound(to));
                }
                if target.path.exists() {
                    return Err(EditError::Io(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists", target.path.display()))));
                }
                fs::rename(&to, &target.path)?;
                let mut e = self.entry(Action::Rename, &to, None, time, summary);
                e.to = Some(target.path.clone());
                e
            }
            _ => {
                // Undo restores the old version wholesale, so refuse if the file was edited
                // after this change rather than silently throwing that edit away.
                if target.after.is_some() && state_of(&target.path) != target.after {
                    return Err(EditError::ChangedSince(target.path.clone()));
                }
                let backup = self.backup(&target.path, time, n)?;
                remove_any(&target.path)?;
                if let Some(b) = &target.backup {
                    copy_all(b, &target.path)?;
                }
                self.entry(Action::Undo, &target.path, backup, time, summary)
            }
        };
        all[pos].undone = true;
        atomic_write(&self.journal_path(), serde_json::to_string_pretty(&all).expect("journal serializes").as_bytes())?;
        self.record(entry)
    }
}

/// A file's fingerprint, "absent" when nothing is there, or `None` for a folder (not tracked).
fn state_of(path: &Path) -> Option<String> {
    match fs::metadata(path) {
        Err(_) => Some("absent".into()),
        Ok(m) if m.is_file() => fs::read(path).ok().map(|b| fingerprint(&b)),
        Ok(_) => None,
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// Writes to a temp file next to `path`, then renames it over, so readers never see half a file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.skillz-tmp", path.file_name().unwrap_or_default().to_string_lossy()));
    fs::write(&tmp, bytes)?;
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, path)
}

fn copy_all(from: &Path, to: &Path) -> io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::copy(from, to).map(|_| ())
    }
}

fn remove_any(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_backup_undo_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let file = tmp.path().join("CLAUDE.md");
        fs::write(&file, "old").unwrap();
        let h = read_state(&file).unwrap().hash;

        let e = store.write(&file, "new", Some(&h), "edit").unwrap();
        assert_eq!(e.action, Action::Write);
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        assert_eq!(fs::read_to_string(e.backup.as_ref().unwrap()).unwrap(), "old");

        store.undo(&e.id).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "old");
        let hist = store.history();
        assert_eq!(hist[0].action, Action::Undo);
        assert!(hist[1].undone);

        // Undo the undo.
        store.undo(&hist[0].id).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn undo_refuses_after_an_outside_edit() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let file = tmp.path().join("AGENTS.md");
        let e = store.write(&file, "mine", None, "create").unwrap();
        fs::write(&file, "edited elsewhere").unwrap();
        assert!(matches!(store.undo(&e.id), Err(EditError::ChangedSince(_))));
        assert_eq!(fs::read_to_string(&file).unwrap(), "edited elsewhere");
    }

    #[test]
    fn stale_hash_is_a_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let file = tmp.path().join("a.md");
        fs::write(&file, "one").unwrap();
        let h = read_state(&file).unwrap().hash;
        fs::write(&file, "changed outside").unwrap();
        assert!(matches!(store.write(&file, "mine", Some(&h), "edit"), Err(EditError::Conflict(_))));
        assert_eq!(fs::read_to_string(&file).unwrap(), "changed outside");
    }

    #[test]
    fn invalid_json_and_toml_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        assert!(matches!(store.write(&tmp.path().join("s.json"), "{oops", None, "x"), Err(EditError::Invalid(_))));
        assert!(matches!(store.write(&tmp.path().join("c.toml"), "a = [", None, "x"), Err(EditError::Invalid(_))));
        assert!(!tmp.path().join("s.json").exists());
    }

    #[test]
    fn create_then_undo_removes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let file = tmp.path().join("skills/new/SKILL.md");
        let e = store.write(&file, "---\nname: new\n---\n", None, "create").unwrap();
        assert_eq!(e.action, Action::Create);
        store.undo(&e.id).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn delete_folder_and_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let dir = tmp.path().join("skills/pdf");
        fs::create_dir_all(dir.join("scripts")).unwrap();
        fs::write(dir.join("SKILL.md"), "s").unwrap();
        fs::write(dir.join("scripts/run.sh"), "echo").unwrap();
        let e = store.delete(&dir, None, "delete").unwrap();
        assert!(!dir.exists());
        store.undo(&e.id).unwrap();
        assert_eq!(fs::read_to_string(dir.join("scripts/run.sh")).unwrap(), "echo");
    }

    #[test]
    fn copy_folder_then_undo_removes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let src = tmp.path().join("a/pdf");
        fs::create_dir_all(src.join("scripts")).unwrap();
        fs::write(src.join("SKILL.md"), "s").unwrap();
        fs::write(src.join("scripts/x.sh"), "x").unwrap();
        let dest = tmp.path().join("b/pdf");
        let e = store.copy_in(&src, &dest, "copy").unwrap();
        assert_eq!(fs::read_to_string(dest.join("scripts/x.sh")).unwrap(), "x");
        assert!(store.copy_in(&src, &dest, "again").is_err());
        store.undo(&e.id).unwrap();
        assert!(!dest.exists() && src.exists());
    }

    #[test]
    fn rename_and_undo() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let a = tmp.path().join("skills/a");
        let b = tmp.path().join("skills/b");
        fs::create_dir_all(&a).unwrap();
        let e = store.rename(&a, &b, "rename").unwrap();
        assert!(b.exists() && !a.exists());
        store.undo(&e.id).unwrap();
        assert!(a.exists() && !b.exists());
        // Undoing the undo renames it again rather than losing it.
        let inverse = store.history()[0].clone();
        store.undo(&inverse.id).unwrap();
        assert!(b.exists() && !a.exists());
    }

    #[test]
    fn diff_marks_lines() {
        let d = diff("a\nb\n", "a\nc\n");
        let tags: Vec<_> = d.iter().map(|l| (l.tag, l.text.as_str())).collect();
        assert_eq!(tags, [("same", "a"), ("remove", "b"), ("add", "c")]);
    }
}
