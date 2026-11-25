use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Read-only GitLab REST client bound to one host and token.
pub struct Client {
    http: reqwest::Client,
    api: String,
    token: String,
}

/// Issue fields shown in the UI.
#[derive(Clone, Debug, Serialize)]
pub struct IssueInfo {
    pub title: String,
    pub state: String,
    pub url: String,
}

/// Merge request fields shown in the UI.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct MrInfo {
    pub iid: u64,
    pub title: String,
    /// One of `open`, `draft`, `merged`, `closed`.
    pub state: String,
    pub url: String,
}

#[derive(Deserialize)]
struct RawIssue {
    title: String,
    state: String,
    web_url: String,
}

#[derive(Deserialize)]
struct RawMr {
    iid: u64,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    web_url: String,
}

impl Client {
    /// Build a client for the GitLab base URL.
    pub fn new(host: &str, token: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("haiku-flow")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            api: format!("{}/api/v4", host.trim().trim_end_matches('/')),
            token: token.to_string(),
        })
    }

    /// Fetch an issue.
    pub async fn issue(&self, project: &str, iid: u64) -> Result<IssueInfo, String> {
        let raw: RawIssue = self
            .get(&format!("{}/issues/{iid}", self.project_url(project)), &[])
            .await?;
        Ok(IssueInfo {
            title: raw.title,
            state: raw.state,
            url: raw.web_url,
        })
    }

    /// Fetch merge requests related to an issue, falling back to those from `branch`.
    pub async fn merge_requests(
        &self,
        project: &str,
        iid: u64,
        branch: Option<&str>,
    ) -> Result<Vec<MrInfo>, String> {
        let base = self.project_url(project);
        let related: Result<Vec<RawMr>, String> = self
            .get(&format!("{base}/issues/{iid}/related_merge_requests"), &[])
            .await;
        let raw = match related {
            Ok(found) if !found.is_empty() => found,
            other => match branch {
                Some(branch) => {
                    self.get(
                        &format!("{base}/merge_requests"),
                        &[("source_branch", branch)],
                    )
                    .await?
                }
                None => other?,
            },
        };
        Ok(raw.into_iter().map(to_mr_info).collect())
    }

    /// Return the API URL of a project, with `/` in the path escaped.
    fn project_url(&self, project: &str) -> String {
        format!("{}/projects/{}", self.api, project.replace('/', "%2F"))
    }

    /// Send an authenticated GET and decode the JSON body.
    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<T, String> {
        let resp = self
            .http
            .get(url)
            .header("PRIVATE-TOKEN", &self.token)
            .query(query)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("GitLab returned {status}"));
        }
        resp.json().await.map_err(|e| e.without_url().to_string())
    }
}

/// Map GitLab's merge request state and draft flag to the UI state.
pub fn mr_state(state: &str, draft: bool) -> &'static str {
    match state {
        "merged" => "merged",
        "closed" | "locked" => "closed",
        _ if draft => "draft",
        _ => "open",
    }
}

fn to_mr_info(raw: RawMr) -> MrInfo {
    MrInfo {
        iid: raw.iid,
        title: raw.title,
        state: mr_state(&raw.state, raw.draft).to_string(),
        url: raw.web_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mr_state_mapping() {
        assert_eq!(mr_state("opened", false), "open");
        assert_eq!(mr_state("opened", true), "draft");
        assert_eq!(mr_state("merged", true), "merged");
        assert_eq!(mr_state("closed", false), "closed");
        assert_eq!(mr_state("locked", false), "closed");
    }

    #[test]
    fn project_url_escapes_slashes() {
        let c = Client::new("https://gitlab.com/", "t").unwrap();
        assert_eq!(
            c.project_url("group/sub/proj"),
            "https://gitlab.com/api/v4/projects/group%2Fsub%2Fproj"
        );
    }

    #[test]
    fn raw_mr_decodes_without_draft_field() {
        let mr: RawMr = serde_json::from_str(
            r#"{"iid":3,"title":"t","state":"opened","web_url":"https://x/y/-/merge_requests/3"}"#,
        )
        .unwrap();
        assert_eq!(to_mr_info(mr).state, "open");
    }
}
