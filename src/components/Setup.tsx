import { useQuery } from "@tanstack/react-query";
import { api, errMsg } from "../api";
import { ConfigForm } from "./ConfigForm";
import { TokenForm } from "./TokenForm";

/** First-run screen shown until a config exists. */
export function Setup() {
  const defaults = useQuery({
    queryKey: ["default-config"],
    queryFn: api.defaultConfig,
  });

  return (
    <div className="setup">
      <h1>Welcome to haiku-flow</h1>
      <p className="muted">
        Pick the folders that contain your git repos and your GitLab host. You
        can change these later in Settings.
      </p>
      {defaults.isError && <p className="error">{errMsg(defaults.error)}</p>}
      {defaults.data && (
        <ConfigForm initial={defaults.data} submitLabel="Get started" />
      )}
      <TokenForm hideWhenFound />
    </div>
  );
}
