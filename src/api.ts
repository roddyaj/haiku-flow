import { invoke } from "@tauri-apps/api/core";

export interface Config {
  roots: string[];
  host: string;
  scan_depth: number;
  issue_regex: string;
}

export type TokenSource = "env" | "shell" | "keychain" | "none";

export interface RepoEntry {
  path: string;
  name: string;
  hidden: boolean;
}

export interface IssueRef {
  iid: number;
  /** True when entered by hand rather than parsed from the branch name. */
  manual: boolean;
  url: string | null;
}

export interface RemoteInfo {
  project: string;
  web_base: string;
  /** True when the remote is on the configured host, so API calls are allowed. */
  api: boolean;
}

export interface RepoStatus {
  path: string;
  name: string;
  branch: string;
  detached: boolean;
  ahead: number | null;
  behind: number | null;
  default_branch: string;
  behind_main: number | null;
  dirty: number;
  remote: RemoteInfo | null;
  issue: IssueRef | null;
}

export interface BranchList {
  local: string[];
  /** Remote-tracking branches as `remote/name`. */
  remote: string[];
}

export type MrState = "open" | "draft" | "merged" | "closed";

export interface MrInfo {
  iid: number;
  title: string;
  state: MrState;
  url: string;
}

export interface IssueDetails {
  title: string;
  state: string;
  url: string;
  mrs: MrInfo[];
}

export interface MrRef {
  iid: number;
  state: string;
  url: string;
}

export interface DoneItem {
  project: string;
  iid: number;
  title: string | null;
  issue_url: string;
  mrs: MrRef[];
  /** Unix seconds. */
  done_at: number;
}

export type NewDoneItem = Omit<DoneItem, "done_at">;

export const api = {
  getConfig: () => invoke<Config | null>("get_config"),
  defaultConfig: () => invoke<Config>("default_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  tokenStatus: () => invoke<TokenSource>("token_status"),
  saveToken: (token: string) => invoke<TokenSource>("save_token", { token }),
  clearToken: () => invoke<TokenSource>("clear_token"),
  listRepos: () => invoke<RepoEntry[]>("list_repos"),
  repoStatus: (path: string) => invoke<RepoStatus>("repo_status", { path }),
  fetchRepo: (path: string) => invoke<void>("fetch_repo", { path }),
  listBranches: (path: string) => invoke<BranchList>("list_branches", { path }),
  checkoutBranch: (path: string, name: string, remote: boolean) =>
    invoke<void>("checkout_branch", { path, name, remote }),
  pullRepo: (path: string) => invoke<void>("pull_repo", { path }),
  setHidden: (path: string, hidden: boolean) =>
    invoke<void>("set_hidden", { path, hidden }),
  setManualIssue: (path: string, branch: string, issue: number | null) =>
    invoke<void>("set_manual_issue", { path, branch, issue }),
  issueDetails: (project: string, iid: number) =>
    invoke<IssueDetails>("issue_details", { project, iid }),
  listDone: () => invoke<DoneItem[]>("list_done"),
  markDone: (item: NewDoneItem) => invoke<void>("mark_done", { item }),
  undoDone: (project: string, iid: number) =>
    invoke<void>("undo_done", { project, iid }),
};

/** Convert a rejected invoke value into display text. */
export function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
