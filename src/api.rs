//! Fetchers for GitHub, npm, PyPI and crates.io.
//!
//! Every fetcher is blocking and returns a [`Report`], a source-agnostic
//! description of what to show: headline metrics, an optional chart and an
//! optional table of items (repos / packages / crates).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const RATE_LIMITED: &str = "Rate limited (429). Try again in a minute.";
const USER_AGENT: &str = "pkgstats-desktop/0.1 (https://github.com/; stats viewer)";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    GitHub,
    Npm,
    PyPI,
    Crates,
}

impl Source {
    pub const ALL: [Source; 4] = [Source::GitHub, Source::Npm, Source::PyPI, Source::Crates];

    pub fn label(self) -> &'static str {
        match self {
            Source::GitHub => "GitHub",
            Source::Npm => "npm",
            Source::PyPI => "PyPI",
            Source::Crates => "crates.io",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    User,
    Package,
}

#[derive(Clone, Debug)]
pub struct Metric {
    pub label: String,
    pub value: String,
    /// Set for plain counts ("1,234"); these are the values the Following page tracks.
    pub num: Option<f64>,
}

#[derive(Clone, Debug)]
pub enum Chart {
    /// Daily time series: (YYYY-MM-DD, value).
    Daily { title: String, points: Vec<(String, f64)> },
    /// Labelled bars, already sorted.
    Bars { title: String, bars: Vec<(String, f64)> },
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub description: String,
    pub url: String,
    /// One value per entry in [`Report::columns`]; `sort_key` orders the table.
    pub values: Vec<String>,
    pub sort_key: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub title: String,
    pub subtitle: String,
    pub url: String,
    pub metrics: Vec<Metric>,
    pub charts: Vec<Chart>,
    pub columns: Vec<String>,
    pub items: Vec<Item>,
    /// Shown under the header, e.g. "only the first 30 packages were checked".
    pub note: Option<String>,
}

impl Report {
    /// Numeric metrics, as recorded in daily snapshots.
    pub fn tracked(&self) -> BTreeMap<String, f64> {
        self.metrics.iter().filter_map(|m| Some((m.label.clone(), m.num?))).collect()
    }
}

pub type FetchResult = Result<Report, String>;

pub fn fetch(source: Source, kind: Kind, query: &str, github_token: Option<&str>) -> FetchResult {
    let q = query.trim();
    if q.is_empty() {
        return Err("Empty query".into());
    }
    let http = Http::new(github_token);
    match (source, kind) {
        (Source::GitHub, Kind::User) => github_user(&http, q),
        (Source::GitHub, Kind::Package) => github_repo(&http, q),
        (Source::Npm, Kind::User) => npm_user(&http, q),
        (Source::Npm, Kind::Package) => npm_package(&http, q),
        (Source::PyPI, Kind::User) => pypi_user(&http, q),
        (Source::PyPI, Kind::Package) => pypi_package(&http, q),
        (Source::Crates, Kind::User) => crates_user(&http, q),
        (Source::Crates, Kind::Package) => crates_package(&http, q),
    }
}

// ---------------------------------------------------------------------------
// HTTP

struct Http {
    agent: ureq::Agent,
    github_token: Option<String>,
}

impl Http {
    fn new(github_token: Option<&str>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build();
        Self {
            agent,
            github_token: github_token.filter(|t| !t.is_empty()).map(str::to_owned),
        }
    }

    fn get_json(&self, url: &str) -> Result<Value, String> {
        // Short-lived cache: going Back, drilling in and Following refreshes reuse responses
        // instead of spending the registries' rate limits again.
        const TTL: Duration = Duration::from_secs(10 * 60);
        static CACHE: LazyLock<Mutex<HashMap<String, (Instant, Value)>>> = LazyLock::new(Default::default);
        if let Some((at, v)) = CACHE.lock().unwrap().get(url)
            && at.elapsed() < TTL
        {
            return Ok(v.clone());
        }
        let v = self.get_json_uncached(url)?;
        CACHE.lock().unwrap().insert(url.to_string(), (Instant::now(), v.clone()));
        Ok(v)
    }

    fn get_json_uncached(&self, url: &str) -> Result<Value, String> {
        let is_github = url.starts_with("https://api.github.com/");
        // npm's downloads API and pypistats throttle bursts with 429; back off and retry.
        let mut attempt = 0;
        let result = loop {
            let mut req = self.agent.get(url).set("Accept", "application/json");
            if is_github {
                req = req.set("Accept", "application/vnd.github+json");
                if let Some(token) = &self.github_token {
                    req = req.set("Authorization", &format!("Bearer {token}"));
                }
            }
            match req.call() {
                Err(ureq::Error::Status(429, resp)) if !is_github && attempt < 4 => {
                    attempt += 1;
                    // npm answers `Retry-After: 0` while still blocking, so never wait less than 2^n s.
                    let hinted = resp.header("retry-after").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                    std::thread::sleep(Duration::from_secs(hinted.max(1 << attempt).min(20)));
                }
                other => break other,
            }
        };
        match result {
            Ok(resp) => resp.into_json::<Value>().map_err(|e| format!("Bad JSON from {url}: {e}")),
            Err(ureq::Error::Status(404, _)) => Err("Not found (404)".into()),
            Err(ureq::Error::Status(403 | 429, resp)) if url.contains("api.github.com") => {
                let reset = resp.header("x-ratelimit-reset").unwrap_or("?").to_owned();
                Err(format!(
                    "GitHub rate limit hit (resets at unix time {reset}). Set a token in Settings or the GITHUB_TOKEN env var."
                ))
            }
            Err(ureq::Error::Status(429, _)) => Err(RATE_LIMITED.into()),
            Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code} from {url}")),
            Err(e) => Err(format!("Network error: {e}")),
        }
    }

    fn post_xml(&self, url: &str, body: &str) -> Result<String, String> {
        self.agent
            .post(url)
            .set("Content-Type", "text/xml")
            .send_string(body)
            .map_err(|e| format!("Request to {url} failed: {e}"))?
            .into_string()
            .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Helpers

fn s(v: &Value, key: &str) -> String {
    match &v[key] {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn n(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or(0.0)
}

pub fn fmt_num(x: f64) -> String {
    let x = x.round() as i64;
    let digits = x.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if x < 0 { format!("-{out}") } else { out }
}

pub fn fmt_compact(x: f64) -> String {
    let a = x.abs();
    if a >= 1e9 {
        format!("{:.1}B", x / 1e9)
    } else if a >= 1e6 {
        format!("{:.1}M", x / 1e6)
    } else if a >= 1e3 {
        format!("{:.1}K", x / 1e3)
    } else {
        format!("{}", x.round())
    }
}

fn metric(label: &str, value: impl Into<String>) -> Metric {
    let value = value.into();
    let is_count = !value.is_empty() && value.chars().all(|c| c.is_ascii_digit() || c == ',');
    let num = is_count.then(|| value.replace(',', "").parse().ok()).flatten();
    Metric { label: label.into(), value, num }
}

fn date_only(iso: &str) -> String {
    iso.get(..10).unwrap_or(iso).to_string()
}

fn top_bars(items: &[Item], title: &str) -> Option<Chart> {
    let bars: Vec<(String, f64)> = items
        .iter()
        .filter(|i| i.sort_key > 0.0)
        .take(15)
        .map(|i| (i.name.clone(), i.sort_key))
        .collect();
    (!bars.is_empty()).then(|| Chart::Bars { title: title.into(), bars })
}

fn sort_items(items: &mut [Item]) {
    items.sort_by(|a, b| b.sort_key.total_cmp(&a.sort_key));
}

/// Percent-encode a path segment (keeps it simple: only unreserved chars pass).
fn enc(segment: &str) -> String {
    let mut out = String::new();
    for b in segment.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// GitHub

fn github_user(http: &Http, user: &str) -> FetchResult {
    let u = http.get_json(&format!("https://api.github.com/users/{}", enc(user)))?;

    let mut repos = Vec::new();
    for page in 1..=5 {
        let batch = http.get_json(&format!(
            "https://api.github.com/users/{}/repos?per_page=100&type=owner&page={page}",
            enc(user)
        ))?;
        let batch = batch.as_array().cloned().unwrap_or_default();
        let done = batch.len() < 100;
        repos.extend(batch);
        if done {
            break;
        }
    }

    let stars: f64 = repos.iter().map(|r| n(r, "stargazers_count")).sum();
    let forks: f64 = repos.iter().map(|r| n(r, "forks_count")).sum();

    let mut items: Vec<Item> = repos
        .iter()
        .filter(|r| !r["fork"].as_bool().unwrap_or(false))
        .map(|r| Item {
            name: s(r, "name"),
            description: s(r, "description"),
            url: s(r, "html_url"),
            values: vec![
                fmt_num(n(r, "stargazers_count")),
                fmt_num(n(r, "forks_count")),
                s(r, "language"),
                date_only(&s(r, "pushed_at")),
            ],
            sort_key: n(r, "stargazers_count"),
        })
        .collect();
    sort_items(&mut items);

    let name = s(&u, "name");
    let login = s(&u, "login");
    let mut subtitle = s(&u, "bio");
    for extra in [s(&u, "company"), s(&u, "location")] {
        if !extra.is_empty() {
            if !subtitle.is_empty() {
                subtitle.push_str("  ·  ");
            }
            subtitle.push_str(&extra);
        }
    }

    Ok(Report {
        title: if name.is_empty() { login.clone() } else { format!("{name} (@{login})") },
        subtitle,
        url: s(&u, "html_url"),
        metrics: vec![
            metric("Followers", fmt_num(n(&u, "followers"))),
            metric("Following", fmt_num(n(&u, "following"))),
            metric("Public repos", fmt_num(n(&u, "public_repos"))),
            metric("Total stars", fmt_num(stars)),
            metric("Total forks", fmt_num(forks)),
            metric("Joined", date_only(&s(&u, "created_at"))),
        ],
        charts: top_bars(&items, "Top repositories by stars").into_iter().collect(),
        columns: vec!["Stars".into(), "Forks".into(), "Language".into(), "Last push".into()],
        items,
        note: (repos.len() >= 500).then(|| "Only the first 500 repositories were counted.".into()),
    })
}

fn github_repo(http: &Http, query: &str) -> FetchResult {
    let full = query
        .trim_start_matches("https://github.com/")
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let (owner, repo) = full
        .split_once('/')
        .ok_or("For a GitHub repository, search for owner/repo (e.g. rust-lang/rust).")?;
    let base = format!("https://api.github.com/repos/{}/{}", enc(owner), enc(repo));
    let r = http.get_json(&base)?;

    let languages = http.get_json(&format!("{base}/languages")).unwrap_or(Value::Null);
    let mut bars: Vec<(String, f64)> = languages
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))).collect())
        .unwrap_or_default();
    bars.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f64 = bars.iter().map(|b| b.1).sum();
    if total > 0.0 {
        for b in &mut bars {
            b.1 = b.1 / total * 100.0;
        }
    }
    bars.truncate(12);

    let release = http
        .get_json(&format!("{base}/releases/latest"))
        .map(|rel| s(&rel, "tag_name"))
        .unwrap_or_default();

    let license = r["license"]["spdx_id"].as_str().unwrap_or("—").to_string();
    let mut metrics = vec![
        metric("Stars", fmt_num(n(&r, "stargazers_count"))),
        metric("Forks", fmt_num(n(&r, "forks_count"))),
        metric("Watchers", fmt_num(n(&r, "subscribers_count"))),
        metric("Open issues + PRs", fmt_num(n(&r, "open_issues_count"))),
        metric("Language", s(&r, "language")),
        metric("License", license),
        metric("Created", date_only(&s(&r, "created_at"))),
        metric("Last push", date_only(&s(&r, "pushed_at"))),
    ];
    if !release.is_empty() {
        metrics.push(metric("Latest release", release));
    }

    Ok(Report {
        title: s(&r, "full_name"),
        subtitle: s(&r, "description"),
        url: s(&r, "html_url"),
        metrics,
        charts: (!bars.is_empty()).then(|| Chart::Bars { title: "Languages (% of code)".into(), bars }).into_iter().collect(),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// npm

fn npm_user(http: &Http, user: &str) -> FetchResult {
    let user = user.trim_start_matches('~').to_lowercase();
    let mut objects = Vec::new();
    let mut total = 0;
    // The search API caps `size` at 250; page with `from`.
    for from in (0..1000).step_by(250) {
        let res = http.get_json(&format!(
            "https://registry.npmjs.org/-/v1/search?text=maintainer:{}&size=250&from={from}",
            enc(&user)
        ))?;
        let batch = res["objects"].as_array().cloned().unwrap_or_default();
        total = n(&res, "total") as usize;
        objects.extend(batch);
        if objects.len() >= total || from + 250 >= total {
            break;
        }
    }
    // The search is fuzzy-ish; keep packages that really list this user as a maintainer.
    let maintained = |o: &Value| {
        o["package"]["maintainers"]
            .as_array()
            .is_some_and(|ms| ms.iter().any(|m| s(m, "username").eq_ignore_ascii_case(&user)))
    };
    if objects.iter().any(maintained) {
        objects.retain(maintained);
    }
    if objects.is_empty() {
        return Err(format!("No npm packages found for maintainer \"{user}\"."));
    }

    // Download numbers embedded in search results are cached and often stale,
    // so take the real last-30-days series from the downloads API.
    let names: Vec<String> = objects.iter().map(|o| s(&o["package"], "name")).collect();
    let daily = npm_daily_downloads(http, &names);
    let missing = names.iter().filter(|n| !daily.contains_key(*n)).count();

    // Packages the downloads API didn't answer for (rate limit) fall back to the
    // search result's cached counts rather than showing 0.
    let cached: HashMap<String, (f64, f64)> = objects
        .iter()
        .map(|o| (s(&o["package"], "name"), (n(&o["downloads"], "weekly"), n(&o["downloads"], "monthly"))))
        .collect();
    let window = |name: &str, days: usize| -> f64 {
        match daily.get(name) {
            Some(d) => d.iter().rev().take(days).map(|p| p.1).sum(),
            None => cached.get(name).map_or(0.0, |c| if days <= 7 { c.0 } else { c.1 }),
        }
    };

    let mut items: Vec<Item> = objects
        .iter()
        .map(|o| {
            let p = &o["package"];
            let name = s(p, "name");
            let (weekly, monthly) = (window(&name, 7), window(&name, 30));
            Item {
                description: s(p, "description"),
                url: format!("https://www.npmjs.com/package/{name}"),
                values: vec![fmt_num(weekly), fmt_num(monthly), s(p, "version"), date_only(&s(p, "date"))],
                sort_key: monthly,
                name,
            }
        })
        .collect();
    sort_items(&mut items);

    // Sum every package's series into one "all packages" line.
    let mut by_day: BTreeMap<String, f64> = BTreeMap::new();
    for series in daily.values() {
        for (day, v) in series {
            *by_day.entry(day.clone()).or_default() += v;
        }
    }
    let total_daily: Vec<(String, f64)> = by_day.into_iter().collect();
    let monthly: f64 = names.iter().map(|n| window(n, 30)).sum();
    let weekly: f64 = names.iter().map(|n| window(n, 7)).sum();

    let mut notes = Vec::new();
    if total > items.len() && objects.len() >= 1000 {
        notes.push(format!("npm search returns at most 1,000 results; showing {} of {total}.", items.len()));
    }
    if missing > 0 {
        notes.push(format!(
            "npm's downloads API is rate-limiting, so {missing} package(s) show npm search's cached counts              (may be a few days old). Search again in a minute for exact numbers."
        ));
    }

    let mut charts = Vec::new();
    if !total_daily.is_empty() {
        let title = if missing == 0 {
            "Daily downloads, all packages (last 30 days)".to_string()
        } else {
            format!("Daily downloads, {} of {} packages (last 30 days)", daily.len(), names.len())
        };
        charts.push(Chart::Daily { title, points: total_daily });
    }
    charts.extend(top_bars(&items, "Top packages by monthly downloads"));

    Ok(Report {
        title: format!("~{user} on npm"),
        subtitle: "Packages maintained by this user".into(),
        url: format!("https://www.npmjs.com/~{user}"),
        metrics: vec![
            metric("Packages", fmt_num(items.len() as f64)),
            metric("Weekly downloads", fmt_num(weekly)),
            metric("Monthly downloads", fmt_num(monthly)),
            metric("Top package", items.first().map(|i| i.name.clone()).unwrap_or_default()),
        ],
        charts,
        columns: vec!["Weekly".into(), "Monthly".into(), "Version".into(), "Published".into()],
        note: (!notes.is_empty()).then(|| notes.join(" ")),
        items,
    })
}

/// Last-30-days daily downloads per package from api.npmjs.org.
/// Unscoped names go in bulk requests (max 128); scoped ones must be fetched one by one.
fn npm_daily_downloads(http: &Http, names: &[String]) -> BTreeMap<String, Vec<(String, f64)>> {
    fn parse_series(v: &Value) -> Option<Vec<(String, f64)>> {
        let arr = v["downloads"].as_array()?;
        Some(arr.iter().map(|d| (s(d, "day"), n(d, "downloads"))).collect())
    }

    let (scoped, plain): (Vec<&String>, Vec<&String>) = names.iter().partition(|n| n.starts_with('@'));
    let mut out = BTreeMap::new();
    // Once a request is still rate-limited after retries, stop; callers fall back.
    let blocked = std::sync::atomic::AtomicBool::new(false);
    let fetch = |url: &str| -> Option<Value> {
        if blocked.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        match http.get_json(url) {
            Ok(v) => Some(v),
            Err(e) => {
                if e == RATE_LIMITED {
                    blocked.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                None
            }
        }
    };

    for chunk in plain.chunks(128) {
        let list = chunk.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(",");
        let Some(res) = fetch(&format!("https://api.npmjs.org/downloads/range/last-month/{list}")) else {
            continue;
        };
        if chunk.len() == 1 {
            // A single name returns the plain (non-bulk) shape.
            if let Some(series) = parse_series(&res) {
                out.insert(chunk[0].clone(), series);
            }
        } else if let Some(map) = res.as_object() {
            for (name, v) in map {
                if let Some(series) = parse_series(v) {
                    out.insert(name.clone(), series);
                }
            }
        }
    }

    // Scoped packages: a few worker threads over a shared queue.
    const MAX_SCOPED: usize = 300;
    let queue = std::sync::Mutex::new(scoped.into_iter().take(MAX_SCOPED));
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..3 {
            scope.spawn(|| {
                loop {
                    let Some(name) = queue.lock().unwrap().next() else { break };
                    let url = format!("https://api.npmjs.org/downloads/range/last-month/{name}");
                    if let Some(series) = fetch(&url).as_ref().and_then(parse_series) {
                        results.lock().unwrap().push((name.clone(), series));
                    }
                }
            });
        }
    });
    out.extend(results.into_inner().unwrap());
    out
}

fn npm_package(http: &Http, pkg: &str) -> FetchResult {
    // Scoped names (@scope/name) keep their slash on the registry and the downloads API.
    let path = pkg.replace(' ', "").to_lowercase();
    let latest = http.get_json(&format!("https://registry.npmjs.org/{path}/latest"))?;

    let (points, totals, note) = match http.get_json(&format!("https://api.npmjs.org/downloads/range/last-year/{path}")) {
        Ok(range) => {
            let points: Vec<(String, f64)> = range["downloads"]
                .as_array()
                .map(|a| a.iter().map(|d| (s(d, "day"), n(d, "downloads"))).collect())
                .unwrap_or_default();
            let sum_last = |days: usize| -> f64 { points.iter().rev().take(days).map(|p| p.1).sum() };
            let totals = [points.last().map_or(0.0, |p| p.1), sum_last(7), sum_last(30), sum_last(365)];
            (points, totals, None)
        }
        Err(range_err) => {
            // The point endpoints are CDN-cached and usually still answer while ranges are throttled.
            let point = |period: &str| {
                http.get_json(&format!("https://api.npmjs.org/downloads/point/{period}/{path}"))
                    .map(|v| n(&v, "downloads"))
            };
            let totals = [point("last-day"), point("last-week"), point("last-month"), point("last-year")];
            if totals.iter().all(Result::is_err) {
                return Err(format!("npm downloads: {range_err}"));
            }
            let totals = totals.map(|t| t.unwrap_or(0.0));
            (Vec::new(), totals, Some(format!("Daily chart unavailable: {range_err}")))
        }
    };

    let deps = latest["dependencies"].as_object().map_or(0, |m| m.len());
    let maintainers = latest["maintainers"].as_array().map_or(0, |a| a.len());

    Ok(Report {
        title: format!("{} @ {}", s(&latest, "name"), s(&latest, "version")),
        subtitle: s(&latest, "description"),
        url: format!("https://www.npmjs.com/package/{path}"),
        metrics: vec![
            metric("Last day", fmt_num(totals[0])),
            metric("Last 7 days", fmt_num(totals[1])),
            metric("Last 30 days", fmt_num(totals[2])),
            metric("Last 365 days", fmt_num(totals[3])),
            metric("License", s(&latest, "license")),
            metric("Dependencies", deps.to_string()),
            metric("Maintainers", maintainers.to_string()),
        ],
        charts: (!points.is_empty())
            .then(|| Chart::Daily { title: "Daily downloads (last year)".into(), points })
            .into_iter()
            .collect(),
        note,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// PyPI

/// Normalised project name as used by pypistats (PEP 503).
fn pep503(name: &str) -> String {
    let mut out = String::new();
    let mut last_sep = false;
    for c in name.chars() {
        if c == '-' || c == '_' || c == '.' {
            if !last_sep {
                out.push('-');
            }
            last_sep = true;
        } else {
            out.extend(c.to_lowercase());
            last_sep = false;
        }
    }
    out
}

fn pypi_user(http: &Http, user: &str) -> FetchResult {
    // pypi.org/user/<name> is behind a bot check, but XML-RPC `user_packages` still works.
    let body = format!(
        "<?xml version=\"1.0\"?><methodCall><methodName>user_packages</methodName>\
         <params><param><value><string>{}</string></value></param></params></methodCall>",
        user.replace('&', "&amp;").replace('<', "&lt;")
    );
    let xml = http.post_xml("https://pypi.org/pypi", &body)?;
    if xml.contains("<fault>") {
        return Err("PyPI returned an error for this user.".into());
    }
    // Response is an array of [role, package] string pairs.
    let strings: Vec<String> = xml
        .split("<string>")
        .skip(1)
        .filter_map(|chunk| chunk.split("</string>").next())
        .map(|x| x.replace("&amp;", "&"))
        .collect();
    let mut packages: Vec<(String, String)> = strings
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[1].clone(), c[0].clone()))
        .collect();
    packages.sort();
    packages.dedup_by(|a, b| a.0 == b.0);
    if packages.is_empty() {
        return Err(format!("No PyPI packages found for user \"{user}\" (usernames are case-sensitive)."));
    }

    const LIMIT: usize = 40;
    let mut items = Vec::new();
    let mut weekly_total = 0.0;
    for (name, role) in packages.iter().take(LIMIT) {
        let recent = http
            .get_json(&format!("https://pypistats.org/api/packages/{}/recent", pep503(name)))
            .ok();
        let d = recent.as_ref().map(|r| &r["data"]);
        let week = d.map_or(0.0, |d| n(d, "last_week"));
        let month = d.map_or(0.0, |d| n(d, "last_month"));
        weekly_total += week;
        items.push(Item {
            name: name.clone(),
            description: String::new(),
            url: format!("https://pypi.org/project/{name}/"),
            values: vec![
                if recent.is_some() { fmt_num(week) } else { "—".into() },
                if recent.is_some() { fmt_num(month) } else { "—".into() },
                role.clone(),
            ],
            sort_key: month,
        });
    }
    sort_items(&mut items);
    let monthly: f64 = items.iter().map(|i| i.sort_key).sum();
    let weekly: f64 = weekly_total;

    Ok(Report {
        title: format!("{user} on PyPI"),
        subtitle: "Packages this user owns or maintains".into(),
        url: format!("https://pypi.org/user/{user}/"),
        metrics: vec![
            metric("Packages", fmt_num(packages.len() as f64)),
            metric("Weekly downloads", fmt_num(weekly)),
            metric("Monthly downloads", fmt_num(monthly)),
            metric("Top package", items.first().map(|i| i.name.clone()).unwrap_or_default()),
        ],
        charts: top_bars(&items, "Top packages by monthly downloads").into_iter().collect(),
        columns: vec!["Weekly".into(), "Monthly".into(), "Role".into()],
        note: (packages.len() > LIMIT)
            .then(|| format!("Download stats fetched for the first {LIMIT} of {} packages.", packages.len())),
        items,
    })
}

fn pypi_package(http: &Http, pkg: &str) -> FetchResult {
    let meta = http.get_json(&format!("https://pypi.org/pypi/{}/json", enc(pkg)))?;
    let info = &meta["info"];
    let norm = pep503(&s(info, "name"));

    let recent = http.get_json(&format!("https://pypistats.org/api/packages/{norm}/recent"));
    let overall = http.get_json(&format!("https://pypistats.org/api/packages/{norm}/overall?mirrors=false"));

    let mut points: Vec<(String, f64)> = overall
        .as_ref()
        .ok()
        .and_then(|o| o["data"].as_array())
        .map(|a| {
            a.iter()
                .filter(|d| d["category"] == "without_mirrors")
                .map(|d| (s(d, "date"), n(d, "downloads")))
                .collect()
        })
        .unwrap_or_default();
    points.sort_by(|a, b| a.0.cmp(&b.0));

    let mut metrics = Vec::new();
    match &recent {
        Ok(r) => {
            let d = &r["data"];
            metrics.push(metric("Last day", fmt_num(n(d, "last_day"))));
            metrics.push(metric("Last week", fmt_num(n(d, "last_week"))));
            metrics.push(metric("Last month", fmt_num(n(d, "last_month"))));
        }
        Err(_) => metrics.push(metric("Downloads", "unavailable")),
    }
    let releases = meta["releases"].as_object().map_or(0, |m| m.len());
    let license = {
        let expr = s(info, "license_expression");
        let lic = s(info, "license");
        if !expr.is_empty() {
            expr
        } else if !lic.is_empty() && lic.len() < 40 {
            lic
        } else {
            "—".into()
        }
    };
    metrics.extend([
        metric("Releases", releases.to_string()),
        metric("License", license),
        metric("Requires Python", s(info, "requires_python")),
        metric("Author", {
            let a = s(info, "author");
            if a.is_empty() { s(info, "author_email") } else { a }
        }),
    ]);

    Ok(Report {
        title: format!("{} {}", s(info, "name"), s(info, "version")),
        subtitle: s(info, "summary"),
        url: s(info, "package_url"),
        metrics,
        charts: (!points.is_empty())
            .then(|| Chart::Daily { title: "Daily downloads, without mirrors (≈ last 180 days)".into(), points })
            .into_iter()
            .collect(),
        note: recent.err().map(|e| format!("pypistats.org: {e}")),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// crates.io

fn crates_user(http: &Http, user: &str) -> FetchResult {
    let u = http.get_json(&format!("https://crates.io/api/v1/users/{}", enc(user)))?;
    let u = &u["user"];
    let id = n(u, "id") as u64;

    let mut crates = Vec::new();
    for page in 1..=5 {
        let res = http.get_json(&format!(
            "https://crates.io/api/v1/crates?user_id={id}&per_page=100&page={page}&sort=downloads"
        ))?;
        let batch = res["crates"].as_array().cloned().unwrap_or_default();
        let done = batch.len() < 100;
        crates.extend(batch);
        if done {
            break;
        }
    }

    let mut items: Vec<Item> = crates
        .iter()
        .map(|c| Item {
            name: s(c, "name"),
            description: s(c, "description").trim().to_string(),
            url: format!("https://crates.io/crates/{}", s(c, "name")),
            values: vec![
                fmt_num(n(c, "downloads")),
                fmt_num(n(c, "recent_downloads")),
                {
                    let v = s(c, "default_version");
                    if v.is_empty() { s(c, "max_version") } else { v }
                },
                date_only(&s(c, "updated_at")),
            ],
            sort_key: n(c, "downloads"),
        })
        .collect();
    sort_items(&mut items);

    let total: f64 = crates.iter().map(|c| n(c, "downloads")).sum();
    let recent: f64 = crates.iter().map(|c| n(c, "recent_downloads")).sum();
    let name = s(u, "name");

    Ok(Report {
        title: if name.is_empty() { s(u, "login") } else { format!("{name} ({})", s(u, "login")) },
        subtitle: "Crates owned by this user".into(),
        url: format!("https://crates.io/users/{}", s(u, "login")),
        metrics: vec![
            metric("Crates", fmt_num(crates.len() as f64)),
            metric("All-time downloads", fmt_num(total)),
            metric("Last 90 days", fmt_num(recent)),
            metric("Top crate", items.first().map(|i| i.name.clone()).unwrap_or_default()),
        ],
        charts: top_bars(&items, "Top crates by all-time downloads").into_iter().collect(),
        columns: vec!["All-time".into(), "Last 90 days".into(), "Version".into(), "Updated".into()],
        items,
        note: None,
    })
}

fn crates_package(http: &Http, name: &str) -> FetchResult {
    let res = http.get_json(&format!("https://crates.io/api/v1/crates/{}", enc(name)))?;
    let c = &res["crate"];
    let dl = http.get_json(&format!("https://crates.io/api/v1/crates/{}/downloads", enc(name)))?;

    // Sum per-version downloads plus "extra" (old versions) by date.
    let mut by_day: BTreeMap<String, f64> = BTreeMap::new();
    for d in dl["version_downloads"].as_array().into_iter().flatten() {
        *by_day.entry(s(d, "date")).or_default() += n(d, "downloads");
    }
    for d in dl["meta"]["extra_downloads"].as_array().into_iter().flatten() {
        *by_day.entry(s(d, "date")).or_default() += n(d, "downloads");
    }
    let points: Vec<(String, f64)> = by_day.into_iter().collect();

    let versions = res["versions"].as_array().map_or(0, |a| a.len());
    let version = {
        let v = s(c, "default_version");
        if v.is_empty() { s(c, "max_version") } else { v }
    };

    Ok(Report {
        title: format!("{} {}", s(c, "name"), version),
        subtitle: s(c, "description").trim().to_string(),
        url: format!("https://crates.io/crates/{}", s(c, "name")),
        metrics: vec![
            metric("All-time downloads", fmt_num(n(c, "downloads"))),
            metric("Last 90 days", fmt_num(n(c, "recent_downloads"))),
            metric("Versions", versions.to_string()),
            metric("Created", date_only(&s(c, "created_at"))),
            metric("Updated", date_only(&s(c, "updated_at"))),
        ],
        charts: (!points.is_empty()).then(|| Chart::Daily { title: "Daily downloads (last 90 days)".into(), points }).into_iter().collect(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers() {
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(1234567.0), "1,234,567");
        assert_eq!(fmt_compact(1_300_000.0), "1.3M");
    }

    /// Hits the real APIs: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_fetchers() {
        let token = std::env::var("GITHUB_TOKEN").ok();
        let cases = [
            (Source::GitHub, Kind::User, "dtolnay"),
            (Source::GitHub, Kind::Package, "rust-lang/rust"),
            (Source::Npm, Kind::User, "sindresorhus"),
            (Source::Npm, Kind::Package, "@types/node"),
            (Source::PyPI, Kind::User, "kennethreitz"),
            (Source::PyPI, Kind::Package, "requests"),
            (Source::Crates, Kind::User, "dtolnay"),
            (Source::Crates, Kind::Package, "serde"),
        ];
        for (source, kind, q) in cases {
            let r = fetch(source, kind, q, token.as_deref()).unwrap_or_else(|e| panic!("{source:?} {kind:?} {q}: {e}"));
            let chart: Vec<String> = r
                .charts
                .iter()
                .map(|c| match c {
                    Chart::Daily { points, .. } => format!("daily x{}", points.len()),
                    Chart::Bars { bars, .. } => format!("bars x{}", bars.len()),
                })
                .collect();
            let chart = chart.join("+");
            let m: Vec<String> = r.metrics.iter().take(3).map(|m| format!("{}={}", m.label, m.value)).collect();
            println!("{source:?}/{kind:?} {q}: {} | {} | chart {chart} | items {}", r.title, m.join(", "), r.items.len());
        }
    }

    #[test]
    fn normalises_pypi_names() {
        assert_eq!(pep503("Django_REST.framework"), "django-rest-framework");
    }
}
