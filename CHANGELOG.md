# Changelog

## 0.1.5

- The shaded area under download charts stays in place when the top of the curve is scrolled out of view.
- Debug builds (`cargo run`) optimize their libraries, so they're smooth instead of laggy.

## 0.1.4

- Debug builds (`cargo run`) no longer open a console window next to the app. Release builds already didn't.

## 0.1.3

- Title bar keeps the window buttons in place at any width; when the window is narrow, its buttons show icons only (hover for names) so the search box and Go still fit.
- Pages scroll sideways when the window is too narrow, and scroll bars are always visible instead of only on hover.
- Source filters, status messages and result tabs wrap instead of running off the edge.
- The close button's cross is the same size as the minimize and maximize icons.

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
