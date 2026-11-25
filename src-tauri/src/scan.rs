use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Find git repos under the roots, descending at most `depth` levels.
pub fn find_repos(roots: &[String], depth: u32) -> Vec<PathBuf> {
    let mut found = BTreeSet::new();
    for root in roots {
        walk(Path::new(root), depth, &mut found);
    }
    let mut repos: Vec<PathBuf> = found.into_iter().collect();
    repos.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    repos
}

/// Collect `dir` if it is a repo; otherwise recurse into its subdirectories.
fn walk(dir: &Path, remaining: u32, found: &mut BTreeSet<PathBuf>) {
    if dir.join(".git").exists() {
        found.insert(dir.to_path_buf());
        return;
    }
    if remaining == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk(&path, remaining - 1, found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(root: &Path, rel: &str) {
        fs::create_dir_all(root.join(rel).join(".git")).unwrap();
    }

    #[test]
    fn depth_one_finds_direct_children_only() {
        let dir = tempfile::tempdir().unwrap();
        repo(dir.path(), "a");
        repo(dir.path(), "b");
        repo(dir.path(), "group/c");
        fs::create_dir_all(dir.path().join("plain")).unwrap();
        let roots = vec![dir.path().to_string_lossy().into_owned()];
        let names: Vec<_> = find_repos(&roots, 1)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn depth_two_finds_nested_and_does_not_enter_repos() {
        let dir = tempfile::tempdir().unwrap();
        repo(dir.path(), "a");
        repo(dir.path(), "a/vendor/inner");
        repo(dir.path(), "group/c");
        let roots = vec![dir.path().to_string_lossy().into_owned()];
        let names: Vec<_> = find_repos(&roots, 2)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a", "c"]);
    }

    #[test]
    fn root_that_is_a_repo_is_included_and_missing_root_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        repo(dir.path(), "");
        let roots = vec![
            dir.path().to_string_lossy().into_owned(),
            "/definitely/not/here".to_string(),
        ];
        assert_eq!(find_repos(&roots, 1).len(), 1);
    }
}
