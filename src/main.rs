#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod api;
mod history;
mod icons;
mod settings;
mod theme;
mod track;

use api::{Chart, FetchResult, GitHubAccount, Kind, RateLimits, Report, Source};
use history::History;
use settings::Settings;
use eframe::egui::{self, RichText};
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
            .with_decorations(false)
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([680.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PkgStats",
        options,
        Box::new(|cc| {
            let app = App::default();
            theme::apply(&cc.egui_ctx, app.settings.accent());
            Ok(Box::new(app))
        }),
    )
}

fn snapshot_followed() {
    let mut store = Store::load();
    let token = saved_or_env_token();
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

/// The connected account's token, else the GITHUB_TOKEN environment variable.
fn saved_or_env_token() -> Option<String> {
    accounts::load_github_token().or_else(|| std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()))
}

/// How often the running app checks whether followed items need today's snapshot.
const AUTO_REFRESH_EVERY: Duration = Duration::from_secs(30 * 60);
/// Height of the custom title bar (the window has no native frame).
const TITLE_BAR_HEIGHT: f32 = 40.0;
/// How close to the window edge the pointer must be to resize.
const RESIZE_EDGE: f32 = 5.0;
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
    Accounts,
}

enum Msg {
    /// (search generation, source, result)
    Search(u64, Source, FetchResult),
    /// (followed key, result, remove the entry if it fails)
    Follow(String, FetchResult, bool),
    /// Result of checking a token: (token, account, limits); `save` = store it when valid.
    Account(Result<(String, GitHubAccount, RateLimits), String>, bool),
    Limits(Result<RateLimits, String>),
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
    OpenAccounts,
    /// Validate and save a pasted GitHub token.
    ConnectGitHub(String),
    DisconnectGitHub,
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
    account: Option<GitHubAccount>,
    limits: Option<RateLimits>,
    token_input: String,
    connecting: bool,
    account_error: Option<String>,
    account_checked: bool,
    settings: Settings,
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
            github_token: saved_or_env_token().unwrap_or_default(),
            show_settings: false,
            filter: String::new(),
            store,
            refreshing: HashSet::new(),
            chart_metric: HashMap::new(),
            last_auto_refresh: None,
            status: None,
            sidebar_open: false,
            history: History::load(),
            account: None,
            limits: None,
            token_input: String::new(),
            connecting: false,
            account_error: None,
            account_checked: false,
            settings: Settings::load(),
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

    /// Check a token in the background; `save` stores it once GitHub accepts it.
    fn check_token(&mut self, ctx: &egui::Context, token: String, save: bool) {
        self.connecting = true;
        self.account_error = None;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = api::github_account(&token).map(|account| {
                let limits = api::github_rate_limits(Some(&token)).unwrap_or_default();
                (token, account, limits)
            });
            let _ = tx.send(Msg::Account(result, save));
            ctx.request_repaint();
        });
    }

    fn refresh_limits(&self, ctx: &egui::Context) {
        let (tx, ctx, token) = (self.tx.clone(), ctx.clone(), self.github_token.clone());
        std::thread::spawn(move || {
            let token = Some(token).filter(|t| !t.is_empty());
            let _ = tx.send(Msg::Limits(api::github_rate_limits(token.as_deref())));
            ctx.request_repaint();
        });
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
                Msg::Account(result, save) => {
                    self.connecting = false;
                    match result {
                        Ok((token, account, limits)) => {
                            if save && let Err(e) = accounts::save_github_token(&token) {
                                self.account_error = Some(format!("Connected, but couldn't save the token: {e}"));
                            }
                            self.github_token = token;
                            self.token_input.clear();
                            self.account = Some(account);
                            self.limits = Some(limits);
                        }
                        // A saved token GitHub now rejects would break every request: drop it.
                        Err(e) if !save && e.contains("401") => {
                            self.github_token.clear();
                            self.account_error = Some(format!("Your saved GitHub token stopped working. {e} Connect again."));
                        }
                        Err(e) => self.account_error = Some(e),
                    }
                }
                Msg::Limits(result) => {
                    if let Ok(limits) = result {
                        self.limits = Some(limits);
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
            Action::OpenAccounts => {
                self.view = View::Accounts;
                self.refresh_limits(ctx);
            }
            Action::ConnectGitHub(token) => {
                let token = token.trim().to_string();
                if !token.is_empty() {
                    self.check_token(ctx, token, true);
                }
            }
            Action::DisconnectGitHub => {
                if let Err(e) = accounts::delete_github_token() {
                    self.account_error = Some(format!("Couldn't remove the saved token: {e}"));
                }
                self.github_token.clear();
                self.account = None;
                self.refresh_limits(ctx);
            }
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

    /// Custom window frame: app controls on the left, window buttons on the right,
    /// and the empty space in between drags the window (double-click maximizes).
    fn title_bar(&mut self, ui: &mut egui::Ui, on_resize_edge: bool) {
        let ctx = ui.ctx().clone();
        let rect = egui::Rect::from_min_size(ui.max_rect().min, egui::vec2(ui.max_rect().width(), TITLE_BAR_HEIGHT));

        // Registered before the widgets, so buttons and the search box on top win their clicks.
        let bar = ui.interact(rect, egui::Id::unique("title_bar"), egui::Sense::click_and_drag());
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if bar.drag_started_by(egui::PointerButton::Primary) && !on_resize_edge {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if bar.double_clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }

        let mut submit = false;
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| {
            ui.add_space(8.0);
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
            ui.add_space(4.0);
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
            // Leave room for Go, settings, the window buttons and some empty, draggable bar.
            let width = (ui.available_width() - 330.0).clamp(140.0, 460.0);
            let edit = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(hint).desired_width(width));
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            if icons::button(ui, Some(Icon::Search), "Go", false, Some(theme::accent())).clicked() {
                submit = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if icons::window_button(ui, Icon::Close, TITLE_BAR_HEIGHT, "Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let (icon, tip) = if maximized { (Icon::WinRestore, "Restore") } else { (Icon::WinMaximize, "Maximize") };
                if icons::window_button(ui, icon, TITLE_BAR_HEIGHT, tip).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                if icons::window_button(ui, Icon::WinMinimize, TITLE_BAR_HEIGHT, "Minimize").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                ui.add_space(8.0);
                if icons::icon_button(ui, Icon::Settings, "Settings").clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
        });

        if submit {
            let search = Search { kind: self.kind, query: self.query.clone(), sources: self.sources };
            self.start_search(&ctx, search, true);
        }
    }

    /// What's left under the title bar: source filters, status line and result tabs.
    fn has_sub_bar(&self) -> bool {
        self.view == View::Search || self.status.is_some()
    }

    fn sub_bar(&mut self, ui: &mut egui::Ui) {
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

        if self.view == View::Search && !self.tabs.is_empty() {
            ui.separator();
            self.tab_strip(ui);
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
                ui.label(RichText::new("App style").strong());
                icons::info(ui, "Accent colour for buttons, links, charts, bars and highlights. Saved for next time.");
            });
            let current = self.settings.accent();
            let mut picked = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (name, color) in theme::PRESETS {
                    if icons::swatch(ui, color, color == current, name).clicked() {
                        picked = Some(color);
                    }
                }
                ui.add_space(6.0);
                let mut custom = current;
                let is_custom = !theme::PRESETS.iter().any(|(_, c)| *c == current);
                let resp = egui::color_picker::color_edit_button_srgba(ui, &mut custom, egui::color_picker::Alpha::Opaque);
                if resp.on_hover_text(if is_custom { "Custom colour (selected)" } else { "Custom colour" }).changed() {
                    picked = Some(custom);
                }
            });
            if let Some(color) = picked {
                theme::set_accent(ctx, color);
                self.settings.set_accent(color);
                if let Err(e) = self.settings.save() {
                    self.status = Some((format!("Could not save settings: {e}"), true));
                }
            }

            ui.add_space(6.0);
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
            let accounts_label = match &self.account {
                Some(a) => format!("Token · @{}", a.login),
                None => "Add token".to_string(),
            };
            if icons::nav_row(ui, Some(Icon::Link), None, &accounts_label, self.view == View::Accounts).clicked() {
                action = Some(Action::OpenAccounts);
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
    // Add token page

    fn accounts_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Add token");
                icons::info(
                    ui,
                    "A GitHub token raises the number of requests PkgStats can make. \
                     It's kept in Windows Credential Manager, never in PkgStats' files.",
                );
            });
            ui.add_space(8.0);

            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    icons::source_badge(ui, Source::GitHub);
                    ui.label(RichText::new("GitHub").strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &self.account {
                        Some(a) => {
                            let who = if a.name.is_empty() { format!("@{}", a.login) } else { format!("{} (@{})", a.name, a.login) };
                            ui.label(RichText::new(format!("Token added · {who}")).color(icons::GREEN));
                            icons::inline(ui, Icon::Check, icons::GREEN);
                        }
                        None => {
                            ui.label(RichText::new("No token").weak());
                        }
                    });
                });
                ui.add_space(8.0);

                let connected = self.account.is_some();
                egui::Grid::new("github_benefits").spacing([28.0, 6.0]).show(ui, |ui| {
                    for h in ["", "Without token", "With token", "Your limit now"] {
                        ui.label(RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    let now = |remaining: f64, limit: f64| match self.limits {
                        Some(_) if limit > 0.0 => format!("{} of {} left", api::fmt_num(remaining), api::fmt_num(limit)),
                        _ => "—".into(),
                    };
                    let l = self.limits.unwrap_or_default();
                    let rows = [
                        ("Searches per minute", "10", "30", now(l.search_remaining, l.search_limit)),
                        ("Requests per hour", "60", "5,000", now(l.core_remaining, l.core_limit)),
                    ];
                    for (label, without, with, current) in rows {
                        ui.label(label);
                        let (a, b) = if connected {
                            (RichText::new(without).weak(), RichText::new(with).strong().color(icons::GREEN))
                        } else {
                            (RichText::new(without).strong(), RichText::new(with).color(icons::GREEN))
                        };
                        ui.label(a);
                        ui.label(b);
                        ui.label(RichText::new(current).weak());
                        ui.end_row();
                    }
                });
                ui.add_space(10.0);

                if let Some(a) = &self.account {
                    ui.horizontal(|ui| {
                        if icons::button(ui, Some(Icon::External), "Open profile", false, None).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&a.url));
                        }
                        if icons::button(ui, Some(Icon::Close), "Remove token", false, None).clicked() {
                            action = Some(Action::DisconnectGitHub);
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if icons::button(ui, Some(Icon::External), "Create a token on GitHub", false, None).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(
                                "https://github.com/settings/tokens/new?description=PkgStats&scopes=",
                            ));
                        }
                        icons::info(
                            ui,
                            "Opens GitHub's token page with the name filled in. Leave every scope unticked: \
                             PkgStats only reads public data. Click Generate token, copy it and paste it below.",
                        );
                    });
                    ui.horizontal(|ui| {
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.token_input)
                                .password(true)
                                .hint_text("paste token (ghp_… or github_pat_…)")
                                .desired_width(320.0),
                        );
                        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if self.connecting {
                            ui.spinner();
                        } else if icons::button(ui, Some(Icon::Link), "Add token", false, Some(theme::accent())).clicked() || enter {
                            action = Some(Action::ConnectGitHub(self.token_input.clone()));
                        }
                    });
                }
                if let Some(e) = &self.account_error {
                    ui.label(RichText::new(e).color(icons::RED));
                }
            });
            ui.add_space(10.0);

            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                for source in [Source::Npm, Source::PyPI, Source::Crates] {
                    ui.horizontal(|ui| {
                        icons::source_badge(ui, source);
                        ui.label(RichText::new(source.label()).strong());
                        ui.label(RichText::new("No token needed").weak());
                        icons::info(
                            ui,
                            &format!("{}'s public stats have the same limits with or without a token.", source.label()),
                        );
                    });
                }
            });
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
                    if ui.link(RichText::new(&e.query).color(theme::accent()).strong()).on_hover_text("Search again").clicked() {
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

            theme::card().show(ui, |ui| {
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
        if !self.account_checked {
            self.account_checked = true;
            if !self.github_token.is_empty() {
                self.check_token(&ctx, self.github_token.clone(), false);
            }
        }

        let resize_dir = resize_direction(&ctx);

        egui::Panel::top("title_bar")
            .exact_size(TITLE_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(theme::bg()))
            .show(ui, |ui| self.title_bar(ui, resize_dir.is_some()));

        if self.has_sub_bar() {
            let sub_frame = egui::Frame::new().fill(theme::bg()).inner_margin(egui::Margin::symmetric(14, 6));
            egui::Panel::top("sub_bar").frame(sub_frame).show(ui, |ui| self.sub_bar(ui));
        }

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

        let central = egui::Frame::new().fill(theme::bg()).inner_margin(egui::Margin::symmetric(24, 18));
        egui::CentralPanel::default().frame(central).show(ui, |ui| {
            let main_action = match self.view {
                View::Search => self.search_view(ui),
                View::Following => self.following_view(ui),
                View::History => self.history_view(ui),
                View::Accounts => self.accounts_view(ui),
            };
            action = action.take().or(main_action);
        });
        if let Some(action) = action {
            self.apply(&ctx, action);
        }

        window_edges(&ctx, resize_dir);
    }
}

/// Which edge or corner the pointer is on, if the window can be resized there.
fn resize_direction(ctx: &egui::Context) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection::*;
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return None;
    }
    let rect = ctx.viewport_rect();
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    let left = pos.x - rect.left() < RESIZE_EDGE;
    let right = rect.right() - pos.x < RESIZE_EDGE;
    let top = pos.y - rect.top() < RESIZE_EDGE;
    let bottom = rect.bottom() - pos.y < RESIZE_EDGE;
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => NorthWest,
        (_, true, true, _) => NorthEast,
        (true, _, _, true) => SouthWest,
        (_, true, _, true) => SouthEast,
        (true, ..) => West,
        (_, true, ..) => East,
        (_, _, true, _) => North,
        (_, _, _, true) => South,
        _ => return None,
    })
}

/// Without a native frame the app provides its own edge resizing and a 1px outline.
fn window_edges(ctx: &egui::Context, dir: Option<egui::ResizeDirection>) {
    use egui::ResizeDirection::*;
    if let Some(dir) = dir {
        ctx.set_cursor_icon(match dir {
            North | South => egui::CursorIcon::ResizeVertical,
            East | West => egui::CursorIcon::ResizeHorizontal,
            NorthWest | SouthEast => egui::CursorIcon::ResizeNwSe,
            NorthEast | SouthWest => egui::CursorIcon::ResizeNeSw,
        });
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
    if !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("window_outline")));
        let outline = egui::Stroke::new(1.0, theme::palette().border_strong);
        painter.rect_stroke(ctx.viewport_rect(), 0.0, outline, egui::StrokeKind::Inside);
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
        ui.heading(&report.title);
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
        ui.label(RichText::new(&report.subtitle).color(theme::muted()));
    }
    if let Some(note) = &report.note {
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Info, icons::GOLD);
            ui.label(RichText::new(note).small().color(icons::GOLD));
        });
    }
    ui.add_space(16.0);

    metric_cards(ui, report, followed);

    for chart in &report.charts {
        match chart {
            Chart::Daily { title, points } => daily_chart(ui, title, points, true),
            Chart::Bars { title, bars } => bar_rows(ui, title, bars),
        }
        ui.add_space(12.0);
    }

    if !report.items.is_empty() {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("All items ({})", report.items.len())).family(theme::semibold()).size(15.0));
                ui.add(egui::TextEdit::singleline(filter).hint_text("filter…").desired_width(180.0));
                icons::info(ui, "Click a name to see its stats, or the arrow next to it to open its page in your browser.");
            });
            ui.add_space(8.0);
            let needle = filter.to_lowercase();
            egui::Grid::new(("items", source.label())).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                ui.label(theme::caps_label("Name"));
                ui.label("");
                for col in &report.columns {
                    ui.label(theme::caps_label(col));
                }
                ui.label(theme::caps_label("Description"));
                ui.end_row();
    
                for item in report.items.iter().filter(|i| {
                    needle.is_empty()
                        || i.name.to_lowercase().contains(&needle)
                        || i.description.to_lowercase().contains(&needle)
                }) {
                    if ui.link(RichText::new(&item.name).color(theme::accent())).on_hover_text("Show stats").clicked() {
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
                    ui.label(RichText::new(if more { format!("{desc}…") } else { desc }).color(theme::muted()));
                    ui.end_row();
                }
            });
        });
    }
    action
}

fn followed_card(ui: &mut egui::Ui, f: &Followed, metric: Option<String>, busy: bool) -> Option<Action> {
    let mut action = None;
    let key = f.key();
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            icons::source_badge(ui, f.source);
            let kind_icon = if f.kind == Kind::User { Icon::User } else { Icon::Package };
            icons::inline(ui, kind_icon, ui.visuals().weak_text_color());
            if ui.link(RichText::new(&f.title).strong().size(15.0).color(theme::accent())).on_hover_text("Show full stats").clicked()
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

/// Equal-width metric tiles, up to four per row.
fn metric_cards(ui: &mut egui::Ui, report: &Report, followed: Option<&Followed>) {
    const GAP: f32 = 12.0;
    let avail = ui.available_width();
    let cols = (((avail + GAP) / (210.0 + GAP)).floor() as usize).clamp(1, 4);
    let tile_w = (avail - GAP * (cols as f32 - 1.0)) / cols as f32;
    // Frame inner margin (20 each side) + 1px border.
    let content_w = tile_w - 42.0;

    for row in report.metrics.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for m in row {
                theme::card().show(ui, |ui| {
                    ui.set_width(content_w);
                    ui.set_height(74.0);
                    ui.vertical(|ui| {
                        ui.label(theme::caps_label(&m.label));
                        ui.add_space(8.0);
                        let value = if m.value.is_empty() { "—" } else { m.value.as_str() };
                        ui.add(egui::Label::new(theme::big_number(value)).truncate());
                        if let Some(change) = followed.and_then(|f| f.change(&m.label, 1)) {
                            ui.horizontal(|ui| {
                                delta_label(ui, Some(change));
                                ui.label(RichText::new("vs yesterday").small().color(theme::muted()));
                            });
                        }
                    });
                });
            }
        });
        ui.add_space(GAP);
    }
}

/// Monotone cubic (Fritsch–Carlson) interpolation: a smooth curve through every
/// point that never overshoots, so counts can't dip below zero between days.
fn smooth(pts: &[[f64; 2]], steps: usize) -> Vec<[f64; 2]> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let d: Vec<f64> = (0..n - 1).map(|i| (pts[i + 1][1] - pts[i][1]) / (pts[i + 1][0] - pts[i][0])).collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
        let h = a * a + b * b;
        if h > 9.0 {
            let t = 3.0 / h.sqrt();
            m[i] = t * a * d[i];
            m[i + 1] = t * b * d[i];
        }
    }

    let mut out = Vec::with_capacity((n - 1) * steps + 1);
    for i in 0..n - 1 {
        let ([x0, y0], [x1, y1]) = (pts[i], pts[i + 1]);
        let h = x1 - x0;
        for k in 0..steps {
            let t = k as f64 / steps as f64;
            let (t2, t3) = (t * t, t * t * t);
            let y = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
                + (t3 - 2.0 * t2 + t) * h * m[i]
                + (-2.0 * t3 + 3.0 * t2) * y1
                + (t3 - t2) * h * m[i + 1];
            out.push([x0 + t * h, y]);
        }
    }
    out.push(pts[n - 1]);
    out
}

/// Smooth line chart in a card. With `with_average`, the curve is the 7-day
/// average (daily downloads zig-zag between weekdays and weekends) and the raw
/// daily values are drawn faintly behind it.
fn daily_chart(ui: &mut egui::Ui, title: &str, points: &[(String, f64)], with_average: bool) {
    if points.is_empty() {
        return;
    }
    let daily: Vec<[f64; 2]> = points.iter().enumerate().map(|(i, p)| [i as f64, p.1]).collect();
    let avg: Vec<[f64; 2]> = (0..points.len())
        .map(|i| {
            let window = &points[i.saturating_sub(6)..=i];
            [i as f64, window.iter().map(|p| p.1).sum::<f64>() / window.len() as f64]
        })
        .collect();
    let steps = (800 / points.len()).clamp(2, 16);
    let main_curve = smooth(if with_average { &avg } else { &daily }, steps);

    let dates: Vec<String> = points.iter().map(|p| p.0.clone()).collect();
    let dates_axis = dates.clone();
    let (raw, averaged) = (daily.clone(), avg.clone());

    let body = |ui: &mut egui::Ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).family(theme::semibold()).size(15.0).color(theme::text()));
            if with_average {
                ui.label(RichText::new("7-day average").small().color(theme::muted()));
            }
        });
        ui.add_space(10.0);

        Plot::new(("daily", title))
            .height(if with_average { 260.0 } else { 190.0 })
            .show_background(false)
            .grid_color(theme::grid())
            .cursor_color(theme::muted().gamma_multiply(0.5))
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .include_y(0.0)
            .y_axis_min_width(44.0)
            .x_axis_formatter(move |mark, _| {
                let i = mark.value.round();
                if i < 0.0 || (mark.value - i).abs() > 1e-6 {
                    return String::new();
                }
                // "09-20" like the dashboard; full dates show in the hover label.
                dates_axis.get(i as usize).map(|d| d.get(5..).unwrap_or(d).to_string()).unwrap_or_default()
            })
            .y_axis_formatter(|mark, _| api::fmt_compact(mark.value))
            .label_formatter(move |pos| {
                let x = match pos {
                    egui_plot::HoverPosition::NearDataPoint { position, .. } => position.x,
                    egui_plot::HoverPosition::Elsewhere { position } => position.x,
                };
                // The curve is interpolated, so report the real values for the nearest day.
                let i = x.round();
                if i < 0.0 {
                    return None;
                }
                let i = i as usize;
                let date = dates.get(i)?;
                let day = api::fmt_num(raw.get(i)?[1]);
                Some(if with_average {
                    format!("{date}\n{day}  (7-day avg {})", api::fmt_num(averaged.get(i)?[1]))
                } else {
                    format!("{date}\n{day}")
                })
            })
            .show(ui, |plot| {
                if with_average {
                    plot.line(
                        Line::new("daily", PlotPoints::new(daily))
                            .color(theme::accent().gamma_multiply(0.22))
                            .width(1.0),
                    );
                }
                plot.line(
                    Line::new(title, PlotPoints::new(main_curve))
                        .color(theme::accent())
                        .width(2.2)
                        .fill(0.0)
                        .fill_alpha(0.10),
                );
            });
    };
    // Report charts get their own card; the small history chart sits inside a Following card already.
    if with_average {
        theme::card().show(ui, body);
        ui.add_space(12.0);
    } else {
        body(ui);
    }
}

/// Horizontal bar list in a card, painted directly so long labels stay readable.
fn bar_rows(ui: &mut egui::Ui, title: &str, bars: &[(String, f64)]) {
    if bars.is_empty() {
        return;
    }
    let max = bars.iter().map(|b| b.1).fold(0.0_f64, f64::max).max(1.0);
    let is_percent = title.contains('%');
    let (label_w, value_w) = (180.0, 90.0);

    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::card_title(ui, title);
        for (label, value) in bars {
            let width = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let font = egui::FontId::proportional(13.0);

            let short: String = label.chars().take(26).collect();
            painter.text(rect.left_center(), egui::Align2::LEFT_CENTER, short, font.clone(), theme::text());

            // Faint full-width track, then the value bar on top.
            let track = egui::Rect::from_min_size(
                egui::pos2(rect.left() + label_w, rect.top() + 7.0),
                egui::vec2((width - label_w - value_w).max(20.0), rect.height() - 14.0),
            );
            painter.rect_filled(track, 5.0, theme::grid().gamma_multiply(0.6));
            let bar = egui::Rect::from_min_size(track.min, egui::vec2((track.width() * (value / max) as f32).max(4.0), track.height()));
            painter.rect_filled(bar, 5.0, theme::accent().gamma_multiply(if resp.hovered() { 1.0 } else { 0.8 }));

            let text = if is_percent { format!("{value:.1}%") } else { api::fmt_compact(*value) };
            painter.text(egui::pos2(track.right() + 10.0, rect.center().y), egui::Align2::LEFT_CENTER, text, font, theme::muted());
            resp.on_hover_text(format!("{label}: {}", if is_percent { format!("{value:.2}%") } else { api::fmt_num(*value) }));
        }
    });
    ui.add_space(12.0);
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

#[cfg(test)]
mod tests {
    use super::smooth;

    #[test]
    fn smooth_curve_hits_every_point_without_overshooting() {
        let pts = [[0.0, 0.0], [1.0, 140.0], [2.0, 20.0], [3.0, 20.0], [4.0, 50.0]];
        let curve = smooth(&pts, 8);
        for p in pts {
            assert!(curve.iter().any(|c| (c[0] - p[0]).abs() < 1e-9 && (c[1] - p[1]).abs() < 1e-9));
        }
        assert!(curve.iter().all(|c| (0.0..=140.0).contains(&c[1])), "overshoot");
        assert!(curve.windows(2).all(|w| w[1][0] > w[0][0]));
    }
}
