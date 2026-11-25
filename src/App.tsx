import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, errMsg } from "./api";
import { DoneList } from "./components/DoneList";
import { RepoList } from "./components/RepoList";
import { Settings } from "./components/Settings";
import { Setup } from "./components/Setup";

type Tab = "repos" | "done" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "repos", label: "Repos" },
  { id: "done", label: "Done" },
  { id: "settings", label: "Settings" },
];

export default function App() {
  const config = useQuery({ queryKey: ["config"], queryFn: api.getConfig });
  const [tab, setTab] = useState<Tab>("repos");

  if (config.isPending) return <p className="center muted">Loading…</p>;
  if (config.isError)
    return <p className="center error">{errMsg(config.error)}</p>;
  if (!config.data) return <Setup />;

  return (
    <div className="app">
      <header className="topbar">
        <h1>haiku-flow</h1>
        <nav>
          {TABS.map((t) => (
            <button
              key={t.id}
              className={t.id === tab ? "tab active" : "tab"}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>
      <main>
        {tab === "repos" && <RepoList />}
        {tab === "done" && <DoneList />}
        {tab === "settings" && <Settings config={config.data} />}
      </main>
    </div>
  );
}
