import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errMsg, type Config } from "../api";

interface Props {
  initial: Config;
  submitLabel: string;
  /** Show the scan depth and issue regex fields. */
  advanced?: boolean;
}

/** Form for repo roots, GitLab host, and scan options. */
export function ConfigForm({ initial, submitLabel, advanced }: Props) {
  const qc = useQueryClient();
  const [config, setConfig] = useState(initial);
  const [saved, setSaved] = useState(false);

  const save = useMutation({
    mutationFn: () => api.saveConfig(config),
    onSuccess: () => {
      setSaved(true);
      return qc.invalidateQueries();
    },
  });

  const edit = (patch: Partial<Config>) => {
    setSaved(false);
    setConfig({ ...config, ...patch });
  };

  async function addFolders() {
    const picked = await open({
      directory: true,
      multiple: true,
      title: "Choose folders that contain your git repos",
    });
    if (!picked) return;
    const folders = Array.isArray(picked) ? picked : [picked];
    edit({ roots: [...new Set([...config.roots, ...folders])] });
  }

  return (
    <form
      className="form"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate();
      }}
    >
      <fieldset>
        <legend>Repo folders</legend>
        {config.roots.length === 0 && (
          <p className="muted">No folders yet. Add one that contains your repos.</p>
        )}
        <ul className="roots">
          {config.roots.map((root) => (
            <li key={root}>
              <code>{root}</code>
              <button
                type="button"
                className="link"
                onClick={() =>
                  edit({ roots: config.roots.filter((r) => r !== root) })
                }
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
        <button type="button" onClick={addFolders}>
          Add folder…
        </button>
      </fieldset>

      <label>
        GitLab host
        <input
          value={config.host}
          onChange={(e) => edit({ host: e.target.value })}
          placeholder="https://gitlab.com"
        />
      </label>

      {advanced && (
        <>
          <label>
            Scan depth
            <input
              type="number"
              min={1}
              max={10}
              value={config.scan_depth}
              onChange={(e) => edit({ scan_depth: Number(e.target.value) || 1 })}
            />
            <span className="hint">
              1 finds repos that are direct children of a folder.
            </span>
          </label>
          <label>
            Issue regex
            <input
              value={config.issue_regex}
              onChange={(e) => edit({ issue_regex: e.target.value })}
            />
            <span className="hint">
              Matched against the branch name; group 1 is the issue number.
            </span>
          </label>
        </>
      )}

      <div className="actions">
        <button
          type="submit"
          className="primary"
          disabled={save.isPending || config.roots.length === 0}
        >
          {submitLabel}
        </button>
        {saved && <span className="muted">Saved</span>}
        {save.isError && <span className="error">{errMsg(save.error)}</span>}
      </div>
    </form>
  );
}
