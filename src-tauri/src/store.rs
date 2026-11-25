use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_HOST: &str = "https://gitlab.com";
pub const DEFAULT_ISSUE_REGEX: &str = r"^(\d+)-";

const CONFIG_FILE: &str = "config.toml";
const STATE_FILE: &str = "state.json";

/// User settings, persisted as TOML.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Absolute folders scanned for repos.
    pub roots: Vec<String>,
    /// GitLab base URL, for example `https://gitlab.com`.
    pub host: String,
    /// Directory levels below each root searched for repos.
    pub scan_depth: u32,
    /// Regex whose first group captures the issue number from a branch name.
    pub issue_regex: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            host: DEFAULT_HOST.to_string(),
            scan_depth: 1,
            issue_regex: DEFAULT_ISSUE_REGEX.to_string(),
        }
    }
}

/// Merge request captured when an issue is marked done.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MrRef {
    pub iid: u64,
    pub state: String,
    pub url: String,
}

/// Finished issue, independent of any repo.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DoneItem {
    /// GitLab project path, for example `group/project`.
    pub project: String,
    pub iid: u64,
    pub title: Option<String>,
    pub issue_url: String,
    pub mrs: Vec<MrRef>,
    /// Unix seconds; set by the backend when the item is added.
    #[serde(default)]
    pub done_at: u64,
}

/// Issue number entered by hand for one branch of one repo.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManualIssue {
    pub repo: String,
    pub branch: String,
    pub iid: u64,
}

/// Local state, persisted as JSON.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    /// Absolute paths of repos hidden from the list.
    pub hidden: Vec<String>,
    pub manual_issues: Vec<ManualIssue>,
    pub done: Vec<DoneItem>,
}

impl State {
    /// Report whether the repo is hidden.
    pub fn is_hidden(&self, path: &str) -> bool {
        self.hidden.iter().any(|p| p == path)
    }

    /// Hide or unhide a repo.
    pub fn set_hidden(&mut self, path: &str, hidden: bool) {
        self.hidden.retain(|p| p != path);
        if hidden {
            self.hidden.push(path.to_string());
        }
    }

    /// Return the hand-entered issue for a repo branch.
    pub fn manual_issue(&self, repo: &str, branch: &str) -> Option<u64> {
        self.manual_issues
            .iter()
            .find(|m| m.repo == repo && m.branch == branch)
            .map(|m| m.iid)
    }

    /// Set or clear the hand-entered issue for a repo branch.
    pub fn set_manual_issue(&mut self, repo: &str, branch: &str, iid: Option<u64>) {
        self.manual_issues
            .retain(|m| !(m.repo == repo && m.branch == branch));
        if let Some(iid) = iid {
            self.manual_issues.push(ManualIssue {
                repo: repo.to_string(),
                branch: branch.to_string(),
                iid,
            });
        }
    }

    /// Add a done item, replacing any for the same project and issue.
    pub fn mark_done(&mut self, mut item: DoneItem, now: u64) {
        item.done_at = now;
        self.undo_done(&item.project, item.iid);
        self.done.push(item);
    }

    /// Remove the done item for a project and issue.
    pub fn undo_done(&mut self, project: &str, iid: u64) {
        self.done.retain(|d| !(d.project == project && d.iid == iid));
    }

    /// Return done items, newest first.
    pub fn done_newest_first(&self) -> Vec<DoneItem> {
        let mut items = self.done.clone();
        items.sort_by(|a, b| b.done_at.cmp(&a.done_at));
        items
    }
}

/// Load the config, or `None` when it has never been saved.
pub fn load_config(dir: &Path) -> Option<Config> {
    let text = fs::read_to_string(dir.join(CONFIG_FILE)).ok()?;
    toml::from_str(&text).ok()
}

/// Write the config atomically.
pub fn save_config(dir: &Path, config: &Config) -> io::Result<()> {
    let text = toml::to_string_pretty(config).map_err(io::Error::other)?;
    write_atomic(&dir.join(CONFIG_FILE), text.as_bytes())
}

/// Load the state, or the default when missing or unreadable.
pub fn load_state(dir: &Path) -> State {
    fs::read_to_string(dir.join(STATE_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the state atomically.
pub fn save_state(dir: &Path, state: &State) -> io::Result<()> {
    let text = serde_json::to_string_pretty(state).map_err(io::Error::other)?;
    write_atomic(&dir.join(STATE_FILE), text.as_bytes())
}

/// Return the current time in unix seconds.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write via a temp file and rename so a crash never leaves a partial file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp: PathBuf = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(project: &str, iid: u64) -> DoneItem {
        DoneItem {
            project: project.into(),
            iid,
            title: None,
            issue_url: String::new(),
            mrs: vec![],
            done_at: 0,
        }
    }

    #[test]
    fn hidden_toggles() {
        let mut s = State::default();
        s.set_hidden("/a", true);
        s.set_hidden("/a", true);
        assert_eq!(s.hidden.len(), 1);
        assert!(s.is_hidden("/a"));
        s.set_hidden("/a", false);
        assert!(!s.is_hidden("/a"));
    }

    #[test]
    fn manual_issue_is_per_branch() {
        let mut s = State::default();
        s.set_manual_issue("/a", "main", Some(5));
        s.set_manual_issue("/a", "dev", Some(6));
        s.set_manual_issue("/a", "main", Some(7));
        assert_eq!(s.manual_issue("/a", "main"), Some(7));
        assert_eq!(s.manual_issue("/a", "dev"), Some(6));
        s.set_manual_issue("/a", "main", None);
        assert_eq!(s.manual_issue("/a", "main"), None);
    }

    #[test]
    fn done_is_keyed_by_project_and_issue_and_sorted() {
        let mut s = State::default();
        s.mark_done(item("g/p", 1), 10);
        s.mark_done(item("g/p", 2), 20);
        s.mark_done(item("g/p", 1), 30);
        let done = s.done_newest_first();
        assert_eq!(done.len(), 2);
        assert_eq!((done[0].iid, done[0].done_at), (1, 30));
        s.undo_done("g/p", 1);
        assert_eq!(s.done.len(), 1);
    }

    #[test]
    fn config_round_trips_and_defaults_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_config(dir.path()).is_none());
        let cfg = Config {
            roots: vec!["/x".into()],
            ..Config::default()
        };
        save_config(dir.path(), &cfg).unwrap();
        assert_eq!(load_config(dir.path()), Some(cfg));
        fs::write(dir.path().join(CONFIG_FILE), "roots = [\"/y\"]").unwrap();
        let loaded = load_config(dir.path()).unwrap();
        assert_eq!(loaded.host, DEFAULT_HOST);
        assert_eq!(loaded.scan_depth, 1);
    }

    #[test]
    fn state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_state(dir.path()), State::default());
        let mut s = State::default();
        s.set_hidden("/a", true);
        save_state(dir.path(), &s).unwrap();
        assert_eq!(load_state(dir.path()), s);
    }
}
