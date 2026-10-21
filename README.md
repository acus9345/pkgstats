# PkgStats

A native desktop app (Rust + [egui](https://github.com/emilk/egui)) that shows developer and package stats from **GitHub**, **npm**, **PyPI** and **crates.io**.
<img width="827" height="600" alt="image" src="https://github.com/user-attachments/assets/2a240d42-7c2e-4b18-bc7d-4e8f54ca62e8" />
(dont mind my things behind**)


## Install

- **Windows**: download `pkgstats-windows-x64.exe` from the [latest release](https://github.com/acus9345/pkgstats/releases/latest) and run it.
- **With Rust**: `cargo install pkgstats`, then run `pkgstats`.

## Modes

**User**: enter a username to look it up on each selected registry at once:

| Source    | What you get |
|-----------|--------------|
| GitHub    | followers, following, repo count, total stars/forks, top repos chart, repo table |
| npm       | package count, weekly/monthly downloads across all packages, top packages |
| PyPI      | owned/maintained packages with weekly/monthly downloads (first 40, via pypistats.org) |
| crates.io | crate count, all-time and 90-day downloads, top crates |

**Package / repo**: enter a package name (`react`, `requests`, `serde`) or `owner/repo` for GitHub:

| Source    | What you get |
|-----------|--------------|
| GitHub    | stars, forks, watchers, issues, license, latest release, language breakdown |
| npm       | last day/week/month/year downloads, daily chart for the last year, deps, license |
| PyPI      | last day/week/month downloads, ~180-day daily chart, releases, Python requirement |
| crates.io | all-time and 90-day downloads, daily chart for 90 days, version count |

If there's no exact match (a typo, or a GitHub name without `owner/`), GitHub, npm and crates.io show their search results instead. Click a result to open its stats. PyPI has no search API, so it needs the exact name.

In a user report, click a package or repo name to see its stats, or click the arrow icon next to it to open its page in your browser. **Open page** in a report's header opens that user's or package's page. The back arrow returns to the previous report.

## Window

PkgStats draws its own title bar, like Claude Desktop. The menu, back, Search and Following, the User / Package toggle, the search box and settings sit in it, with minimize, maximize and close on the right. Drag the empty part of the bar to move the window, double-click it to maximize, and drag any edge to resize.

## Menu and history

The menu button (top left) opens a sidebar with Search, Following, History and Settings, plus your followed items and latest searches. **History** lists every search you've run, grouped by day. Click one to run it again, remove single entries, or clear them all. History is saved to `%APPDATA%\PkgStats\history.json` (last 200 searches).

## Following (daily tracking)

- Click **Follow** (star) on any report, or enter your username on the **Following** page and click **Follow me on all sites**. Sites where the name doesn't exist are skipped.
- PkgStats saves one snapshot per day of every count (stars, followers, downloads…). The Following page shows the current value and the change over 1, 7 and 30 days, plus a history chart. Report cards also show "vs yesterday" for followed items.
- Snapshots are taken when the app opens and every 30 minutes while it runs. To keep recording while it's closed, use **Settings → Enable background task**. This adds an hourly Windows Task Scheduler job that runs `pkgstats --snapshot` and only fetches once a day.
- Data is stored in `%APPDATA%\PkgStats\following.json`.

## Build from source

```bash
cargo run --release
```

Without a token, GitHub allows 60 requests per hour and 10 searches per minute. Open **menu → Add token** and paste a GitHub token to raise that to 5,000 per hour and 30 searches per minute. The token needs no scopes, and the page links to GitHub's token form. It's stored in Windows Credential Manager. If no token is added, the `GITHUB_TOKEN` environment variable is used. npm, PyPI and crates.io need no token.

**Settings → App style** sets the app colour: Claude (default), Blue, Soft yellow, Peach, Rose, Lavender, Sky, Mint, Silver, or a custom colour. Claude matches Claude Desktop's dark theme (neutral near-black surfaces, outlined cards, grey selection, clay-orange accent), and Blue is a neutral near-black look. Any other colour tints the whole app, like Chrome's themes: background, sidebar, cards, borders, grid lines and text become tones of that colour. The choice is saved to `%APPDATA%\PkgStats\settings.json`.

## Tests

```bash
cargo test                             # offline unit tests
cargo test -- --ignored --nocapture    # hits the live APIs
```

## Notes

- PyPI user pages are behind a bot check, so user lookups use PyPI's XML-RPC `user_packages` call. PyPI usernames are case-sensitive.
- npm's search API returns at most 1,000 packages per maintainer. Download counts come from npm's official downloads API (api.npmjs.org), not the stale numbers in search results. That API throttles bursts, so PkgStats backs off and retries. If it's still blocked, it shows search's cached counts and says so.
- API responses are cached in memory for 10 minutes.
