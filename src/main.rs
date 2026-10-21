#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod history;
mod icons;
mod track;

use api::{Chart, FetchResult, Kind, Report, Source};
use history::History;
use eframe::egui::{self, Color32, RichText};
use egui_plot::{Line, Plot, PlotPoints};
use icons::Icon;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use track::{Followed, Store};

fn main() -> eframe::Result {
    // `pkgstats --snapshot` records today's numbers for everything followed and exits.
    // The optional scheduled task (Settings) runs this so history fills in while the app is closed.
    if std::env::args().any(|a| a == "--snapshot") {
        snapshot_followed();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PkgStats")
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([680.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native("PkgStats", options, Box::new(|_cc| Ok(Box::new(App::default()))))
}

fn snapshot_followed() {
    let mut store = Store::load();
    let token = std::env::var("GITHUB_TOKEN").ok();
    for f in store.stale() {
        match api::fetch(f.source, f.kind, &f.query, token.as_deref()) {
            Ok(report) => {
                store.record(&f.key(), &report);
            }
            Err(e) => store.record_error(&f.key(), e),
        }
    }
    let _ = store.save();
}

const ACCENT: Color32 = Color32::from_rgb(96, 165, 250);
/// How often the running app checks whether followed items need today's snapshot.
const AUTO_REFRESH_EVERY: Duration = Duration::from_secs(30 * 60);
const TASK_NAME: &str = "PkgStats daily snapshot";

enum TabState {
    Loading,
    Done(Report),
    Failed(String),
}

struct Tab {
    source: Source,
    state: TabState,
}

#[derive(Clone)]
struct Search {
    kind: Kind,
    query: String,
    sources: [bool; 4],
}

#[derive(PartialEq)]
enum View {
    Search,
    Following,
    History,
}

enum Msg {
    /// (search generation, source, result)
    Search(u64, Source, FetchResult),
    /// (followed key, result, remove the entry if it fails)
    Follow(String, FetchResult, bool),
}

/// Something the user clicked inside a report or the Following page.
enum Action {
    Search(Search),
    ToggleFollow(Source),
    Unfollow(String),
    Refresh(Vec<Followed>),
    SelectMetric(String, String),
    FollowMe,
    RemoveHistory(usize),
    ClearHistory,
}

struct App {
    view: View,
    query: String,
    kind: Kind,
    sources: [bool; 4],
    tabs: Vec<Tab>,
    selected: usize,
    generation: u64,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    back_stack: Vec<Search>,
    current: Option<Search>,
    github_token: String,
    show_settings: bool,
    filter: String,
    store: Store,
    /// Keys of followed entries with a fetch in flight.
    refreshing: HashSet<String>,
    /// Followed key -> metric shown in its history chart.
    chart_metric: HashMap<String, String>,
    last_auto_refresh: Option<Instant>,
    status: Option<(String, bool)>,
    sidebar_open: bool,
    history: History,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = channel();
        let store = Store::load();
        Self {
            view: if store.followed.is_empty() { View::Search } else { View::Following },
            query: String::new(),
            kind: Kind::User,
            sources: [true; 4],
            tabs: Vec::new(),
            selected: 0,
            generation: 0,
            tx,
            rx,
            back_stack: Vec::new(),
            current: None,
            github_token: std::env::var("GITHUB_TOKEN").unwrap_or_default(),
            show_settings: false,
            filter: String::new(),
            store,
            refreshing: HashSet::new(),
            chart_metric: HashMap::new(),
            last_auto_refresh: None,
            status: None,
            sidebar_open: false,
            history: History::load(),
        }
    }
}

impl App {
    fn start_search(&mut self, ctx: &egui::Context, search: Search, push_history: bool) {
        if search.query.trim().is_empty() || !search.sources.iter().any(|&on| on) {
            return;
        }
        if push_history && let Some(prev) = self.current.take() {
            self.back_stack.push(prev);
        }
        self.view = View::Search;
        self.generation += 1;
        self.query = search.query.clone();
        self.kind = search.kind;
        self.sources = search.sources;
        self.selected = 0;
        self.filter.clear();
        self.tabs.clear();

        for (i, source) in Source::ALL.into_iter().enumerate() {
            if !search.sources[i] {
                continue;
            }
            self.tabs.push(Tab { source, state: TabState::Loading });
            let (tx, ctx, generation) = (self.tx.clone(), ctx.clone(), self.generation);
            let (kind, query, token) = (search.kind, search.query.clone(), self.github_token.clone());
            std::thread::spawn(move || {
                let result = api::fetch(source, kind, &query, Some(&token));
                let _ = tx.send(Msg::Search(generation, source, result));
                ctx.request_repaint();
            });
        }
        if push_history {
            self.history.add(search.kind, &search.query, search.sources);
            self.save_history();
        }
        self.current = Some(search);
    }

    fn refresh_followed(&mut self, ctx: &egui::Context, entries: Vec<Followed>, drop_on_error: bool) {
        for f in entries {
            let key = f.key();
            if !self.refreshing.insert(key.clone()) {
                continue;
            }
            let (tx, ctx, token) = (self.tx.clone(), ctx.clone(), self.github_token.clone());
            std::thread::spawn(move || {
                let result = api::fetch(f.source, f.kind, &f.query, Some(&token));
                let _ = tx.send(Msg::Follow(key, result, drop_on_error));
                ctx.request_repaint();
            });
        }
    }

    fn save_history(&mut self) {
        if let Err(e) = self.history.save() {
            self.status = Some((format!("Could not save history: {e}"), true));
        }
    }

    fn save_store(&mut self) {
        if let Err(e) = self.store.save() {
            self.status = Some((format!("Could not save followed list: {e}"), true));
        }
    }

    fn receive(&mut self) {
        let mut changed = false;
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Search(generation, source, result) => {
                    if generation != self.generation {
                        continue; // stale result from an older search
                    }
                    // Viewing something you follow also refreshes today's snapshot.
                    if let (Ok(report), Some(cur)) = (&result, &self.current) {
                        changed |= self.store.record(&track::key(source, cur.kind, &cur.query), report);
                    }
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.source == source) {
                        tab.state = match result {
                            Ok(report) => TabState::Done(report),
                            Err(e) => TabState::Failed(e),
                        };
                    }
                }
                Msg::Follow(key, result, drop_on_error) => {
                    self.refreshing.remove(&key);
                    changed = true;
                    match result {
                        Ok(report) => {
                            self.store.record(&key, &report);
                        }
                        Err(e) if drop_on_error => {
                            if let Some(f) = self.store.find(&key) {
                                self.status = Some((
                                    format!("Not following {} on {}: {e}", f.query, f.source.label()),
                                    false,
                                ));
                            }
                            self.store.unfollow(&key);
                        }
                        Err(e) => self.store.record_error(&key, e),
                    }
                }
            }
        }
        if changed {
            self.save_store();
        }
    }

    fn auto_refresh(&mut self, ctx: &egui::Context) {
        let due = self.last_auto_refresh.is_none_or(|t| t.elapsed() >= AUTO_REFRESH_EVERY);
        if due {
            self.last_auto_refresh = Some(Instant::now());
            let stale = self.store.stale();
            self.refresh_followed(ctx, stale, false);
        }
        // Wake up periodically even when idle, so a new day gets its snapshot.
        ctx.request_repaint_after(AUTO_REFRESH_EVERY);
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Search(search) => self.start_search(ctx, search, true),
            Action::ToggleFollow(source) => {
                let Some(cur) = self.current.clone() else { return };
                let key = track::key(source, cur.kind, &cur.query);
                if self.store.find(&key).is_some() {
                    self.store.unfollow(&key);
                } else {
                    self.store.follow(source, cur.kind, &cur.query);
                    let report = self.tabs.iter().find(|t| t.source == source).and_then(|t| match &t.state {
                        TabState::Done(r) => Some(r),
                        _ => None,
                    });
                    if let Some(report) = report {
                        self.store.record(&key, report);
                    }
                }
                self.save_store();
            }
            Action::Unfollow(key) => {
                self.store.unfollow(&key);
                self.save_store();
            }
            Action::Refresh(entries) => self.refresh_followed(ctx, entries, false),
            Action::RemoveHistory(index) => {
                self.history.remove(index);
                self.save_history();
            }
            Action::ClearHistory => {
                self.history.clear();
                self.save_history();
            }
            Action::SelectMetric(key, metric) => {
                self.chart_metric.insert(key, metric);
            }
            Action::FollowMe => {
                let name = self.store.my_username.trim().to_string();
                if name.is_empty() {
                    return;
                }
                let mut added = Vec::new();
                for source in Source::ALL {
                    if self.store.follow(source, Kind::User, &name) {
                        added.extend(self.store.find(&track::key(source, Kind::User, &name)).cloned());
                    }
                }
                self.save_store();
                if added.is_empty() {
                    self.status = Some((format!("Already following {name} everywhere."), false));
                }
                // Registries where the name doesn't exist get dropped again.
                self.refresh_followed(ctx, added, true);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Top bar

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut submit = false;

        ui.horizontal(|ui| {
            if icons::icon_button(ui, Icon::Menu, "Menu").clicked() {
                self.sidebar_open = !self.sidebar_open;
            }
            ui.add_enabled_ui(!self.back_stack.is_empty(), |ui| {
                if icons::icon_button(ui, Icon::Back, "Back").clicked()
                    && let Some(prev) = self.back_stack.pop()
                {
                    self.current = None;
                    self.start_search(&ctx, prev, false);
                }
            });
            if icons::button(ui, Some(Icon::Search), "Search", self.view == View::Search, None).clicked() {
                self.view = View::Search;
            }
            let label = format!("Following ({})", self.store.followed.len());
            if icons::button(ui, Some(Icon::Star), &label, self.view == View::Following, None).clicked() {
                self.view = View::Following;
            }
            ui.separator();

            if icons::button(ui, Some(Icon::User), "User", self.kind == Kind::User, None).clicked() {
                self.kind = Kind::User;
            }
            if icons::button(ui, Some(Icon::Package), "Package / repo", self.kind == Kind::Package, None).clicked() {
                self.kind = Kind::Package;
            }

            let hint = match self.kind {
                Kind::User => "username, e.g. sindresorhus",
                Kind::Package => "package name, or owner/repo for GitHub",
            };
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text(hint)
                    .desired_width((ui.available_width() - 120.0).max(140.0)),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            if icons::button(ui, Some(Icon::Search), "Go", false, Some(ACCENT)).clicked() {
                submit = true;
            }
            if icons::icon_button(ui, Icon::Settings, "Settings").clicked() {
                self.show_settings = !self.show_settings;
            }
        });

        if self.view == View::Search {
            ui.horizontal(|ui| {
                ui.label("Sources:");
                for (i, source) in Source::ALL.into_iter().enumerate() {
                    icons::source_badge(ui, source);
                    ui.checkbox(&mut self.sources[i], source.label());
                    ui.add_space(6.0);
                }
            });
        }

        if let Some((msg, is_error)) = self.status.clone() {
            ui.horizontal(|ui| {
                let color = if is_error { icons::RED } else { ui.visuals().text_color() };
                icons::inline(ui, Icon::Info, color);
                ui.label(RichText::new(msg).color(color));
                if icons::icon_button(ui, Icon::Close, "Dismiss").clicked() {
                    self.status = None;
                }
            });
        }

        if submit {
            let search = Search { kind: self.kind, query: self.query.clone(), sources: self.sources };
            self.start_search(&ctx, search, true);
        }
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, tab) in self.tabs.iter().enumerate() {
                let (icon, color) = match &tab.state {
                    TabState::Loading => (Icon::Dots, ui.visuals().weak_text_color()),
                    TabState::Done(_) => (Icon::Check, icons::GREEN),
                    TabState::Failed(_) => (Icon::Cross, icons::RED),
                };
                if icons::button(ui, Some(icon), tab.source.label(), self.selected == i, Some(color)).clicked() {
                    self.selected = i;
                }
            }
        });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new("Settings").open(&mut open).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("GitHub token").strong());
                icons::info(
                    ui,
                    "Optional. Raises GitHub's limit from 60 to 5,000 requests per hour. \
                     Defaults to the GITHUB_TOKEN environment variable. Not saved to disk.",
                );
            });
            ui.add(egui::TextEdit::singleline(&mut self.github_token).password(true).desired_width(320.0));

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Daily snapshots").strong());
                icons::info(
                    ui,
                    &format!(
                        "While PkgStats is open it records today's numbers for everything you follow. \
                         To keep recording when it's closed, add the background task: it checks hourly \
                         and fetches once a day.\n\nData file: {}",
                        Store::path().display()
                    ),
                );
            });
            if cfg!(windows) {
                ui.horizontal(|ui| {
                    if icons::button(ui, Some(Icon::Check), "Enable background task", false, None).clicked() {
                        self.status = Some(match install_task() {
                            Ok(()) => ("Background snapshot task added to Task Scheduler.".into(), false),
                            Err(e) => (format!("Could not add the task: {e}"), true),
                        });
                    }
                    if icons::button(ui, Some(Icon::Close), "Remove task", false, None).clicked() {
                        self.status = Some(match schtasks(&["/Delete", "/F", "/TN", TASK_NAME]) {
                            Ok(()) => ("Background snapshot task removed.".into(), false),
                            Err(e) => (format!("Could not remove the task: {e}"), true),
                        });
                    }
                });
            } else {
                ui.code("0 * * * * /path/to/pkgstats --snapshot");
            }
        });
        self.show_settings = open;
    }

    // -----------------------------------------------------------------------
    // Sidebar

    fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let heading = |ui: &mut egui::Ui, text: &str| {
            ui.add_space(10.0);
            ui.label(RichText::new(text).small().strong().weak());
            ui.add_space(2.0);
        };

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(6.0);
            if icons::nav_row(ui, Some(Icon::Search), None, "Search", self.view == View::Search).clicked() {
                self.view = View::Search;
            }
            let following = format!("Following ({})", self.store.followed.len());
            if icons::nav_row(ui, Some(Icon::Star), None, &following, self.view == View::Following).clicked() {
                self.view = View::Following;
            }
            if icons::nav_row(ui, Some(Icon::Clock), None, "History", self.view == View::History).clicked() {
                self.view = View::History;
            }
            if icons::nav_row(ui, Some(Icon::Settings), None, "Settings", self.show_settings).clicked() {
                self.show_settings = !self.show_settings;
            }

            if !self.store.followed.is_empty() {
                heading(ui, "FOLLOWED");
                for f in &self.store.followed {
                    let resp = icons::nav_row(ui, None, Some(f.source), &f.title, false);
                    if resp.on_hover_text(format!("{} · {}", f.source.label(), f.title)).clicked() {
                        let mut sources = [false; 4];
                        sources[Source::ALL.iter().position(|&s| s == f.source).unwrap_or(0)] = true;
                        action = Some(Action::Search(Search { kind: f.kind, query: f.query.clone(), sources }));
                    }
                }
            }

            if !self.history.entries.is_empty() {
                heading(ui, "HISTORY");
                for e in self.history.entries.iter().take(8) {
                    let icon = if e.kind == Kind::User { Icon::User } else { Icon::Package };
                    let resp = icons::nav_row(ui, Some(icon), None, &e.query, false);
                    if resp.on_hover_text(format!("{} · {}", e.at, site_list(e.sources))).clicked() {
                        action = Some(Action::Search(Search { kind: e.kind, query: e.query.clone(), sources: e.sources }));
                    }
                }
                if self.history.entries.len() > 8
                    && icons::nav_row(ui, Some(Icon::Dots), None, "Show all", false).clicked()
                {
                    self.view = View::History;
                }
            }
        });
        action
    }

    // -----------------------------------------------------------------------
    // Search results

    fn search_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let Some(tab) = self.tabs.get(self.selected) else {
            welcome(ui);
            return None;
        };
        let mut action = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match &tab.state {
            TabState::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Fetching {} stats…", tab.source.label()));
                });
            }
            TabState::Failed(e) => {
                ui.horizontal(|ui| {
                    icons::source_badge(ui, tab.source);
                    ui.label(RichText::new(format!("{} lookup failed", tab.source.label())).strong());
                });
                ui.label(RichText::new(e).color(icons::RED));
            }
            TabState::Done(report) => {
                let followed = self
                    .current
                    .as_ref()
                    .and_then(|c| self.store.find(&track::key(tab.source, c.kind, &c.query)));
                action = report_view(ui, tab.source, report, followed, &mut self.filter);
            }
        });
        action
    }

    // -----------------------------------------------------------------------
    // History page

    fn history_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("History").strong());
                icons::info(ui, "Your past searches (last 200). Click one to run it again.");
                ui.add_space(8.0);
                if !self.history.entries.is_empty()
                    && icons::button(ui, Some(Icon::Trash), "Clear all", false, None).clicked()
                {
                    action = Some(Action::ClearHistory);
                }
            });
            ui.add_space(8.0);

            if self.history.entries.is_empty() {
                ui.label(RichText::new("No searches yet.").weak());
                return;
            }

            let mut last_date = "";
            for (i, e) in self.history.entries.iter().enumerate() {
                if e.date() != last_date {
                    last_date = e.date();
                    ui.add_space(8.0);
                    ui.label(RichText::new(day_label(last_date)).small().strong().weak());
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(e.time()).monospace().weak());
                    let icon = if e.kind == Kind::User { Icon::User } else { Icon::Package };
                    icons::inline(ui, icon, ui.visuals().weak_text_color());
                    if ui.link(RichText::new(&e.query).color(ACCENT).strong()).on_hover_text("Search again").clicked() {
                        action = Some(Action::Search(Search { kind: e.kind, query: e.query.clone(), sources: e.sources }));
                    }
                    for (source, on) in Source::ALL.into_iter().zip(e.sources) {
                        if on {
                            icons::source_badge(ui, source);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::icon_button(ui, Icon::Close, "Remove from history").clicked() {
                            action = Some(Action::RemoveHistory(i));
                        }
                    });
                });
            }
        });
        action
    }

    // -----------------------------------------------------------------------
    // Following page

    fn following_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Following").strong());
                icons::info(
                    ui,
                    "A snapshot is saved once a day, so you can see how stars, followers and downloads change. \
                     Changes appear after the second day. Click a metric to see its history chart.",
                );
                ui.add_space(8.0);
                let busy = !self.refreshing.is_empty();
                if busy {
                    ui.spinner();
                    ui.label(RichText::new(format!("updating {}…", self.refreshing.len())).weak());
                } else if icons::button(ui, Some(Icon::Refresh), "Refresh now", false, None).clicked() {
                    action = Some(Action::Refresh(self.store.followed.clone()));
                }
            });
            ui.add_space(8.0);

            egui::Frame::group(ui.style()).corner_radius(8.0).inner_margin(10).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Your username").strong());
                    icons::info(
                        ui,
                        "Follows this name on GitHub, npm, PyPI and crates.io. Sites where it doesn't exist \
                         are skipped. Use the star on any report to follow other users and packages.",
                    );
                });
                ui.horizontal(|ui| {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.store.my_username)
                            .hint_text("the name you use on GitHub / npm / PyPI / crates.io")
                            .desired_width(300.0),
                    );
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if edit.lost_focus() {
                        let _ = self.store.save();
                    }
                    let clicked = icons::button(ui, Some(Icon::Star), "Follow me on all sites", false, Some(icons::GOLD))
                        .clicked();
                    if clicked || enter {
                        action = Some(Action::FollowMe);
                    }
                });
            });
            ui.add_space(10.0);

            if self.store.followed.is_empty() {
                ui.label(RichText::new("Nothing followed yet.").weak());
                return;
            }

            for f in &self.store.followed {
                let key = f.key();
                let metric = self.chart_metric.get(&key).cloned();
                if let Some(a) = followed_card(ui, f, metric, self.refreshing.contains(&key)) {
                    action = Some(a);
                }
                ui.add_space(8.0);
            }
        });
        action
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive();
        self.auto_refresh(&ctx);

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            self.top_bar(ui);
            if self.view == View::Search && !self.tabs.is_empty() {
                ui.separator();
                self.tab_strip(ui);
            }
            ui.add_space(4.0);
        });

        self.settings_window(&ctx);

        let mut action = None;
        // Borderless sidebar under the top bar, slides in from the left.
        let sidebar_frame = egui::Frame::side_top_panel(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .stroke(egui::Stroke::NONE)
            .inner_margin(egui::Margin::symmetric(8, 4));
        let mut open = self.sidebar_open;
        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(220.0)
            .show_separator_line(false)
            .frame(sidebar_frame)
            .show_collapsible(ui, &mut open, |ui| {
                action = self.sidebar(ui);
            });
        self.sidebar_open = open;

        egui::CentralPanel::default().show(ui, |ui| {
            let main_action = match self.view {
                View::Search => self.search_view(ui),
                View::Following => self.following_view(ui),
                View::History => self.history_view(ui),
            };
            action = action.take().or(main_action);
        });
        if let Some(action) = action {
            self.apply(&ctx, action);
        }
    }
}

// ---------------------------------------------------------------------------
// Views

fn report_view(
    ui: &mut egui::Ui,
    source: Source,
    report: &Report,
    followed: Option<&Followed>,
    filter: &mut String,
) -> Option<Action> {
    let mut action = None;

    ui.horizontal_wrapped(|ui| {
        icons::source_badge(ui, source);
        ui.heading(RichText::new(&report.title).strong());
        if !report.url.is_empty() && icons::button(ui, Some(Icon::External), "Open page", false, None).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(&report.url));
        }
        if !report.is_search {
            let (icon, text) = if followed.is_some() { (Icon::StarFilled, "Following") } else { (Icon::Star, "Follow") };
            let resp = icons::button(ui, Some(icon), text, followed.is_some(), Some(icons::GOLD))
                .on_hover_text("Save a snapshot every day to track changes");
            if resp.clicked() {
                action = Some(Action::ToggleFollow(source));
            }
        }
    });
    if !report.subtitle.is_empty() {
        ui.label(RichText::new(&report.subtitle).weak());
    }
    if let Some(note) = &report.note {
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Info, icons::GOLD);
            ui.label(RichText::new(note).small().color(icons::GOLD));
        });
    }
    ui.add_space(8.0);

    metric_cards(ui, report, followed);
    ui.add_space(10.0);

    for (i, chart) in report.charts.iter().enumerate() {
        if i > 0 {
            ui.add_space(12.0);
        }
        match chart {
            Chart::Daily { title, points } => daily_chart(ui, title, points, true),
            Chart::Bars { title, bars } => bar_rows(ui, title, bars),
        }
    }

    if !report.items.is_empty() {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("All items ({})", report.items.len())).strong());
            ui.add(egui::TextEdit::singleline(filter).hint_text("filter…").desired_width(180.0));
            icons::info(ui, "Click a name to see its stats, or the arrow next to it to open its page in your browser.");
        });
        ui.add_space(4.0);
        let needle = filter.to_lowercase();
        egui::Grid::new(("items", source.label())).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("Name").strong());
            ui.label("");
            for col in &report.columns {
                ui.label(RichText::new(col).strong());
            }
            ui.label(RichText::new("Description").strong());
            ui.end_row();

            for item in report.items.iter().filter(|i| {
                needle.is_empty()
                    || i.name.to_lowercase().contains(&needle)
                    || i.description.to_lowercase().contains(&needle)
            }) {
                if ui.link(RichText::new(&item.name).color(ACCENT)).on_hover_text("Show stats").clicked() {
                    action = Some(Action::Search(drill_down(source, report, &item.name)));
                }
                if icons::icon_button(ui, Icon::External, &item.url).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(&item.url));
                }
                for v in &item.values {
                    ui.label(v);
                }
                let desc: String = item.description.chars().take(90).collect();
                let more = item.description.chars().count() > 90;
                ui.label(RichText::new(if more { format!("{desc}…") } else { desc }).weak());
                ui.end_row();
            }
        });
    }
    action
}

fn followed_card(ui: &mut egui::Ui, f: &Followed, metric: Option<String>, busy: bool) -> Option<Action> {
    let mut action = None;
    let key = f.key();
    egui::Frame::group(ui.style()).corner_radius(8.0).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            icons::source_badge(ui, f.source);
            let kind_icon = if f.kind == Kind::User { Icon::User } else { Icon::Package };
            icons::inline(ui, kind_icon, ui.visuals().weak_text_color());
            if ui.link(RichText::new(&f.title).strong().size(15.0).color(ACCENT)).on_hover_text("Show full stats").clicked()
            {
                let mut sources = [false; 4];
                sources[Source::ALL.iter().position(|&s| s == f.source).unwrap_or(0)] = true;
                action = Some(Action::Search(Search { kind: f.kind, query: f.query.clone(), sources }));
            }
            if !f.url.is_empty() && icons::icon_button(ui, Icon::External, "Open page").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(&f.url));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::icon_button(ui, Icon::Close, "Unfollow").clicked() {
                    action = Some(Action::Unfollow(key.clone()));
                }
                if busy {
                    ui.spinner();
                } else if icons::icon_button(ui, Icon::Refresh, "Refresh").clicked() {
                    action = Some(Action::Refresh(vec![f.clone()]));
                }
                if let Some((date, _)) = f.latest() {
                    let first = f.history.keys().next().cloned().unwrap_or_default();
                    ui.label(
                        RichText::new(format!("{} snapshot(s), since {first} · last {date}", f.history.len()))
                            .small()
                            .weak(),
                    );
                }
            });
        });
        if let Some(e) = &f.last_error {
            ui.label(RichText::new(format!("Last update failed: {e}")).small().color(icons::RED));
        }

        let Some((_, snap)) = f.latest() else {
            ui.label(RichText::new("No data yet.").weak());
            return;
        };
        let selected = metric.filter(|m| snap.contains_key(m)).or_else(|| snap.keys().next().cloned());

        ui.add_space(4.0);
        egui::Grid::new(("follow", &key)).striped(true).spacing([24.0, 4.0]).show(ui, |ui| {
            for h in ["Metric", "Now", "1 day", "7 days", "30 days"] {
                ui.label(RichText::new(h).small().strong());
            }
            ui.end_row();
            for (name, value) in snap {
                let is_sel = selected.as_deref() == Some(name.as_str());
                if ui.selectable_label(is_sel, name).on_hover_text("Show history").clicked() {
                    action = Some(Action::SelectMetric(key.clone(), name.clone()));
                }
                ui.label(RichText::new(api::fmt_num(*value)).strong());
                for days in [1, 7, 30] {
                    delta_label(ui, f.change(name, days));
                }
                ui.end_row();
            }
        });

        if let Some(m) = selected {
            let series = f.series(&m);
            ui.add_space(6.0);
            if series.len() >= 2 {
                daily_chart(ui, &format!("{m}: history"), &series, false);
            } else {
                icons::info(ui, "The history chart appears once there are two days of snapshots.");
            }
        }
    });
    action
}

fn delta_label(ui: &mut egui::Ui, change: Option<f64>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        match change {
            None => {
                ui.label(RichText::new("—").weak());
            }
            Some(d) if d.abs() < 0.5 => {
                ui.label(RichText::new("0").weak());
            }
            Some(d) => {
                let (icon, color) = if d > 0.0 { (Icon::Up, icons::GREEN) } else { (Icon::Down, icons::RED) };
                icons::inline(ui, icon, color);
                let sign = if d > 0.0 { "+" } else { "" };
                ui.label(RichText::new(format!("{sign}{}", api::fmt_num(d))).color(color));
            }
        }
    });
}

fn site_list(sources: [bool; 4]) -> String {
    let names: Vec<&str> = Source::ALL.iter().zip(sources).filter(|(_, on)| *on).map(|(s, _)| s.label()).collect();
    names.join(", ")
}

/// "TODAY", "YESTERDAY" or the date itself.
fn day_label(date: &str) -> String {
    let today = chrono::Local::now().date_naive();
    match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) if d == today => "TODAY".into(),
        Ok(d) if Some(d) == today.pred_opt() => "YESTERDAY".into(),
        _ => date.to_string(),
    }
}

fn drill_down(source: Source, report: &Report, item: &str) -> Search {
    let mut sources = [false; 4];
    sources[Source::ALL.iter().position(|&s| s == source).unwrap_or(0)] = true;
    // GitHub repos need owner/repo; the owner is the login in the report URL.
    let query = match source {
        // Search results already carry the full owner/repo.
        Source::GitHub if item.contains('/') => item.to_string(),
        Source::GitHub => {
            let owner = report.url.trim_end_matches('/').rsplit('/').next().unwrap_or_default();
            format!("{owner}/{item}")
        }
        _ => item.to_string(),
    };
    Search { kind: Kind::Package, query, sources }
}

fn welcome(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading(RichText::new("PkgStats").size(32.0).strong());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            // Centre the row of badges.
            ui.add_space((ui.available_width() - 4.0 * 26.0) / 2.0);
            for s in Source::ALL {
                icons::source_badge(ui, s);
            }
        });
        ui.add_space(6.0);
        ui.label("Developer and package stats from GitHub, npm, PyPI and crates.io.");
        ui.add_space(6.0);
        icons::info(
            ui,
            "User: look up a username on each site (followers, stars, packages, downloads).\n\n\
             Package / repo: look up a package (react, requests, serde) or a repo (rust-lang/rust) \
             for download history and details.\n\n\
             Following: star a report, or enter your username on the Following page, to track changes every day.",
        );
    });
}

fn metric_cards(ui: &mut egui::Ui, report: &Report, followed: Option<&Followed>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        for m in &report.metrics {
            egui::Frame::group(ui.style())
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(120.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&m.label).small().weak());
                        let value = if m.value.is_empty() { "—" } else { m.value.as_str() };
                        ui.label(RichText::new(value).size(18.0).strong());
                        if let Some(change) = followed.and_then(|f| f.change(&m.label, 1)) {
                            ui.horizontal(|ui| {
                                delta_label(ui, Some(change));
                                ui.label(RichText::new("vs yesterday").small().weak());
                            });
                        }
                    });
                });
        }
    });
}

fn daily_chart(ui: &mut egui::Ui, title: &str, points: &[(String, f64)], with_average: bool) {
    if points.is_empty() {
        return;
    }
    ui.label(RichText::new(title).strong());

    let daily: Vec<[f64; 2]> = points.iter().enumerate().map(|(i, p)| [i as f64, p.1]).collect();
    // A 7-day rolling average smooths out the weekday/weekend saw-tooth.
    let avg: Vec<[f64; 2]> = (0..points.len())
        .map(|i| {
            let lo = i.saturating_sub(6);
            let window = &points[lo..=i];
            [i as f64, window.iter().map(|p| p.1).sum::<f64>() / window.len() as f64]
        })
        .collect();

    let dates: Vec<String> = points.iter().map(|p| p.0.clone()).collect();
    let dates_axis = dates.clone();

    Plot::new(("daily", title))
        .height(if with_average { 260.0 } else { 180.0 })
        .allow_scroll(false)
        .include_y(0.0)
        .legend(egui_plot::Legend::default())
        .x_axis_formatter(move |mark, _| {
            let i = mark.value.round();
            if i < 0.0 || (mark.value - i).abs() > 1e-6 {
                return String::new();
            }
            dates_axis.get(i as usize).cloned().unwrap_or_default()
        })
        .y_axis_formatter(|mark, _| api::fmt_compact(mark.value))
        .label_formatter(move |pos| {
            let p = match pos {
                egui_plot::HoverPosition::NearDataPoint { position, .. } => *position,
                egui_plot::HoverPosition::Elsewhere { position } => *position,
            };
            let i = p.x.round();
            if i < 0.0 {
                return None;
            }
            let date = dates.get(i as usize)?;
            Some(format!("{date}\n{}", api::fmt_num(p.y)))
        })
        .show(ui, |plot| {
            if with_average {
                plot.line(Line::new("daily", PlotPoints::new(daily)).color(ACCENT.gamma_multiply(0.5)));
                plot.line(Line::new("7-day average", PlotPoints::new(avg)).color(ACCENT).width(2.0));
            } else {
                plot.line(Line::new(title, PlotPoints::new(daily)).color(ACCENT).width(2.0));
            }
        });
}

/// Horizontal bar list, painted directly so long labels stay readable.
fn bar_rows(ui: &mut egui::Ui, title: &str, bars: &[(String, f64)]) {
    if bars.is_empty() {
        return;
    }
    ui.label(RichText::new(title).strong());
    ui.add_space(4.0);
    let max = bars.iter().map(|b| b.1).fold(0.0_f64, f64::max).max(1.0);
    let is_percent = title.contains('%');
    let label_w = 180.0;
    let value_w = 90.0;

    for (label, value) in bars {
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let text_color = ui.visuals().text_color();
        let font = egui::FontId::proportional(13.0);

        let short: String = label.chars().take(26).collect();
        painter.text(rect.left_center(), egui::Align2::LEFT_CENTER, short, font.clone(), text_color);

        let bar_space = (width - label_w - value_w).max(20.0);
        let bar_w = (value / max) as f32 * bar_space;
        let bar = egui::Rect::from_min_size(
            egui::pos2(rect.left() + label_w, rect.top() + 4.0),
            egui::vec2(bar_w.max(2.0), rect.height() - 8.0),
        );
        painter.rect_filled(bar, 3.0, ACCENT.gamma_multiply(if resp.hovered() { 1.0 } else { 0.75 }));

        let text = if is_percent { format!("{value:.1}%") } else { api::fmt_compact(*value) };
        painter.text(
            egui::pos2(bar.right() + 6.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            font,
            ui.visuals().weak_text_color(),
        );
        resp.on_hover_text(format!("{label}: {}", if is_percent { format!("{value:.2}%") } else { api::fmt_num(*value) }));
    }
}

// ---------------------------------------------------------------------------
// Background snapshot task (Windows Task Scheduler)

fn schtasks(args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new("schtasks");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("schtasks exited with {}", out.status))
    }
}

fn install_task() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let run = format!("\"{}\" --snapshot", exe.display());
    // Hourly, so a missed day (PC off at a fixed time) is caught up; the
    // snapshot command skips anything that already has today's numbers.
    schtasks(&["/Create", "/F", "/SC", "HOURLY", "/TN", TASK_NAME, "/TR", &run])
}
