# Changelog

## 0.1.2

- Download charts now draw the curve through each day's real value. Before, the main line was a trailing 7-day average, which lagged behind spikes and didn't match the numbers. Hovering a day shows its count.

## 0.1.1

- App icon: the exe, taskbar and Alt+Tab now show PkgStats' own icon (rising bars) instead of the default one.

## 0.1.0

First release.

- Look up a **user** or a **package / repo** on GitHub, npm, PyPI and crates.io: followers, stars, packages, download totals and daily download charts.
- Typos and partial names show each site's search results (GitHub, npm, crates.io).
- **Following**: star any report, or enter your username once, to save a snapshot every day and see changes over 1, 7 and 30 days. An optional background task keeps recording while the app is closed.
- **History** of past searches, and a sidebar menu with followed items and recent searches.
- **Add token**: a GitHub token raises GitHub's limits from 60 to 5,000 requests per hour and from 10 to 30 searches per minute. It's stored in Windows Credential Manager.
- Dark dashboard look with smooth charts, a custom title bar, and app colours (Claude by default, plus Blue, Soft yellow, Peach, Rose, Lavender, Sky, Mint, Silver or a custom colour).
