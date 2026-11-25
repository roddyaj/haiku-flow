import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, errMsg } from "../api";
import { RepoRow } from "./RepoRow";

const FETCH_CONCURRENCY = 4;

export function RepoList() {
  const qc = useQueryClient();
  const repos = useQuery({ queryKey: ["repos"], queryFn: api.listRepos });
  const done = useQuery({ queryKey: ["done"], queryFn: api.listDone });
  const [showHidden, setShowHidden] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [failed, setFailed] = useState(0);

  const all = repos.data ?? [];
  const hiddenCount = all.filter((r) => r.hidden).length;
  const visible = all.filter((r) => showHidden || !r.hidden);

  const refresh = () =>
    Promise.all(
      ["repos", "status", "issue", "done"].map((key) =>
        qc.invalidateQueries({ queryKey: [key] }),
      ),
    );

  async function fetchAll() {
    setFetching(true);
    setFailed(0);
    let failures = 0;
    try {
      const targets = all.filter((r) => !r.hidden);
      for (let i = 0; i < targets.length; i += FETCH_CONCURRENCY) {
        const batch = targets.slice(i, i + FETCH_CONCURRENCY);
        const results = await Promise.allSettled(
          batch.map((r) => api.fetchRepo(r.path)),
        );
        failures += results.filter((r) => r.status === "rejected").length;
      }
    } finally {
      setFailed(failures);
      setFetching(false);
      await refresh();
    }
  }

  return (
    <section>
      <div className="toolbar">
        <button onClick={refresh}>Refresh</button>
        <button onClick={fetchAll} disabled={fetching || all.length === 0}>
          {fetching ? "Fetching…" : "Fetch all"}
        </button>
        {failed > 0 && (
          <span className="error">
            {failed} fetch{failed === 1 ? "" : "es"} failed
          </span>
        )}
        <span className="spacer" />
        {hiddenCount > 0 && (
          <label className="inline">
            <input
              type="checkbox"
              checked={showHidden}
              onChange={(e) => setShowHidden(e.target.checked)}
            />
            Show hidden ({hiddenCount})
          </label>
        )}
      </div>

      {repos.isPending && <p className="muted">Scanning…</p>}
      {repos.isError && <p className="error">{errMsg(repos.error)}</p>}
      {repos.data && visible.length === 0 && (
        <p className="muted">
          {all.length === 0
            ? "No repos found. Check the folders and scan depth in Settings."
            : "All repos are hidden."}
        </p>
      )}

      <div className="repos">
        {visible.map((repo) => (
          <RepoRow
            key={repo.path}
            repo={repo}
            done={done.data ?? []}
          />
        ))}
      </div>
    </section>
  );
}
