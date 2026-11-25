use regex::Regex;
use serde::Serialize;
use std::{path::Path, process::Command};

/// Parsed `origin` remote.
#[derive(Clone, Debug, PartialEq)]
pub struct Remote {
    /// Lowercased hostname without port or credentials.
    pub host: String,
    /// GitLab project path, for example `group/project`.
    pub project: String,
}

/// Issue linked to a branch.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct IssueRef {
    pub iid: u64,
    /// True when entered by hand rather than parsed from the branch name.
    pub manual: bool,
    /// Issue page, when the remote is known.
    pub url: Option<String>,
}

/// Git state of one repo.
#[derive(Clone, Debug, Serialize)]
pub struct RepoStatus {
    pub path: String,
    pub name: String,
    pub branch: String,
    pub detached: bool,
    /// Commits ahead of the upstream branch; `None` without an upstream.
    pub ahead: Option<u32>,
    /// Commits behind the upstream branch; `None` without an upstream.
    pub behind: Option<u32>,
    pub default_branch: String,
    /// Commits behind `origin/<default>`; `None` on the default branch or when it is missing.
    pub behind_main: Option<u32>,
    /// Count of changed, staged, untracked, or deleted paths.
    pub dirty: u32,
    pub remote: Option<RemoteInfo>,
    pub issue: Option<IssueRef>,
}

/// Remote details sent to the UI.
#[derive(Clone, Debug, Serialize)]
pub struct RemoteInfo {
    pub project: String,
    /// Base URL for links, without a trailing slash.
    pub web_base: String,
    /// True when the remote is on the configured GitLab host, so API calls are allowed.
    pub api: bool,
}

/// Inputs for building a status that do not come from git.
pub struct StatusContext<'a> {
    /// Configured GitLab base URL.
    pub host: &'a str,
    pub issue_regex: &'a str,
    /// Look up the hand-entered issue for a branch.
    pub manual_issue: &'a dyn Fn(&str) -> Option<u64>,
}

/// Run git in `path` and return trimmed-right stdout.
pub fn run(path: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    let out = cmd.output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Fetch from the remotes, updating local refs only.
pub fn fetch(path: &Path) -> Result<(), String> {
    run(path, &["fetch", "--all", "--prune"]).map(|_| ())
}

/// Local and remote branches of a repo.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct BranchList {
    pub local: Vec<String>,
    /// Remote-tracking branches as `remote/name`, excluding any that have a local branch of the same name.
    pub remote: Vec<String>,
}

/// List the repo's local and remote branches.
pub fn list_branches(path: &Path) -> Result<BranchList, String> {
    run(
        path,
        &["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes"],
    )
    .map(|out| parse_branches(&out))
}

/// Check out a branch. A remote branch (`remote/name`) gets a local tracking branch.
pub fn checkout(path: &Path, name: &str, remote: bool) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.starts_with('-') {
        return Err("Invalid branch name".to_string());
    }
    let args: &[&str] = if remote {
        &["checkout", "--track", name]
    } else {
        &["checkout", name]
    };
    run(path, args).map(|_| ())
}

/// Fast-forward the current branch from its upstream.
pub fn pull(path: &Path) -> Result<(), String> {
    run(path, &["pull", "--ff-only"]).map(|_| ())
}

/// Gather the status of the repo at `path`.
pub fn status(path: &Path, ctx: &StatusContext) -> RepoStatus {
    let branch = current_branch(path);
    let detached = branch == "HEAD";
    let (ahead, behind) = run(
        path,
        &["rev-list", "--left-right", "--count", "HEAD...@{u}"],
    )
    .ok()
    .and_then(|s| parse_left_right(&s))
    .unzip();

    let default_branch = run(path, &["symbolic-ref", "refs/remotes/origin/HEAD"])
        .ok()
        .and_then(|s| parse_default_branch(&s))
        .unwrap_or_else(|| "main".to_string());
    let behind_main = if branch == default_branch {
        None
    } else {
        run(
            path,
            &[
                "rev-list",
                "--count",
                &format!("HEAD..origin/{default_branch}"),
            ],
        )
        .ok()
        .and_then(|s| s.trim().parse().ok())
    };

    let dirty = run(path, &["status", "--porcelain"])
        .map(|s| parse_porcelain(&s))
        .unwrap_or(0);

    let remote = run(path, &["remote", "get-url", "origin"])
        .ok()
        .and_then(|url| parse_remote_url(&url));
    let remote_info = remote.as_ref().map(|r| remote_info(r, ctx.host));

    let issue = (ctx.manual_issue)(&branch)
        .map(|iid| (iid, true))
        .or_else(|| issue_from_branch(&branch, ctx.issue_regex).map(|iid| (iid, false)))
        .map(|(iid, manual)| IssueRef {
            iid,
            manual,
            url: remote_info
                .as_ref()
                .map(|r| format!("{}/{}/-/issues/{iid}", r.web_base, r.project)),
        });

    RepoStatus {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        branch,
        detached,
        ahead,
        behind,
        default_branch,
        behind_main,
        dirty,
        remote: remote_info,
        issue,
    }
}

/// Parse `git status --porcelain` output into a count of changed paths.
pub fn parse_porcelain(out: &str) -> u32 {
    out.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

/// Parse `rev-list --left-right --count` output (`ahead<TAB>behind`).
pub fn parse_left_right(out: &str) -> Option<(u32, u32)> {
    let mut parts = out.split_whitespace();
    let left = parts.next()?.parse().ok()?;
    let right = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((left, right))
}

/// Parse `for-each-ref --format=%(refname)` output into local and remote branches.
pub fn parse_branches(out: &str) -> BranchList {
    let mut list = BranchList::default();
    for line in out.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("refs/heads/") {
            list.local.push(name.to_string());
        } else if let Some(name) = line.strip_prefix("refs/remotes/") {
            if !name.ends_with("/HEAD") && name.contains('/') {
                list.remote.push(name.to_string());
            }
        }
    }
    let local = &list.local;
    list.remote
        .retain(|r| r.split_once('/').is_none_or(|(_, b)| !local.iter().any(|l| l == b)));
    list
}

/// Parse `refs/remotes/origin/<branch>` into the branch name.
pub fn parse_default_branch(out: &str) -> Option<String> {
    let name = out.trim().strip_prefix("refs/remotes/origin/")?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Parse an SSH, scp-style, or HTTP(S) remote URL into host and project path.
pub fn parse_remote_url(url: &str) -> Option<Remote> {
    let url = url.trim();
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?,
        None => url.split_once(':')?,
    };
    let host = hostname_of_authority(authority);
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || !path.contains('/') || path.starts_with('/') {
        return None;
    }
    Some(Remote {
        host,
        project: path.to_string(),
    })
}

/// Return the lowercased hostname of a URL or bare host, or `None` when empty.
pub fn hostname_of(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://").map_or(url.trim(), |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or("");
    let host = hostname_of_authority(authority);
    (!host.is_empty()).then_some(host)
}

/// Extract the issue number from a branch name using the first capture group.
pub fn issue_from_branch(branch: &str, pattern: &str) -> Option<u64> {
    let re = Regex::new(pattern).ok()?;
    re.captures(branch)?.get(1)?.as_str().parse().ok()
}

/// Build the link base and API eligibility for a remote against the configured host.
fn remote_info(remote: &Remote, config_host: &str) -> RemoteInfo {
    let api = hostname_of(config_host).as_deref() == Some(remote.host.as_str());
    let web_base = if api {
        config_host.trim().trim_end_matches('/').to_string()
    } else {
        format!("https://{}", remote.host)
    };
    RemoteInfo {
        project: remote.project.clone(),
        web_base,
        api,
    }
}

/// Strip credentials and port from `[user@]host[:port]`, then lowercase.
fn hostname_of_authority(authority: &str) -> String {
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    host.split(':').next().unwrap_or("").to_lowercase()
}

/// Return the current branch name, `HEAD` when detached.
fn current_branch(path: &Path) -> String {
    run(path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .or_else(|_| run(path, &["symbolic-ref", "--short", "HEAD"]))
        .unwrap_or_else(|_| "(unknown)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(host: &str, project: &str) -> Option<Remote> {
        Some(Remote {
            host: host.into(),
            project: project.into(),
        })
    }

    #[test]
    fn porcelain_counts_every_kind_of_change() {
        let out = " M src/a.rs\nA  b.rs\n?? new.txt\n D gone.txt\n\n";
        assert_eq!(parse_porcelain(out), 4);
        assert_eq!(parse_porcelain(""), 0);
    }

    #[test]
    fn left_right_parses_tab_separated_counts() {
        assert_eq!(parse_left_right("3\t5\n"), Some((3, 5)));
        assert_eq!(parse_left_right("0\t0"), Some((0, 0)));
        assert_eq!(parse_left_right(""), None);
        assert_eq!(parse_left_right("1"), None);
        assert_eq!(parse_left_right("a\tb"), None);
        assert_eq!(parse_left_right("1\t2\t3"), None);
    }

    #[test]
    fn default_branch_strips_origin_prefix() {
        assert_eq!(
            parse_default_branch("refs/remotes/origin/main\n"),
            Some("main".into())
        );
        assert_eq!(
            parse_default_branch("refs/remotes/origin/release/1.x"),
            Some("release/1.x".into())
        );
        assert_eq!(parse_default_branch("refs/heads/main"), None);
        assert_eq!(parse_default_branch(""), None);
    }

    #[test]
    fn branches_split_local_and_remote_and_drop_duplicates() {
        let out = "refs/heads/main\nrefs/heads/dev\nrefs/remotes/origin/HEAD\n\
                   refs/remotes/origin/main\nrefs/remotes/origin/feature/x\n\
                   refs/remotes/up/dev\nrefs/remotes/origin\n";
        let list = parse_branches(out);
        assert_eq!(list.local, ["main", "dev"]);
        assert_eq!(list.remote, ["origin/feature/x"]);
    }

    #[test]
    fn checkout_rejects_option_like_names() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(checkout(tmp.path(), "--detach", false).is_err());
        assert!(checkout(tmp.path(), "  ", false).is_err());
    }

    #[test]
    fn checkout_and_pull_work_on_real_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin.git");
        let work = tmp.path().join("work");
        let other = tmp.path().join("other");
        std::fs::create_dir_all(&origin).unwrap();
        g(&origin, &["init", "--bare", "-b", "main"]);
        g(tmp.path(), &["clone", origin.to_str().unwrap(), "work"]);
        g(&work, &["commit", "--allow-empty", "-m", "one"]);
        g(&work, &["push", "-u", "origin", "main"]);

        // Another clone publishes a feature branch.
        g(tmp.path(), &["clone", origin.to_str().unwrap(), "other"]);
        g(&other, &["checkout", "-b", "feature"]);
        g(&other, &["commit", "--allow-empty", "-m", "f1"]);
        g(&other, &["push", "-u", "origin", "feature"]);

        fetch(&work).unwrap();
        let list = list_branches(&work).unwrap();
        assert_eq!(list.local, ["main"]);
        assert_eq!(list.remote, ["origin/feature"]);

        // Checking out the remote branch creates a tracking branch.
        checkout(&work, "origin/feature", true).unwrap();
        assert_eq!(current_branch(&work), "feature");
        let list = list_branches(&work).unwrap();
        assert_eq!(list.local, ["feature", "main"]);
        assert!(list.remote.is_empty());

        // Back to a local branch by name.
        checkout(&work, "main", false).unwrap();
        assert_eq!(current_branch(&work), "main");
        assert!(checkout(&work, "no-such-branch", false).is_err());

        // Pull fast-forwards after the other clone pushes more.
        g(&other, &["commit", "--allow-empty", "-m", "f2"]);
        g(&other, &["push", "origin", "feature"]);
        checkout(&work, "feature", false).unwrap();
        pull(&work).unwrap();
        let log = run(&work, &["log", "--format=%s", "-1"]).unwrap();
        assert_eq!(log, "f2");
    }

    #[test]
    fn remote_url_forms() {
        let expect = remote("gitlab.com", "group/proj");
        assert_eq!(parse_remote_url("git@gitlab.com:group/proj.git"), expect);
        assert_eq!(parse_remote_url("https://gitlab.com/group/proj.git"), expect);
        assert_eq!(parse_remote_url("https://gitlab.com/group/proj"), expect);
        assert_eq!(parse_remote_url("https://tok:x@GitLab.com/group/proj.git/"), expect);
        assert_eq!(parse_remote_url("ssh://git@gitlab.com:2222/group/proj.git"), expect);
        assert_eq!(
            parse_remote_url("git@git.example.org:a/b/c.git"),
            remote("git.example.org", "a/b/c")
        );
    }

    #[test]
    fn remote_url_rejects_non_gitlab_shapes() {
        assert_eq!(parse_remote_url(""), None);
        assert_eq!(parse_remote_url("/local/path/repo"), None);
        assert_eq!(parse_remote_url("file:///local/path/repo.git"), None);
        assert_eq!(parse_remote_url("https://gitlab.com/onlyone"), None);
    }

    #[test]
    fn hostname_of_handles_urls_and_ports() {
        assert_eq!(hostname_of("https://GitLab.com/"), Some("gitlab.com".into()));
        assert_eq!(
            hostname_of("http://git.local:8080/sub"),
            Some("git.local".into())
        );
        assert_eq!(hostname_of("gitlab.com"), Some("gitlab.com".into()));
        assert_eq!(hostname_of(""), None);
    }

    #[test]
    fn issue_from_branch_uses_first_group() {
        let re = r"^(\d+)-";
        assert_eq!(issue_from_branch("123-fix-thing", re), Some(123));
        assert_eq!(issue_from_branch("fix-123", re), None);
        assert_eq!(issue_from_branch("main", re), None);
        assert_eq!(issue_from_branch("feature/45_x", r"/(\d+)_"), Some(45));
        assert_eq!(issue_from_branch("123-x", "("), None);
        assert_eq!(issue_from_branch("123-x", r"^\d+-"), None);
    }

    /// Run git in `dir`, panicking on failure.
    fn g(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn ctx<'a>(manual: &'a dyn Fn(&str) -> Option<u64>) -> StatusContext<'a> {
        StatusContext {
            host: "https://gitlab.com",
            issue_regex: r"^(\d+)-",
            manual_issue: manual,
        }
    }

    #[test]
    fn status_reports_dirty_ahead_behind_and_issue_on_real_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin.git");
        let work = tmp.path().join("work");
        let other = tmp.path().join("other");
        std::fs::create_dir_all(&origin).unwrap();
        g(&origin, &["init", "--bare", "-b", "main"]);
        g(tmp.path(), &["clone", origin.to_str().unwrap(), "work"]);
        g(&work, &["commit", "--allow-empty", "-m", "one"]);
        g(&work, &["push", "-u", "origin", "main"]);
        g(&work, &["remote", "set-head", "origin", "main"]);
        g(&work, &["remote", "set-url", "origin", "git@gitlab.com:grp/proj.git"]);
        g(&work, &["remote", "set-url", "--push", "origin", origin.to_str().unwrap()]);

        let none = |_: &str| None;
        let clean = status(&work, &ctx(&none));
        assert_eq!(clean.branch, "main");
        assert_eq!((clean.ahead, clean.behind), (Some(0), Some(0)));
        assert_eq!((clean.dirty, clean.behind_main), (0, None));
        assert_eq!(clean.default_branch, "main");
        let remote = clean.remote.unwrap();
        assert_eq!(remote.project, "grp/proj");
        assert!(remote.api);
        assert!(clean.issue.is_none());

        // Untracked file makes the repo dirty.
        std::fs::write(work.join("new.txt"), "x").unwrap();
        assert_eq!(status(&work, &ctx(&none)).dirty, 1);
        g(&work, &["add", "new.txt"]);
        g(&work, &["commit", "-m", "two"]);
        let ahead = status(&work, &ctx(&none));
        assert_eq!((ahead.ahead, ahead.behind, ahead.dirty), (Some(1), Some(0), 0));

        // A numbered branch yields an issue; a manual entry wins.
        g(&work, &["checkout", "-b", "123-fix"]);
        let issue = status(&work, &ctx(&none)).issue.unwrap();
        assert_eq!((issue.iid, issue.manual), (123, false));
        assert_eq!(
            issue.url.as_deref(),
            Some("https://gitlab.com/grp/proj/-/issues/123")
        );
        let manual = |b: &str| (b == "123-fix").then_some(9);
        let issue = status(&work, &ctx(&manual)).issue.unwrap();
        assert_eq!((issue.iid, issue.manual), (9, true));

        // Behind origin/main after another clone pushes, then fetching.
        g(&work, &["checkout", "main"]);
        g(&work, &["push", "origin", "main"]);
        g(tmp.path(), &["clone", origin.to_str().unwrap(), "other"]);
        g(&other, &["commit", "--allow-empty", "-m", "three"]);
        g(&other, &["push", "origin", "main"]);
        assert_eq!(status(&work, &ctx(&none)).behind, Some(0));
        // Fetch from the local bare repo, then restore the GitLab-style URL.
        let fake = "git@gitlab.com:grp/proj.git";
        g(&work, &["remote", "set-url", "origin", origin.to_str().unwrap()]);
        fetch(&work).unwrap();
        g(&work, &["remote", "set-url", "origin", fake]);
        let behind = status(&work, &ctx(&none));
        assert_eq!(behind.behind, Some(1));
        g(&work, &["checkout", "123-fix"]);
        assert_eq!(status(&work, &ctx(&none)).behind_main, Some(1));
    }

    #[test]
    fn remote_info_allows_api_only_on_configured_host() {
        let r = Remote {
            host: "gitlab.com".into(),
            project: "g/p".into(),
        };
        let same = remote_info(&r, "https://gitlab.com/");
        assert!(same.api);
        assert_eq!(same.web_base, "https://gitlab.com");
        let other = remote_info(&r, "https://git.corp.example");
        assert!(!other.api);
        assert_eq!(other.web_base, "https://gitlab.com");
    }
}
