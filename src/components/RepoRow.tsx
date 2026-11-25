import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  errMsg,
  type DoneItem,
  type RepoEntry,
  type RepoStatus,
} from "../api";
import { Link } from "./Link";

interface Props {
  repo: RepoEntry;
  done: DoneItem[];
}

export function RepoRow({ repo, done }: Props) {
  const qc = useQueryClient();
  const status = useQuery({
    queryKey: ["status", repo.path],
    queryFn: () => api.repoStatus(repo.path),
  });
  const token = useQuery({ queryKey: ["token"], queryFn: api.tokenStatus });
  const s = status.data;
  const issue = s?.issue ?? null;
  const remote = s?.remote ?? null;

  const details = useQuery({
    queryKey: ["issue", remote?.project, issue?.iid, issue?.manual ? null : s?.branch],
    queryFn: () =>
      api.issueDetails(
        remote!.project,
        issue!.iid,
        issue!.manual ? null : s!.branch,
      ),
    enabled: !!remote?.api && !!issue && !!token.data && token.data !== "none",
    staleTime: 60_000,
  });

  const refreshStatus = () =>
    qc.invalidateQueries({ queryKey: ["status", repo.path] });
  const fetchRepo = useMutation({
    mutationFn: () => api.fetchRepo(repo.path),
    onSettled: () =>
      Promise.all([
        refreshStatus(),
        qc.invalidateQueries({ queryKey: ["issue"] }),
      ]),
  });
  const setHidden = useMutation({
    mutationFn: () => api.setHidden(repo.path, !repo.hidden),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["repos"] }),
  });
  const setIssue = useMutation({
    mutationFn: (iid: number | null) =>
      api.setManualIssue(repo.path, s!.branch, iid),
    onSuccess: refreshStatus,
  });
  const markDone = useMutation({
    mutationFn: () =>
      api.markDone({
        project: remote!.project,
        iid: issue!.iid,
        title: details.data?.title ?? null,
        issue_url: issue!.url ?? "",
        mrs: (details.data?.mrs ?? []).map(({ iid, state, url }) => ({
          iid,
          state,
          url,
        })),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["done"] }),
  });

  const isDone =
    !!remote &&
    !!issue &&
    done.some((d) => d.project === remote.project && d.iid === issue.iid);
  const error = [fetchRepo, setHidden, setIssue, markDone]
    .map((m) => m.error)
    .find(Boolean);

  return (
    <div className={repo.hidden ? "repo hidden" : "repo"}>
      <div className="cell identity">
        <div className="name">{repo.name}</div>
        <div className="branch">
          {s ? (s.detached ? "detached HEAD" : s.branch) : "…"}
        </div>
      </div>

      <div className="cell badges">
        {status.isPending && <span className="muted">…</span>}
        {status.isError && (
          <span className="error" title={errMsg(status.error)}>
            git error
          </span>
        )}
        {s && <Badges s={s} />}
      </div>

      <div className="cell issue">
        {s && (
          <IssueCell
            s={s}
            title={details.data?.title}
            titleError={details.isError}
            isDone={isDone}
            onSetIssue={(iid) => setIssue.mutate(iid)}
            onMarkDone={() => markDone.mutate()}
            markDoneDisabled={!remote || markDone.isPending}
          />
        )}
      </div>

      <div className="cell mrs">
        {details.data?.mrs.map((mr) => (
          <Link
            key={mr.iid}
            href={mr.url}
            title={mr.title}
            className={`mr ${mr.state}`}
          >
            !{mr.iid} {mr.state}
          </Link>
        ))}
      </div>

      <div className="cell actions">
        <button
          onClick={() => fetchRepo.mutate()}
          disabled={fetchRepo.isPending}
        >
          {fetchRepo.isPending ? "Fetching…" : "Fetch"}
        </button>
        <button onClick={() => setHidden.mutate()}>
          {repo.hidden ? "Unhide" : "Hide"}
        </button>
      </div>

      {error && <div className="row-error error">{errMsg(error)}</div>}
    </div>
  );
}

function Badges({ s }: { s: RepoStatus }) {
  const hasUpstream = s.ahead !== null && s.behind !== null;
  const badges: { text: string; cls: string; title: string }[] = [];
  if (s.ahead) {
    badges.push({
      text: `↑${s.ahead}`,
      cls: "ahead",
      title: `${s.ahead} commit(s) not pushed to the upstream branch`,
    });
  }
  if (s.behind) {
    badges.push({
      text: `↓${s.behind}`,
      cls: "behind",
      title: `${s.behind} commit(s) behind the upstream branch`,
    });
  }
  if (s.behind_main) {
    badges.push({
      text: `↓${s.behind_main} ${s.default_branch}`,
      cls: "behind",
      title: `${s.behind_main} commit(s) behind origin/${s.default_branch}`,
    });
  }
  if (s.dirty) {
    badges.push({
      text: `● ${s.dirty}`,
      cls: "dirty",
      title: `${s.dirty} changed, staged, untracked, or deleted file(s)`,
    });
  }
  if (!hasUpstream && !s.detached) {
    badges.push({
      text: "no upstream",
      cls: "muted-badge",
      title: "This branch does not track a remote branch",
    });
  }
  if (badges.length === 0) {
    return (
      <span className="ok" title="In sync and clean">
        ✓
      </span>
    );
  }
  return (
    <>
      {badges.map((b) => (
        <span key={b.text} className={`badge ${b.cls}`} title={b.title}>
          {b.text}
        </span>
      ))}
    </>
  );
}

interface IssueCellProps {
  s: RepoStatus;
  title?: string;
  titleError: boolean;
  isDone: boolean;
  onSetIssue: (iid: number | null) => void;
  onMarkDone: () => void;
  markDoneDisabled: boolean;
}

function IssueCell({
  s,
  title,
  titleError,
  isDone,
  onSetIssue,
  onMarkDone,
  markDoneDisabled,
}: IssueCellProps) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const issue = s.issue;

  if (!issue) {
    if (!editing) {
      return (
        <button className="link" onClick={() => setEditing(true)}>
          + issue
        </button>
      );
    }
    const iid = Number.parseInt(value, 10);
    return (
      <form
        className="issue-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (iid > 0) {
            onSetIssue(iid);
            setEditing(false);
            setValue("");
          }
        }}
      >
        <input
          autoFocus
          type="number"
          min={1}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="Issue #"
        />
        <button type="submit" disabled={!(iid > 0)}>
          Set
        </button>
        <button type="button" className="link" onClick={() => setEditing(false)}>
          Cancel
        </button>
      </form>
    );
  }

  return (
    <>
      {issue.url ? (
        <Link href={issue.url} className="issue-link">
          #{issue.iid}
        </Link>
      ) : (
        <span className="issue-link">#{issue.iid}</span>
      )}
      {title && <span className="issue-title">{title}</span>}
      {titleError && !title && (
        <span className="muted" title="Could not load the issue from GitLab">
          (no title)
        </span>
      )}
      {issue.manual && (
        <button
          className="link"
          title="Remove the manually entered issue"
          onClick={() => onSetIssue(null)}
        >
          ×
        </button>
      )}
      {isDone ? (
        <span className="tag done">done</span>
      ) : (
        <button onClick={onMarkDone} disabled={markDoneDisabled}>
          Mark done
        </button>
      )}
    </>
  );
}
