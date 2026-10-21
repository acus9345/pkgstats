# PkgStats

A native desktop app (Rust + [egui](https://github.com/emilk/egui)) that shows developer and package stats from **GitHub**, **npm**, **PyPI** and **crates.io**.

## Modes

**👤 User**: enter a username to look it up on each selected registry at once:

| Source    | What you get |
|-----------|--------------|
| GitHub    | followers, following, repo count, total stars/forks, top repos chart, repo table |
| npm       | package count, weekly/monthly downloads across all packages, top packages |
| PyPI      | owned/maintained packages with weekly/monthly downloads (first 40, via pypistats.org) |
| crates.io | crate count, all-time and 90-day downloads, top crates |

**📦 Package / repo**: enter a package name (`react`, `requests`, `serde`) or `owner/repo` for GitHub:

| Source    | What you get |
|-----------|--------------|
| GitHub    | stars, forks, watchers, issues, license, latest release, language breakdown |
| npm       | last day/week/month/year downloads, daily chart for the last year, deps, license |
| PyPI      | last day/week/month downloads, ~180-day daily chart, releases, Python requirement |
| crates.io | all-time and 90-day downloads, daily chart for 90 days, version count |

In a user report, click a package or repo name to see its stats, or click the arrow icon next to it to open its page in your browser. **Open page** in a report's header opens that user's or package's page. The back arrow returns to the previous report.

## Menu and history

The menu button (top left) opens a sidebar with Search, Following, History and Settings, plus your followed items and latest searches. **History** lists every search you've run, grouped by day. Click one to run it again, remove single entries, or clear them all. History is saved to `%APPDATA%\PkgStats\history.json` (last 200 searches).

## Following (daily tracking)

- Click **Follow** (star) on any report, or enter your username on the **Following** page and click **Follow me on all sites**. Sites where the name doesn't exist are skipped.
- PkgStats saves one snapshot per day of every count (stars, followers, downloads…). The Following page shows the current value and the change over 1, 7 and 30 days, plus a history chart. Report cards also show "vs yesterday" for followed items.
- Snapshots are taken when the app opens and every 30 minutes while it runs. To keep recording while it's closed, use **Settings → Enable background task**. This adds an hourly Windows Task Scheduler job that runs `pkgstats --snapshot` and only fetches once a day.
- Data is stored in `%APPDATA%\PkgStats\following.json`.

## Run

```bash
cargo run --release
```

GitHub allows 60 unauthenticated requests per hour. To raise that limit, set `GITHUB_TOKEN` before launching, or paste a token in ⚙ Settings. The token is kept in memory only.

## Tests

```bash
cargo test                             # offline unit tests
cargo test -- --ignored --nocapture    # hits the live APIs
```

## Notes

- PyPI user pages are behind a bot check, so user lookups use PyPI's XML-RPC `user_packages` call. PyPI usernames are case-sensitive.
- npm's search API returns at most 1,000 packages per maintainer. Download counts come from npm's official downloads API (api.npmjs.org), not the stale numbers in search results. That API throttles bursts, so PkgStats backs off and retries. If it's still blocked, it shows search's cached counts and says so.
- API responses are cached in memory for 10 minutes.
