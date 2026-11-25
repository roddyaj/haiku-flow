import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api";

interface Props {
  path: string;
  /** Current branch name, or `HEAD` when detached. */
  current: string;
  detached: boolean;
  defaultBranch: string;
  disabled: boolean;
  /** Check out `name`; `remote` is true for a `remote/name` branch with no local copy. */
  onSelect: (name: string, remote: boolean) => void;
}

/** Branch name that opens a searchable list of local and remote branches. */
export function BranchPicker({
  path,
  current,
  detached,
  defaultBranch,
  disabled,
  onSelect,
}: Props) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const root = useRef<HTMLDivElement>(null);

  const branches = useQuery({
    queryKey: ["branches", path],
    queryFn: () => api.listBranches(path),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
  });

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const typed = filter.trim();
  const q = typed.toLowerCase();
  const matches = (name: string) => name.toLowerCase().includes(q);
  const localNames = branches.data?.local ?? [];
  const local = localNames
    .filter((n) => n !== current && matches(n))
    .sort((a, b) => Number(b === defaultBranch) - Number(a === defaultBranch));
  const remoteAll = remoteOptions(branches.data?.remote ?? []);
  const remote = remoteAll.filter((o) => matches(o.ref));
  const known = [
    ...localNames,
    ...remoteAll.flatMap((o) => [o.ref, o.label]),
  ];
  const canUseTyped = typed !== "" && typed !== current && !known.includes(typed);

  function close() {
    setOpen(false);
    setFilter("");
  }

  function choose(name: string, isRemote: boolean) {
    close();
    onSelect(name, isRemote);
  }

  function submit() {
    if (typed === "") return;
    if (localNames.includes(typed)) return choose(typed, false);
    const exact = remoteAll.find((o) => o.ref === typed || o.label === typed);
    if (exact) return choose(exact.ref, true);
    if (local.length + remote.length === 1) {
      return local.length === 1
        ? choose(local[0], false)
        : choose(remote[0].ref, true);
    }
    if (canUseTyped) choose(typed, false);
  }

  return (
    <div className="branch-picker" ref={root}>
      <button
        className="branch-btn"
        title="Switch branch"
        disabled={disabled}
        onClick={() => (open ? close() : setOpen(true))}
      >
        {detached ? "detached HEAD" : current}
        <span className="caret">▾</span>
      </button>
      {open && (
        <div className="popover">
          <input
            autoFocus
            value={filter}
            placeholder="Filter or type a branch"
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
              if (e.key === "Escape") close();
            }}
          />
          <div className="options">
            {branches.isPending && <div className="muted pad">Loading…</div>}
            {branches.isError && <div className="error pad">Could not list branches</div>}
            {canUseTyped && (
              <button className="option" onClick={() => choose(typed, false)}>
                Check out “{typed}”
              </button>
            )}
            {local.length > 0 && <div className="group">Local</div>}
            {local.map((n) => (
              <button key={n} className="option" onClick={() => choose(n, false)}>
                {n}
                {n === defaultBranch && <span className="muted"> default</span>}
              </button>
            ))}
            {remote.length > 0 && <div className="group">Remote</div>}
            {remote.map((o) => (
              <button
                key={o.ref}
                className="option"
                title={o.ref}
                onClick={() => choose(o.ref, true)}
              >
                {o.label}
              </button>
            ))}
            {branches.data && !canUseTyped && local.length + remote.length === 0 && (
              <div className="muted pad">No other branches</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Remote branch as `remote/name` plus its display label. */
interface RemoteOption {
  ref: string;
  label: string;
}

/** Drop the remote name from each label, keeping it only where two remotes share a branch name. */
function remoteOptions(refs: string[]): RemoteOption[] {
  const short = (ref: string) => ref.slice(ref.indexOf("/") + 1);
  const counts = new Map<string, number>();
  for (const ref of refs) counts.set(short(ref), (counts.get(short(ref)) ?? 0) + 1);
  return refs.map((ref) => ({
    ref,
    label: (counts.get(short(ref)) ?? 0) > 1 ? ref : short(ref),
  }));
}
