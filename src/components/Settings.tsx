import type { Config } from "../api";
import { ConfigForm } from "./ConfigForm";
import { TokenForm } from "./TokenForm";

export function Settings({ config }: { config: Config }) {
  return (
    <div className="settings">
      <ConfigForm initial={config} submitLabel="Save settings" advanced />
      <TokenForm />
    </div>
  );
}
