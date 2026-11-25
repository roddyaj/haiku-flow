# haiku-flow

A local desktop app for your own git and GitLab workflow. One list shows every
local repo, its branch, whether it is out of sync or has local changes, and the
GitLab issue and merge request (MR) it relates to. A Done list keeps finished
issues.

GitLab access is read-only. Nothing is written to GitLab; links open in your
browser.

## Prerequisites

- [Rust](https://rustup.rs) (`rustup`) and Node 20+
- `git` on your `PATH`
- macOS: Xcode Command Line Tools. Other platforms: see the
  [Tauri prerequisites](https://tauri.app/start/prerequisites/)

## Run

```bash
npm install
npm run tauri dev
```

Build an app bundle with `npm run tauri build`. Run the backend tests with
`cargo test --manifest-path src-tauri/Cargo.toml`.

## First run

With no config, the app shows a setup screen: pick one or more folders that
contain your repos, and set your GitLab host. Change these later in Settings.

## What it shows

- **Branch** of each repo, found as a direct child of a root (scan depth is
  configurable).
- **↑n / ↓n**: commits ahead of / behind the branch's upstream.
- **↓n main**: commits behind `origin/<default branch>`.
- **● n**: changed, staged, untracked, or deleted files.
- **✓**: in sync and clean.
- **Issue**: parsed from a branch like `123-fix-thing`, or entered by hand with
  “+ issue” (remembered per repo and branch). With a token, the title is shown.
- **MR**: merge requests related to the issue, with state (open, draft, merged,
  closed).
- **Fetch** and **Fetch all** update local refs only. Nothing fetches on its
  own.
- **Hide** removes a repo from the list; “Show hidden” brings it back.
- **Mark done** moves an issue to the Done list with its MRs. **Undo** removes it.

## GitLab token

Create a personal or project access token with the `read_api` scope. The app
finds it in this order:

1. `GITLAB_TOKEN` in the app's environment.
2. `GITLAB_TOKEN` exported by your login shell (`$SHELL -lic …`), so apps
   launched from Finder or the Dock still see values from `.zshrc`.
3. The system keychain, where Settings can save one.

Settings shows which source is in use. The token is never written to the config
file or logs, and is only sent to the configured GitLab host. Without a token
the app still shows git status and issue links, but not titles or MRs.

## Files

Config and state live outside the repo, in the app config directory (on macOS,
`~/Library/Application Support/dev.haikuflow.app/`):

- `config.toml`: roots, host, scan depth, issue regex. See
  [`config.example.toml`](config.example.toml).
- `state.json`: hidden repos, manual issue entries, and Done items.

## License

See [LICENSE](LICENSE).
