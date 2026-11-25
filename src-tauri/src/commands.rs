use crate::{
    git::{self, RepoStatus, StatusContext},
    gitlab::{self, MrInfo},
    scan,
    store::{self, Config, DoneItem, State},
    token::{TokenSource, Tokens},
};
use regex::Regex;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::async_runtime::spawn_blocking;

/// Shared app state managed by Tauri.
pub struct AppState {
    dir: PathBuf,
    config: Mutex<Option<Config>>,
    state: Mutex<State>,
    tokens: Arc<Tokens>,
}

impl AppState {
    /// Load config and state from `dir`.
    pub fn load(dir: PathBuf, tokens: Arc<Tokens>) -> Self {
        Self {
            config: Mutex::new(store::load_config(&dir)),
            state: Mutex::new(store::load_state(&dir)),
            dir,
            tokens,
        }
    }

    /// Apply `change` to the state and persist it.
    fn update_state<T>(&self, change: impl FnOnce(&mut State) -> T) -> Result<T, String> {
        let mut state = self.state.lock().unwrap();
        let result = change(&mut state);
        store::save_state(&self.dir, &state).map_err(|e| format!("Saving state: {e}"))?;
        Ok(result)
    }

    fn config(&self) -> Option<Config> {
        self.config.lock().unwrap().clone()
    }
}

/// Repo found by a scan.
#[derive(Serialize)]
pub struct RepoEntry {
    path: String,
    name: String,
    hidden: bool,
}

/// GitLab details for one issue.
#[derive(Serialize)]
pub struct IssueDetails {
    title: String,
    state: String,
    url: String,
    mrs: Vec<MrInfo>,
}

#[tauri::command]
pub fn get_config(app: tauri::State<'_, AppState>) -> Option<Config> {
    app.config()
}

#[tauri::command]
pub fn default_config() -> Config {
    Config::default()
}

#[tauri::command]
pub fn save_config(app: tauri::State<'_, AppState>, config: Config) -> Result<(), String> {
    let config = validate(config)?;
    store::save_config(&app.dir, &config).map_err(|e| format!("Saving config: {e}"))?;
    *app.config.lock().unwrap() = Some(config);
    Ok(())
}

#[tauri::command]
pub async fn token_status(app: tauri::State<'_, AppState>) -> Result<TokenSource, String> {
    resolve_token(&app).await.map(|(_, source)| source)
}

#[tauri::command]
pub async fn save_token(
    app: tauri::State<'_, AppState>,
    token: String,
) -> Result<TokenSource, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Token is empty".to_string());
    }
    let tokens = app.tokens.clone();
    spawn_blocking(move || tokens.save(&token))
        .await
        .map_err(|e| e.to_string())??;
    resolve_token(&app).await.map(|(_, source)| source)
}

#[tauri::command]
pub async fn clear_token(app: tauri::State<'_, AppState>) -> Result<TokenSource, String> {
    let tokens = app.tokens.clone();
    spawn_blocking(move || tokens.clear())
        .await
        .map_err(|e| e.to_string())??;
    resolve_token(&app).await.map(|(_, source)| source)
}

#[tauri::command]
pub async fn list_repos(app: tauri::State<'_, AppState>) -> Result<Vec<RepoEntry>, String> {
    let Some(config) = app.config() else {
        return Ok(Vec::new());
    };
    let paths = spawn_blocking(move || scan::find_repos(&config.roots, config.scan_depth))
        .await
        .map_err(|e| e.to_string())?;
    let state = app.state.lock().unwrap();
    Ok(paths
        .into_iter()
        .map(|p| {
            let path = p.to_string_lossy().into_owned();
            RepoEntry {
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                hidden: state.is_hidden(&path),
                path,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn repo_status(
    app: tauri::State<'_, AppState>,
    path: String,
) -> Result<RepoStatus, String> {
    let config = app.config().ok_or("Not configured")?;
    let state = app.state.lock().unwrap().clone();
    spawn_blocking(move || {
        let ctx = StatusContext {
            host: &config.host,
            issue_regex: &config.issue_regex,
            manual_issue: &|branch| state.manual_issue(&path, branch),
        };
        git::status(Path::new(&path), &ctx)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_repo(path: String) -> Result<(), String> {
    spawn_blocking(move || git::fetch(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn set_hidden(
    app: tauri::State<'_, AppState>,
    path: String,
    hidden: bool,
) -> Result<(), String> {
    app.update_state(|s| s.set_hidden(&path, hidden))
}

#[tauri::command]
pub fn set_manual_issue(
    app: tauri::State<'_, AppState>,
    path: String,
    branch: String,
    issue: Option<u64>,
) -> Result<(), String> {
    app.update_state(|s| s.set_manual_issue(&path, &branch, issue))
}

#[tauri::command]
pub async fn issue_details(
    app: tauri::State<'_, AppState>,
    project: String,
    iid: u64,
    branch: Option<String>,
) -> Result<IssueDetails, String> {
    let config = app.config().ok_or("Not configured")?;
    let (token, _) = resolve_token(&app).await?;
    let token = token.ok_or("No GitLab token")?;
    let client = gitlab::Client::new(&config.host, &token)?;
    let issue = client.issue(&project, iid).await?;
    let mrs = client
        .merge_requests(&project, iid, branch.as_deref())
        .await
        .unwrap_or_default();
    Ok(IssueDetails {
        title: issue.title,
        state: issue.state,
        url: issue.url,
        mrs,
    })
}

#[tauri::command]
pub fn list_done(app: tauri::State<'_, AppState>) -> Vec<DoneItem> {
    app.state.lock().unwrap().done_newest_first()
}

#[tauri::command]
pub fn mark_done(app: tauri::State<'_, AppState>, item: DoneItem) -> Result<(), String> {
    app.update_state(|s| s.mark_done(item, store::now_secs()))
}

#[tauri::command]
pub fn undo_done(
    app: tauri::State<'_, AppState>,
    project: String,
    iid: u64,
) -> Result<(), String> {
    app.update_state(|s| s.undo_done(&project, iid))
}

/// Resolve the token off the async runtime, since the first lookup runs a login shell.
async fn resolve_token(app: &AppState) -> Result<(Option<String>, TokenSource), String> {
    let tokens = app.tokens.clone();
    spawn_blocking(move || tokens.resolve())
        .await
        .map_err(|e| e.to_string())
}

/// Normalize a config and reject invalid values.
fn validate(mut config: Config) -> Result<Config, String> {
    config.roots.retain(|r| !r.trim().is_empty());
    config.roots.dedup();
    config.host = config.host.trim().trim_end_matches('/').to_string();
    if !(config.host.starts_with("https://") || config.host.starts_with("http://")) {
        return Err("Host must start with https:// or http://".to_string());
    }
    if config.scan_depth == 0 {
        return Err("Scan depth must be at least 1".to_string());
    }
    let re = Regex::new(&config.issue_regex).map_err(|e| format!("Issue regex: {e}"))?;
    if re.captures_len() < 2 {
        return Err("Issue regex needs a capture group for the number".to_string());
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            roots: vec!["/a".into(), "  ".into()],
            ..Config::default()
        }
    }

    #[test]
    fn validate_cleans_and_accepts_defaults() {
        let c = validate(Config {
            host: " https://gitlab.com/ ".into(),
            ..config()
        })
        .unwrap();
        assert_eq!(c.roots, ["/a"]);
        assert_eq!(c.host, "https://gitlab.com");
    }

    #[test]
    fn validate_rejects_bad_values() {
        let bad_host = Config {
            host: "gitlab.com".into(),
            ..config()
        };
        assert!(validate(bad_host).is_err());
        let zero_depth = Config {
            scan_depth: 0,
            ..config()
        };
        assert!(validate(zero_depth).is_err());
        let bad_regex = Config {
            issue_regex: "(".into(),
            ..config()
        };
        assert!(validate(bad_regex).is_err());
        let no_group = Config {
            issue_regex: r"^\d+-".into(),
            ..config()
        };
        assert!(validate(no_group).is_err());
    }
}
