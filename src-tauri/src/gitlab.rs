use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

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
    #[serde(default)]
    description: Option<String>,
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

    /// Fetch merge requests for an issue: those GitLab relates to it plus a search for `#<iid>`,
    /// keeping only those whose title starts with `#<iid> ` or whose description has `Closes #<iid>`.
    pub async fn merge_requests(&self, project: &str, iid: u64) -> Result<Vec<MrInfo>, String> {
        let base = self.project_url(project);
        let related: Result<Vec<RawMr>, String> = self
            .get(
                &format!("{base}/issues/{iid}/related_merge_requests"),
                &[("per_page", "100")],
            )
            .await;
        let search: Result<Vec<RawMr>, String> = self
            .get(
                &format!("{base}/merge_requests"),
                &[("search", &format!("#{iid}")), ("per_page", "100")],
            )
            .await;
        if let (Err(e), Err(_)) = (&related, &search) {
            return Err(e.clone());
        }
        let mut candidates = related.unwrap_or_default();
        candidates.extend(search.unwrap_or_default());
        Ok(select_mrs(candidates, iid))
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

/// Keep merge requests that belong to the issue, deduplicated and ordered by number.
fn select_mrs(candidates: Vec<RawMr>, iid: u64) -> Vec<MrInfo> {
    let mut selected = BTreeMap::new();
    for mr in candidates {
        if mr_references_issue(&mr.title, mr.description.as_deref().unwrap_or(""), iid) {
            selected.entry(mr.iid).or_insert_with(|| to_mr_info(mr));
        }
    }
    selected.into_values().collect()
}

/// Report whether the title starts with `#<iid>` (then whitespace or end),
/// or the description contains `Closes #<iid>`.
fn mr_references_issue(title: &str, description: &str, iid: u64) -> bool {
    let title_re = Regex::new(&format!(r"^#{iid}(?:\s|$)")).expect("valid regex");
    let closes_re = Regex::new(&format!(r"(?i)\bcloses\s+#{iid}\b")).expect("valid regex");
    title_re.is_match(title.trim_start()) || closes_re.is_match(description)
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

    fn raw(iid: u64, title: &str, description: Option<&str>) -> RawMr {
        RawMr {
            iid,
            title: title.into(),
            state: "opened".into(),
            draft: false,
            description: description.map(str::to_string),
            web_url: format!("https://x/y/-/merge_requests/{iid}"),
        }
    }

    #[test]
    fn mr_matches_title_prefix_or_closes_line() {
        assert!(mr_references_issue("#1319 Fix the thing", "", 1319));
        assert!(mr_references_issue("#1319", "", 1319));
        assert!(mr_references_issue("Unrelated title", "Details\n\nCloses #1319", 1319));
        assert!(mr_references_issue("x", "closes #1319.", 1319));
    }

    #[test]
    fn mr_rejects_other_issues_and_mere_mentions() {
        assert!(!mr_references_issue("#13190 Other", "", 1319));
        assert!(!mr_references_issue("Fix #1319", "", 1319));
        assert!(!mr_references_issue("x", "Closes #13190", 1319));
        assert!(!mr_references_issue("x", "Related to #1319", 1319));
        assert!(!mr_references_issue("x", "Closes #1328", 1319));
        assert!(!mr_references_issue("x", "Does not close #1319", 1319));
    }

    #[test]
    fn select_mrs_filters_dedupes_and_sorts() {
        let picked = select_mrs(
            vec![
                raw(944, "Linked but unrelated", Some("See #1319")),
                raw(950, "#1319 Real fix", None),
                raw(948, "Other", Some("Closes #1319")),
                raw(950, "#1319 Real fix", None),
            ],
            1319,
        );
        let iids: Vec<_> = picked.iter().map(|m| m.iid).collect();
        assert_eq!(iids, [948, 950]);
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
