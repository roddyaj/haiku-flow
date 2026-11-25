import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, errMsg, type DoneItem } from "../api";
import { Link } from "./Link";

export function DoneList() {
  const qc = useQueryClient();
  const done = useQuery({ queryKey: ["done"], queryFn: api.listDone });
  const undo = useMutation({
    mutationFn: (item: DoneItem) => api.undoDone(item.project, item.iid),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["done"] }),
  });

  return (
    <section>
      {done.isError && <p className="error">{errMsg(done.error)}</p>}
      {done.data?.length === 0 && (
        <p className="muted">
          Nothing done yet. Use “Mark done” on a repo’s issue.
        </p>
      )}
      <ul className="done-list">
        {done.data?.map((item) => (
          <li key={`${item.project}#${item.iid}`}>
            <div className="done-main">
              <Link href={item.issue_url} className="issue-link">
                {item.project}#{item.iid}
              </Link>
              {item.title && <span className="issue-title">{item.title}</span>}
            </div>
            <div className="done-meta">
              {item.mrs.map((mr) => (
                <Link key={mr.iid} href={mr.url} className={`mr ${mr.state}`}>
                  !{mr.iid} {mr.state}
                </Link>
              ))}
              <span className="muted">
                {new Date(item.done_at * 1000).toLocaleString()}
              </span>
              <button onClick={() => undo.mutate(item)}>Undo</button>
            </div>
          </li>
        ))}
      </ul>
      {undo.isError && <p className="error">{errMsg(undo.error)}</p>}
    </section>
  );
}
