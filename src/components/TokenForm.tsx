import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, errMsg, type TokenSource } from "../api";

const SOURCE_TEXT: Record<TokenSource, string> = {
  env: "Using GITLAB_TOKEN from the environment.",
  shell: "Using GITLAB_TOKEN from your login shell.",
  keychain: "Using the token saved in the system keychain.",
  none: "No token found. Issue titles and merge requests are unavailable.",
};

/** Show where the GitLab token comes from and let the user save one to the keychain. */
export function TokenForm({ hideWhenFound }: { hideWhenFound?: boolean }) {
  const qc = useQueryClient();
  const status = useQuery({ queryKey: ["token"], queryFn: api.tokenStatus });
  const [token, setToken] = useState("");

  const refresh = () => {
    setToken("");
    return Promise.all([
      qc.invalidateQueries({ queryKey: ["token"] }),
      qc.invalidateQueries({ queryKey: ["issue"] }),
    ]);
  };
  const save = useMutation({
    mutationFn: () => api.saveToken(token),
    onSuccess: refresh,
  });
  const clear = useMutation({ mutationFn: api.clearToken, onSuccess: refresh });

  const source = status.data;
  if (hideWhenFound && (source === "env" || source === "shell")) return null;

  return (
    <form
      className="form"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate();
      }}
    >
      <fieldset>
        <legend>GitLab token</legend>
        <p className="muted">
          {source ? SOURCE_TEXT[source] : "Checking…"} A token with{" "}
          <code>read_api</code> scope is enough.
        </p>
        <label>
          Save a token to the keychain
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="glpat-…"
            autoComplete="off"
          />
        </label>
        {(source === "env" || source === "shell") && (
          <p className="hint">
            A token from the environment or login shell takes priority over the
            keychain.
          </p>
        )}
        <div className="actions">
          <button
            type="submit"
            className="primary"
            disabled={!token.trim() || save.isPending}
          >
            Save token
          </button>
          {source === "keychain" && (
            <button
              type="button"
              onClick={() => clear.mutate()}
              disabled={clear.isPending}
            >
              Remove from keychain
            </button>
          )}
          {save.isError && <span className="error">{errMsg(save.error)}</span>}
          {clear.isError && <span className="error">{errMsg(clear.error)}</span>}
        </div>
      </fieldset>
    </form>
  );
}
