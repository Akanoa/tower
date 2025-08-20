use crate::interface::filters::{FilterType, Rowable};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::tcp_server::PollControl;
use executor_item::ExecutorItem;
use protocol::{Message, MessageBody, MessageRegister, MessageReport, MessageUnregister};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Sparkline, Table};
use ratatui::Terminal;
use tenant_item::TenantItem;
use tokio::sync::mpsc;
use watch_item::WatchItem;

mod executor_item;
mod filters;
mod tenant_item;
mod watch_item;

type ExecutorKey = (i64, String);

// Resample a u64 series to exactly `width` points for visual stretching in sparklines
fn resample_to_width_u64(values: &[u64], width: usize) -> Vec<u64> {
    if width == 0 {
        return Vec::new();
    }
    if values.is_empty() {
        return vec![0; width];
    }
    if values.len() == 1 {
        return vec![values[0]; width];
    }
    let src_len = values.len();
    let dst_len = width;
    let mut out = Vec::with_capacity(dst_len);
    let denom = (dst_len - 1) as f64;
    let src_max = (src_len - 1) as f64;
    for i in 0..dst_len {
        let pos = if denom == 0.0 {
            0.0
        } else {
            (i as f64) * (src_max / denom)
        };
        let idx = pos.round().clamp(0.0, src_max) as usize;
        out.push(values[idx]);
    }
    out
}

pub type MessageReceiver = mpsc::UnboundedReceiver<Message>;

#[derive(Clone)]
pub struct AggregatorTabConfig {
    pub backends: Vec<(String, u16)>,
    pub control_tx: mpsc::UnboundedSender<PollControl>,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub tenants: BTreeMap<String, TenantItem>,
    // index of the currently selected visible row in flat view
    pub selected: usize,
    // current tenant filter (case-insensitive contains)
    pub filter: String,
    // current host filter (case-insensitive contains)
    pub host_filter: String,
    // vertical scroll offset for the main table
    pub scroll_offset: usize,
    // rows filters
    pub filters: filters::Filters,
}

impl AppState {
    fn get_or_create_tenant(&mut self, tenant: &str) -> &mut TenantItem {
        self.tenants
            .entry(tenant.to_string())
            .or_insert_with(|| TenantItem {
                tenant: tenant.to_string(),
                folded: false,
                executors: BTreeMap::new(),
            })
    }

    fn get_or_create_executor(
        &mut self,
        tenant: &str,
        executor_id: i64,
        host: &str,
    ) -> &mut ExecutorItem {
        let tenant_entry = self.get_or_create_tenant(tenant);
        tenant_entry
            .executors
            .entry((executor_id, host.to_string()))
            .or_insert_with(|| ExecutorItem {
                executor_id,
                folded: false,
                watchers: BTreeMap::new(),
                host: host.to_string(),
            })
    }

    fn get_or_create_watcher(
        &mut self,
        tenant: &str,
        executor_id: i64,
        watch_id: i64,
        host: &str,
        interest: &str,
    ) -> &mut WatchItem {
        let exec_entry = self.get_or_create_executor(tenant, executor_id, host);
        exec_entry
            .watchers
            .entry(watch_id)
            .or_insert_with(|| WatchItem {
                watch_id,
                lag: 0,
                execution_time: 0.0,
                updated_at: Instant::now(),
                interest: interest.to_string(),
                lag_hist: VecDeque::new(),
                exec_hist: VecDeque::new(),
                time_hist: VecDeque::new(),
            })
    }

    /// Updates the state of a watcher with the provided `MessageReport` data or creates a new watcher
    /// if it does not already exist. This function is used to track metrics such as lag, execution time,
    /// interest, and historical data points for a given host and report details.
    ///
    /// # Parameters
    ///
    /// * `host` - A `String` representing the host associated with the watcher.
    /// * `report` - A `MessageReport` containing the details to update the watcher state, including:
    ///     - `tenant`: The tenant identifier.
    ///     - `executor_id`: The executor's unique identifier.
    ///     - `watch_id`: The watcher's unique identifier.
    ///     - `interest`: The interest string associated with the watcher data.
    ///     - `lag`: The time lag for the current report.
    ///     - `execution_time`: The execution time for the current report.
    ///
    /// # Functionality
    ///
    /// 1. Searches for an existing watcher using the provided `tenant`, `executor_id`, `watch_id`, and `host`.
    ///    If no watcher exists, it creates a new watcher.
    /// 2. Updates the watcher's fields:
    ///     - `lag`: Updates with the report's `lag` value.
    ///     - `execution_time`: Updates with the report's `execution_time`.
    ///     - `updated_at`: Sets the current instant as the last updated timestamp.
    /// 3. Updates the `interest` field, but only if it was previously empty.
    /// 4. Tracks historical values for time, lag, and execution time:
    ///     - Appends the current timestamp (in seconds since the UNIX epoch), the report's lag, and execution time
    ///       to their respective historical collections (`time_hist`, `lag_hist`, `exec_hist`).
    ///     - Ensures that each historical collection does not exceed a maximum capacity of `MAX_POINTS` (`200`).
    ///       If necessary, the oldest entries are removed from the front of the collections.
    ///
    /// # Constants
    ///
    /// * `MAX_POINTS`: The maximum number of historical data points to retain in each of the `time_hist`,
    ///   `lag_hist`, and `exec_hist` collections. This is set to 200.
    pub fn apply_report(&mut self, host: String, report: MessageReport) {
        // Get the watcher or create it with tenant and executor hierarchy if not exist yet
        let watch = self.get_or_create_watcher(
            report.tenant.as_str(),
            report.executor_id,
            report.watch_id,
            host.as_str(),
            report.interest.as_str(),
        );
        // Update fields
        watch.lag = report.lag;
        watch.execution_time = report.execution_time;
        watch.updated_at = Instant::now();
        // Update interest if it changed or was empty
        if watch.interest.is_empty() {
            watch.interest = report.interest.clone();
        }
        /// A constant that defines the maximum number of points (or elements) allowed.
        ///
        /// This is set to a value of `200`, which can be used as a limit or threshold
        /// in scenarios where a maximum capacity for points needs to be enforced.
        const MAX_POINTS: usize = 200;
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        watch.time_hist.push_back(now_ts);
        watch.lag_hist.push_back(report.lag);
        watch.exec_hist.push_back(report.execution_time);
        while watch.time_hist.len() > MAX_POINTS {
            let _ = watch.time_hist.pop_front();
            let _ = watch.lag_hist.pop_front();
            let _ = watch.exec_hist.pop_front();
        }
    }

    /// Applies a registration message to the system by initializing or retrieving a watcher.
    ///
    /// This function processes a given `MessageRegister` and associates it with the specified host.
    /// If a corresponding watcher does not already exist in the system for the provided `tenant`,
    /// `executor_id`, and `watch_id`, it will create one under the hierarchy of tenant and executor.
    ///
    /// # Arguments
    ///
    /// * `host` - A string representing the host associated with the watcher.
    /// * `message_register` - A `MessageRegister` object containing the registration details,
    /// including the tenant, executor ID, and watch ID.
    ///
    /// # Behavior
    ///
    /// If a watcher for the specified tenant/executor/watch ID combination does not exist, it will be
    /// created with the provided host. The watcher may involve maintaining hierarchical relationships
    /// between the tenant and executor entities.
    pub fn apply_register(&mut self, host: String, message_register: MessageRegister) {
        // create it with tenant and executor hierarchy if not exist yet
        let _ = self.get_or_create_watcher(
            message_register.tenant.as_str(),
            message_register.executor_id,
            message_register.watch_id,
            &host,
            "",
        );
    }

    /// Updates the internal state of the system to handle the unregistration of a specific watcher
    /// for a given executor associated with a tenant.
    ///
    /// # Parameters
    /// - `host`: A `String` representing the host associated with the executor being updated.
    /// - `message_unregister`: An instance of `MessageUnregister` containing the tenant ID, executor ID, and watch ID
    ///   to identify the targeted tenant, executor, and watcher respectively.
    ///
    /// # Behavior
    /// - If a tenant matching the tenant ID within `message_unregister` exists in the `tenants` map:
    ///   - Attempts to find the executor using the `(executor_id, host)` pair.
    ///   - If the executor is found:
    ///     - Updates the executor's `host` property to the provided `host`.
    ///     - Removes the watcher specified by `message_unregister.watch_id` from the executor's `watchers`.
    ///
    /// # Notes
    /// - Does nothing if the provided tenant or executor is not found.
    /// - Modifies the state of the tenant and executor (if found) by updating fields and removing
    ///   the watcher.
    /// - The `tenants`, `executors`, and `watchers` are expected to be mutable, as this function
    ///   performs in-place updates.
    ///
    /// # Use-case
    /// This function is used to handle unregistration events when a watcher needs to be removed
    /// from an executor, such as during cleanup or resource management tasks.
    pub fn apply_unregister(&mut self, host: String, message_unregister: MessageUnregister) {
        if let Some(tenant) = self.tenants.get_mut(&message_unregister.tenant) {
            if let Some(exec) = tenant
                .executors
                .get_mut(&(message_unregister.executor_id, host.clone()))
            {
                exec.host = host;
                exec.watchers.remove(&message_unregister.watch_id);
            }
        }
    }

    /// Remove inactive watchers (> max_age since last update) and empty executors/tenants
    pub fn prune_inactive(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.tenants.retain(|_tname, tenant| {
            tenant.executors.retain(|_eid, exec| {
                exec.watchers.retain(|_wid, watch| {
                    now.saturating_duration_since(watch.updated_at) <= max_age
                });
                !exec.watchers.is_empty()
            });
            !tenant.executors.is_empty()
        });
    }

    /// Build visible rows respecting folding
    pub fn visible_rows(&self) -> Vec<Row<'static>> {
        // accumulate visible rows
        let mut rows = Vec::new();

        for (tenant_name, tenant) in &self.tenants {
            // Filter out tenants
            if !self.filters.is_row_visible(tenant, FilterType::Tenant) {
                continue;
            }
            // Compute which executors are visible under the host filter
            let exec_iter = tenant
                .executors
                .iter()
                .filter(|(_id, exec)| self.filters.is_row_visible(*exec, FilterType::Host));
            // We need to peek whether there is any executor to show; collect ids temporarily
            let visible_execs: Vec<(&ExecutorKey, &ExecutorItem)> = exec_iter.collect();
            if visible_execs.is_empty() {
                // Skip tenant entirely if no executor matches the host filter
                continue;
            }

            // add the row displaying the tenant in the table
            let executors: Vec<&ExecutorItem> =
                visible_execs.iter().map(|(_id, exec)| *exec).collect();
            rows.push(tenant.as_row(&executors));

            // If the tenant is folded, all executors are hidden
            if tenant.folded {
                continue;
            }

            for (exec_id, exec) in visible_execs {
                // add the row displaying the executor in the table
                rows.push(exec.as_row());

                // If the executor is folded, all watchers are hidden
                if exec.folded {
                    continue;
                }
                for (_watch_id, watch) in &exec.watchers {
                    // add the row displaying the watcher in the table
                    rows.push(watch.as_row());
                }
            }
        }
        rows
    }

    // Toggle fold state for the item at the visible index, if it's a tenant or executor row
    pub fn toggle_fold_at(&mut self, index: usize) {
        let mut i = 0usize;
        for (_tenant_name, tenant) in self.tenants.iter_mut() {
            // If the row is hidden, then don't take it in an account in folding compute
            if !self.filters.is_row_visible(tenant, FilterType::Tenant) {
                continue;
            }

            // determine if tenant has any execs visible under host filter
            let mut any_exec_visible = false;
            for (_eid, executor) in tenant.executors.iter() {
                // at least on executor matches the host filter
                if self.filters.is_row_visible(executor, FilterType::Host) {
                    any_exec_visible = true;
                    break;
                }
            }
            // If no executors match the host filter, the all tenant can skip
            if !any_exec_visible {
                continue;
            }

            if i == index {
                tenant.folded = !tenant.folded;
                return;
            }
            i += 1;
            if tenant.folded {
                continue;
            }
            for (_exec_id, exec) in tenant.executors.iter_mut() {
                // the executor is filtered by host
                if !self.filters.is_row_visible(exec, FilterType::Host) {
                    continue;
                }
                if i == index {
                    exec.folded = !exec.folded;
                    return;
                }
                i += 1;
                if exec.folded {
                    continue;
                }
                // skip watcher rows
                i += exec.watchers.len();
            }
        }
    }

    // Resolve current selection to a watcher identifier if selection points to a watcher row
    pub fn selected_watch_ids(&self) -> Option<(String, (i64, String), i64)> {
        let mut i = 0usize;
        for (tenant_name, tenant) in &self.tenants {
            // If the row is hidden, then don't take it in an account in folding compute
            if !self.filters.is_row_visible(tenant, FilterType::Tenant) {
                continue;
            }

            // Only consider tenants with at least one visible executor under host filter
            let mut any_exec_visible = false;
            for (_eid, executor) in tenant.executors.iter() {
                // at least on executor matches the host filter
                if self.filters.is_row_visible(executor, FilterType::Host) {
                    any_exec_visible = true;
                    break;
                }
            }
            if !any_exec_visible {
                continue;
            }
            if i == self.selected {
                // tenant row selected
                return None;
            }
            i += 1;
            if tenant.folded {
                continue;
            }
            for (exec_id, exec) in &tenant.executors {
                // the executor is filtered by host
                if !self.filters.is_row_visible(exec, FilterType::Host) {
                    continue;
                }
                if i == self.selected {
                    return None;
                }
                i += 1;
                if exec.folded {
                    continue;
                }
                for (watch_id, _watch) in &exec.watchers {
                    if i == self.selected {
                        return Some((tenant_name.clone(), exec_id.clone(), *watch_id));
                    }
                    i += 1;
                }
            }
        }
        None
    }

    // If selection is on a tenant row, return its name; otherwise None
    pub fn selected_tenant_name(&self) -> Option<String> {
        let mut i = 0usize;
        for (tenant_name, tenant) in &self.tenants {
            // If the row is hidden, then don't take it in an account in folding compute
            if !self.filters.is_row_visible(tenant, FilterType::Tenant) {
                continue;
            }
            // Consider only tenants with at least one visible executor under host filter
            let mut any_exec_visible = false;
            for (_eid, executor) in tenant.executors.iter() {
                // the executor is filtered by host
                if !self.filters.is_row_visible(executor, FilterType::Host) {
                    any_exec_visible = true;
                    break;
                }
            }
            if !any_exec_visible {
                continue;
            }
            if i == self.selected {
                // tenant row selected
                return Some(tenant_name.clone());
            }
            i += 1;
            if tenant.folded {
                continue;
            }
            for (_exec_id, exec) in &tenant.executors {
                // the executor is filtered by host
                if !self.filters.is_row_visible(exec, FilterType::Host) {
                    continue;
                }
                if i == self.selected {
                    // executor row selected
                    return None;
                }
                i += 1;
                if exec.folded {
                    continue;
                }
                // skip watcher rows
                i += exec.watchers.len();
            }
        }
        None
    }
}

pub async fn run_tui(
    mut rx: MessageReceiver,
    aggregator: Option<AggregatorTabConfig>,
    mut logs_rx: Option<mpsc::UnboundedReceiver<String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ratatui::crossterm::event::{self, Event, KeyCode};
    use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    use ratatui::crossterm::{execute, terminal};
    use std::io::{stdout, Stdout};

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout: Stdout = stdout();
    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        ratatui::crossterm::cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::default();
    app.filters.create_filter(FilterType::Tenant, "Tenant");
    app.filters.create_filter(FilterType::Host, "Host");

    let mut last_prune = Instant::now();

    // Aggregator tab state
    let aggregator_mode = aggregator.is_some();
    let logs_available = logs_rx.is_some();
    let (mut backends, mut control_tx_opt) = if let Some(cfg) = aggregator.clone() {
        (cfg.backends, Some(cfg.control_tx))
    } else {
        (Vec::new(), None)
    };
    let mut in_backends_tab = false; // only meaningful if aggregator_mode
    let mut in_logs_tab = false; // available in all modes if logs_rx is Some
    let mut polling_paused = false;
    let mut backends_sel: usize = 0;
    let mut adding_backend = false;
    let mut add_buffer = String::new();

    // Logs state
    let mut logs: VecDeque<String> = VecDeque::new();
    let mut logs_offset: usize = 0; // 0 = follow last lines
    const MAX_LOG_LINES: usize = 2000;

    // UI loop
    loop {
        // Drain incoming messages without blocking UI
        while let Ok(msg) = rx.try_recv() {
            match msg.body {
                MessageBody::Report(r) => app.apply_report(msg.host.clone(), r),
                MessageBody::Register(r) => app.apply_register(msg.host.clone(), r),
                MessageBody::Unregister(u) => app.apply_unregister(msg.host.clone(), u),
            }
        }
        // Periodically prune inactive watchers/executors/tenants
        if last_prune.elapsed() >= Duration::from_secs(5) {
            app.prune_inactive(Duration::from_secs(60));
            last_prune = Instant::now();
        }

        // Drain logs if provided
        if let Some(ref mut lrx) = logs_rx {
            while let Ok(line) = lrx.try_recv() {
                if logs.len() >= MAX_LOG_LINES {
                    let _ = logs.pop_front();
                }
                logs.push_back(line);
                // keep following unless user scrolled up
                if logs_offset == 0 {}
            }
        }

        let selected_watch = app.selected_watch_ids();
        let selected_tenant = app.selected_tenant_name();
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(size);

            if in_logs_tab {
                // Logs tab
                let title = "Logs [Tab switch | Up/Down scroll | c clear | q quit]".to_string();
                let header = Block::default().title(title).borders(Borders::ALL);
                f.render_widget(header, chunks[0]);
                // Determine visible window
                let area = chunks[1];
                let inner_height = area.height.saturating_sub(2) as usize; // rough estimate
                let total = logs.len();
                let start = if logs_offset == 0 {
                    total.saturating_sub(inner_height)
                } else {
                    total.saturating_sub(inner_height + logs_offset)
                };
                let mut display: String = String::new();
                for line in logs.iter().skip(start).take(inner_height) {
                    display.push_str(line);
                    display.push('\n');
                }
                let paragraph = Paragraph::new(display).block(Block::default().title("Recent logs").borders(Borders::ALL));
                f.render_widget(paragraph, chunks[1]);
                return;
            } else if aggregator_mode && in_backends_tab {
                // Backends management tab
                let title = if adding_backend {
                    format!(
                        "Backends Management [Tab switch | p {} | ENTER submit | ESC cancel] Add backend: {}",
                        if polling_paused { "resume" } else { "pause" },
                        add_buffer
                    )
                } else {
                    format!(
                        "Backends Management [Tab switch | p {} | a add | d delete]",
                        if polling_paused { "resume" } else { "pause" }
                    )
                };
                let header = Block::default().title(title).borders(Borders::ALL);
                f.render_widget(header, chunks[0]);
                // List backends and current state
                let mut rows: Vec<Row<'static>> = Vec::new();
                rows.push(Row::new(vec![
                    Line::from("Backend"),
                    Line::from("Port"),
                    Line::from("State"),
                ]).style(Style::default().add_modifier(Modifier::BOLD)));
                if backends_sel >= backends.len() && !backends.is_empty() {
                    backends_sel = backends.len() - 1;
                }
                let sel_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD);
                for (i, (addr, port)) in backends.iter().enumerate() {
                    let mut row = Row::new(vec![
                        Line::from(addr.clone()),
                        Line::from(port.to_string()),
                        Line::from(if polling_paused { "paused" } else { "running" }.to_string()),
                    ]);
                    if i == backends_sel { row = row.style(sel_style); }
                    rows.push(row);
                }
                let table = Table::new(
                    rows,
                    [Constraint::Percentage(60), Constraint::Percentage(20), Constraint::Percentage(20)],
                )
                .block(Block::default().title("Configured backends").borders(Borders::ALL));
                f.render_widget(table, chunks[1]);
                return;
            } else {
                // Main tab (existing UI)
                let tab_hint = if aggregator_mode {
                    if logs_available { "[Tab: Backends/Logs] " } else { "[Tab: Backends] " }
                } else if logs_available {
                    "[Tab: Logs] "
                } else {
                    ""
                };
                let header_title = format!(
                    "{}Watchers by Tenant/Executor (q quit, / tenant filter, h host filter, Esc/Enter exit filter, arrows navigate, Enter/Right fold/unfold) | {}",
                    tab_hint,
                    app.filters.to_string()
                );
                let header = Block::default().title(header_title).borders(Borders::ALL);
                f.render_widget(header, chunks[0]);

                // Build all rows
                let rows_full = app.visible_rows();
                // Clamp selection if rows shrink (e.g., after folding)
                if !rows_full.is_empty() && app.selected >= rows_full.len() {
                    app.selected = rows_full.len() - 1;
                }
                let selected_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD);
                let widths = [
                    Constraint::Percentage(40),
                    Constraint::Percentage(10),
                    Constraint::Percentage(15),
                    Constraint::Percentage(20),
                    Constraint::Percentage(15),
                ];

                // If a watcher is selected, show details pane with charts below the table
                if let Some((ref tenant_name, exec_key, watch_id)) = selected_watch {
                    let main_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                        .split(chunks[1]);
                    // render paginated table in top part
                    {
                        let table_area = main_chunks[0];
                        // approximate inner height: 2 for borders, 1 for header row
                        let page_size = table_area.height.saturating_sub(3) as usize;
                        if page_size == 0 {
                            // Render an empty table with header if no space
                            let empty: Vec<Row<'static>> = Vec::new();
                            let table = Table::new(empty, widths)
                                .header(Row::new(vec![
                                    Line::from("Name"),
                                    Line::from("Lag"),
                                    Line::from("Exec Time"),
                                    Line::from("Updated"),
                                    Line::from("Host"),
                                ]).style(Style::default().add_modifier(Modifier::BOLD)))
                                .block(Block::default().borders(Borders::ALL));
                            f.render_widget(table, table_area);
                        } else {
                            // Keep selected within scroll window
                            if app.selected < app.scroll_offset {
                                app.scroll_offset = app.selected;
                            } else if app.selected >= app.scroll_offset + page_size {
                                app.scroll_offset = app.selected + 1 - page_size;
                            }
                            let rows_slice: Vec<Row<'static>> = rows_full
                                .iter()
                                .cloned()
                                .enumerate()
                                .skip(app.scroll_offset)
                                .take(page_size)
                                .map(|(i, row)| if i == app.selected { row.clone().style(selected_style) } else { row })
                                .collect();
                            let table = Table::new(rows_slice, widths)
                                .header(Row::new(vec![
                                    Line::from("Name"),
                                    Line::from("Lag"),
                                    Line::from("Exec Time"),
                                    Line::from("Updated"),
                                    Line::from("Host"),
                                ]).style(Style::default().add_modifier(Modifier::BOLD)))
                                .block(Block::default().borders(Borders::ALL));
                            f.render_widget(table, table_area);
                        }
                    }

                    // Render details in bottom part
                    if let Some(tenant) = app.tenants.get(tenant_name) {
                        if let Some(exec) = tenant.executors.get(&exec_key) {
                            if let Some(watch) = exec.watchers.get(&watch_id) {
                                let detail_title = format!(
                                    "Details: {tenant_name} / Exec #{} @ {} / Watch #{} | Interest: {}",
                                    exec_key.0,
                                    exec_key.1,
                                    watch_id,
                                    if watch.interest.is_empty() { "<unknown>" } else { &watch.interest }
                                );
                                let detail_block = Block::default().title(detail_title).borders(Borders::ALL);
                                // Split details area into two charts: Lag and Exec Time
                                let detail_inner = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                                    .split(main_chunks[1]);

                                // Prepare data points aligned on Unix time
                                let times = &watch.time_hist;
                                let lag_points: Vec<(f64, f64)> = times
                                    .iter()
                                    .cloned()
                                    .zip(watch.lag_hist.iter().map(|v| *v as f64))
                                    .collect();
                                let exec_points: Vec<(f64, f64)> = times
                                    .iter()
                                    .cloned()
                                    .zip(watch.exec_hist.iter().cloned())
                                    .collect();

                                // Shared X range (unix seconds)
                                let x_min_ts = watch.time_hist.front().copied().unwrap_or_else(|| {
                                    { let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64(); (now - 10.0).max(0.0) }
                                });
                                let x_max_ts = watch.time_hist.back().copied().unwrap_or_else(|| {
                                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64()
                                });
                                let x_upper = if x_max_ts > x_min_ts { x_max_ts } else { x_min_ts + 1.0 };

                                // Render details header above the lag chart
                                let lag_chunks = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                                    .split(detail_inner[0]);
                                f.render_widget(detail_block.clone(), lag_chunks[0]);

                                // Lag sparkline with manual Y-axis labels (no X labels here)
                                let lag_area = lag_chunks[1];
                                let lag_layout = Layout::default()
                                    .direction(Direction::Horizontal)
                                    .constraints([Constraint::Length(8), Constraint::Min(0)].as_ref())
                                    .split(lag_area);
                                // Y-axis labels at left (top/mid/bottom)
                                let y_max_lag = watch.lag_hist.iter().copied().max().unwrap_or(1) as f64;
                                let y_left = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)].as_ref())
                                    .split(lag_layout[0]);
                                let top_lbl = format!("{:.0}", y_max_lag.max(1.0));
                                let mid_lbl = format!("{:.0}", y_max_lag.max(1.0) / 2.0);
                                let bot_lbl = String::from("0");
                                f.render_widget(Paragraph::new(Line::from(top_lbl)), y_left[0]);
                                // Center the middle label vertically within its area
                                let mid_center = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Min(0), Constraint::Length(1), Constraint::Min(0)].as_ref())
                                    .split(y_left[1]);
                                f.render_widget(Paragraph::new(Line::from(mid_lbl)), mid_center[1]);
                                f.render_widget(Paragraph::new(Line::from(bot_lbl)), y_left[2]);
                                // Plot area with sparkline
                                let lag_block = Block::default().title("Lag message over time").borders(Borders::ALL);
                                let lag_inner = lag_block.inner(lag_layout[1]);
                                f.render_widget(lag_block, lag_layout[1]);
                                let lag_vals_src: Vec<u64> = watch.lag_hist.iter().copied().collect();
                                let lag_max = lag_vals_src.iter().copied().max().unwrap_or(1);
                                let lag_vals = resample_to_width_u64(&lag_vals_src, lag_inner.width as usize);
                                let lag_spark = Sparkline::default()
                                    .data(&lag_vals)
                                    .max(lag_max)
                                    .style(Style::default().fg(Color::Red));
                                f.render_widget(lag_spark, lag_inner);

                                // Exec time sparkline with manual Y-axis and X-axis labels (human-readable time)
                                // Layout: top row is plot (with left Y labels and right sparkline in a block), bottom row is X labels
                                let exec_area = detail_inner[1];
                                let exec_v = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
                                    .split(exec_area);
                                // Row for plot: split y labels and sparkline
                                let exec_row = Layout::default()
                                    .direction(Direction::Horizontal)
                                    .constraints([Constraint::Length(8), Constraint::Min(0)].as_ref())
                                    .split(exec_v[0]);
                                // Y-axis labels
                                let y_max_exec = watch.exec_hist.iter().cloned().fold(0.0_f64, f64::max);
                                let y_left_e = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)].as_ref())
                                    .split(exec_row[0]);
                                let top_e = format!("{:.0}", y_max_exec.max(1.0));
                                let mid_e = format!("{:.0}", y_max_exec.max(1.0) / 2.0);
                                let bot_e = String::from("0");
                                f.render_widget(Paragraph::new(Line::from(top_e)), y_left_e[0]);
                                // Center the middle label vertically within its area
                                let mid_center_e = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([Constraint::Min(0), Constraint::Length(1), Constraint::Min(0)].as_ref())
                                    .split(y_left_e[1]);
                                f.render_widget(Paragraph::new(Line::from(mid_e)), mid_center_e[1]);
                                f.render_widget(Paragraph::new(Line::from(bot_e)), y_left_e[2]);
                                // Sparkline plot with block
                                let exec_block = Block::default().title("Exec Time (ms)").borders(Borders::ALL);
                                let exec_inner = exec_block.inner(exec_row[1]);
                                f.render_widget(exec_block, exec_row[1]);
                                let exec_vals_src: Vec<u64> = watch.exec_hist.iter().map(|v| v.max(0.0).round() as u64).collect();
                                let exec_max = exec_vals_src.iter().copied().max().unwrap_or(1);
                                let exec_vals = resample_to_width_u64(&exec_vals_src, exec_inner.width as usize);
                                let exec_spark = Sparkline::default()
                                    .data(&exec_vals)
                                    .max(exec_max)
                                    .style(Style::default().fg(Color::Green));
                                f.render_widget(exec_spark, exec_inner);
                                // X-axis human-readable time labels under the plot, centered in thirds
                                use chrono::{Local, TimeZone};
                                let fmt_time = |ts: f64| -> String {
                                    let secs = ts.floor() as i64;
                                    let dt = match Local.timestamp_opt(secs, 0) { chrono::LocalResult::Single(dt) => dt, _ => Local.timestamp(0, 0) };
                                    dt.format("%H:%M:%S").to_string()
                                };
                                let mid_ts = (x_min_ts + x_upper) / 2.0;
                                let labels_row = Layout::default()
                                    .direction(Direction::Horizontal)
                                    .constraints([
                                        Constraint::Percentage(33),
                                        Constraint::Percentage(34),
                                        Constraint::Percentage(33),
                                    ].as_ref())
                                    .split(exec_v[1]);
                                let left_lbl = Paragraph::new(Line::from(fmt_time(x_min_ts))).alignment(Alignment::Center);
                                let mid_lbl = Paragraph::new(Line::from(fmt_time(mid_ts))).alignment(Alignment::Center);
                                let right_lbl = Paragraph::new(Line::from(fmt_time(x_upper))).alignment(Alignment::Center);
                                f.render_widget(left_lbl, labels_row[0]);
                                f.render_widget(mid_lbl, labels_row[1]);
                                f.render_widget(right_lbl, labels_row[2]);
                            } else {
                                // watcher missing
                            }
                        } else {
                            // executor missing
                        }
                    } else {
                        // tenant missing
                    }
                } else if let Some(ref tenant_name) = selected_tenant {
                    // Tenant selected: show details summary below table
                    let main_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                        .split(chunks[1]);
                    // render paginated table in top part
                    {
                        let table_area = main_chunks[0];
                        let page_size = table_area.height.saturating_sub(3) as usize;
                        if page_size == 0 {
                            let empty: Vec<Row<'static>> = Vec::new();
                            let table = Table::new(empty, widths)
                                .header(Row::new(vec![
                                    Line::from("Name"),
                                    Line::from("Lag"),
                                    Line::from("Exec Time"),
                                    Line::from("Updated"),
                                    Line::from("Host"),
                                ]).style(Style::default().add_modifier(Modifier::BOLD)))
                                .block(Block::default().borders(Borders::ALL));
                            f.render_widget(table, table_area);
                        } else {
                            if app.selected < app.scroll_offset {
                                app.scroll_offset = app.selected;
                            } else if app.selected >= app.scroll_offset + page_size {
                                app.scroll_offset = app.selected + 1 - page_size;
                            }
                            let rows_slice: Vec<Row<'static>> = rows_full
                                .iter()
                                .cloned()
                                .enumerate()
                                .skip(app.scroll_offset)
                                .take(page_size)
                                .map(|(i, row)| if i == app.selected { row.clone().style(selected_style) } else { row })
                                .collect();
                            let table = Table::new(rows_slice, widths)
                                .header(Row::new(vec![
                                    Line::from("Name"),
                                    Line::from("Lag"),
                                    Line::from("Exec Time"),
                                    Line::from("Updated"),
                                    Line::from("Host"),
                                ]).style(Style::default().add_modifier(Modifier::BOLD)))
                                .block(Block::default().borders(Borders::ALL));
                            f.render_widget(table, table_area);
                        }
                    }

                    // compute tenant summary filtered by host
                    let mut num_exec = 0usize;
                    let mut num_watch = 0usize;
                    let mut sum_exec_time = 0.0f64;
                    let mut sum_lag: u128 = 0;
                    if let Some(tenant) = app.tenants.get(tenant_name) {
                        let host_filt = if app.host_filter.is_empty() { None } else { Some(app.host_filter.to_lowercase()) };
                        for (_k, exec) in &tenant.executors {
                            if host_filt.as_ref().map(|hf| exec.host.to_lowercase().contains(hf)).unwrap_or(true) {
                                num_exec += 1;
                                num_watch += exec.watchers.len();
                                for w in exec.watchers.values() {
                                    sum_exec_time += w.execution_time;
                                    sum_lag += w.lag as u128;
                                }
                            }
                        }
                    }
                    let mean_exec = if num_watch > 0 { sum_exec_time / (num_watch as f64) } else { 0.0 };
                    let mean_lag: u64 = if num_watch > 0 { (sum_lag as f64 / num_watch as f64).round() as u64 } else { 0 };
                    let bullet = "•";
                    let details_text = format!(
                        "{b} Number of executors: {ne}\n{b} Number of watchers: {nw}\n{b} Mean execution time: {me:.3} ms\n{b} Mean lag: {ml}",
                        b=bullet, ne=num_exec, nw=num_watch, me=mean_exec, ml=mean_lag
                    );
                    let details = Paragraph::new(details_text)
                        .block(Block::default().title(format!("Tenant details: {}", tenant_name)).borders(Borders::ALL));
                    f.render_widget(details, main_chunks[1]);
                } else {
                    // No watcher or tenant selected, render full-height table (paginated)
                    let table_area = chunks[1];
                    let page_size = table_area.height.saturating_sub(3) as usize;
                    if page_size == 0 {
                        let empty: Vec<Row<'static>> = Vec::new();
                        let table = Table::new(empty, widths)
                            .header(Row::new(vec![
                                Line::from("Name"),
                                Line::from("Lag"),
                                Line::from("Exec Time"),
                                Line::from("Updated"),
                                Line::from("Host"),
                            ]).style(Style::default().add_modifier(Modifier::BOLD)))
                            .block(Block::default().borders(Borders::ALL));
                        f.render_widget(table, table_area);
                    } else {
                        if app.selected < app.scroll_offset {
                            app.scroll_offset = app.selected;
                        } else if app.selected >= app.scroll_offset + page_size {
                            app.scroll_offset = app.selected + 1 - page_size;
                        }
                        let rows_slice: Vec<Row<'static>> = rows_full
                            .iter()
                            .cloned()
                            .enumerate()
                            .skip(app.scroll_offset)
                            .take(page_size)
                            .map(|(i, row)| if i == app.selected { row.clone().style(selected_style) } else { row })
                            .collect();
                        let table = Table::new(rows_slice, widths)
                            .header(Row::new(vec![
                                Line::from("Name"),
                                Line::from("Lag"),
                                Line::from("Exec Time"),
                                Line::from("Updated"),
                                Line::from("Host"),
                            ]).style(Style::default().add_modifier(Modifier::BOLD)))
                            .block(Block::default().borders(Borders::ALL));
                        f.render_widget(table, table_area);
                    }
                }
            }
        })?;

        // Input handling with small timeout to keep UI responsive
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    use ratatui::crossterm::event::KeyCode;
                    if app.filters.is_filter_active(FilterType::Tenant)
                        || app.filters.is_filter_active(FilterType::Host)
                    {
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('/') => {
                                app.filters.clear_filter();
                            }
                            KeyCode::Backspace => {
                                app.filters.pop_char();
                                app.selected = 0;
                            }
                            KeyCode::Char(c) => {
                                if !c.is_control() {
                                    app.filters.push_char(c);
                                    // if filtering_host {
                                    //     app.host_filter.push(c);
                                    // } else {
                                    //     app.filter.push(c);
                                    // }
                                    app.selected = 0;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        // Not in text filtering modes
                        if in_logs_tab {
                            match key.code {
                                KeyCode::Tab => {
                                    // Cycle: Logs -> Main if no aggregator, otherwise Logs -> Main
                                    in_logs_tab = false;
                                }
                                KeyCode::Char('q') => break,
                                KeyCode::Char('c') => {
                                    logs.clear();
                                    logs_offset = 0;
                                }
                                KeyCode::Up => {
                                    // scroll up
                                    if logs.len() > 0 {
                                        // increase offset up to max
                                        let max_off = logs.len().saturating_sub(1);
                                        if logs_offset < max_off {
                                            logs_offset += 1;
                                        }
                                    }
                                }
                                KeyCode::Down => {
                                    if logs_offset > 0 {
                                        logs_offset -= 1;
                                    }
                                }
                                _ => {}
                            }
                        } else if aggregator_mode && in_backends_tab {
                            // Backends tab key handling
                            match key.code {
                                KeyCode::Tab => {
                                    if logs_available {
                                        in_backends_tab = false;
                                        in_logs_tab = true;
                                    } else {
                                        in_backends_tab = false;
                                    }
                                }
                                KeyCode::Char('p') => {
                                    polling_paused = !polling_paused;
                                    if let Some(ref txc) = control_tx_opt {
                                        let _ = if polling_paused {
                                            txc.send(PollControl::Pause)
                                        } else {
                                            txc.send(PollControl::Resume)
                                        };
                                    }
                                }
                                KeyCode::Char('q') => break,
                                code => {
                                    if adding_backend {
                                        match code {
                                            KeyCode::Esc => {
                                                adding_backend = false;
                                                add_buffer.clear();
                                            }
                                            KeyCode::Enter => {
                                                if let Some((addr, port_str)) =
                                                    add_buffer.split_once(':')
                                                {
                                                    if let Ok(port) = port_str.parse::<u16>() {
                                                        let addr_s = addr.to_string();
                                                        if !backends.iter().any(|(a, p)| {
                                                            a == &addr_s && *p == port
                                                        }) {
                                                            backends.push((addr_s.clone(), port));
                                                            if let Some(ref txc) = control_tx_opt {
                                                                let _ = txc.send(
                                                                    PollControl::AddBackend(
                                                                        addr_s.clone(),
                                                                        port,
                                                                    ),
                                                                );
                                                            }
                                                            backends_sel =
                                                                backends.len().saturating_sub(1);
                                                        }
                                                    }
                                                }
                                                adding_backend = false;
                                                add_buffer.clear();
                                            }
                                            KeyCode::Backspace => {
                                                add_buffer.pop();
                                            }
                                            KeyCode::Char(c) => {
                                                if !c.is_control() {
                                                    add_buffer.push(c);
                                                }
                                            }
                                            _ => {}
                                        }
                                    } else {
                                        match code {
                                            KeyCode::Char('a') => {
                                                adding_backend = true;
                                                add_buffer.clear();
                                            }
                                            KeyCode::Char('d') => {
                                                if !backends.is_empty()
                                                    && backends_sel < backends.len()
                                                {
                                                    let (addr, port) =
                                                        backends[backends_sel].clone();
                                                    if let Some(ref txc) = control_tx_opt {
                                                        let _ =
                                                            txc.send(PollControl::RemoveBackend(
                                                                addr.clone(),
                                                                port,
                                                            ));
                                                    }
                                                    backends.remove(backends_sel);
                                                    if backends_sel >= backends.len()
                                                        && backends_sel > 0
                                                    {
                                                        backends_sel -= 1;
                                                    }
                                                }
                                            }
                                            KeyCode::Down => {
                                                if backends_sel + 1 < backends.len() {
                                                    backends_sel += 1;
                                                }
                                            }
                                            KeyCode::Up => {
                                                if backends_sel > 0 {
                                                    backends_sel -= 1;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        } else {
                            // Main tab key handling
                            match key.code {
                                KeyCode::Tab => {
                                    if aggregator_mode {
                                        in_backends_tab = true;
                                    } else if logs_available {
                                        in_logs_tab = true;
                                    }
                                }
                                KeyCode::Char('q') => break,
                                KeyCode::Char('/') => {
                                    app.filters.set_filter(FilterType::Tenant);
                                    // filtering = true;
                                    // filtering_host = false;
                                    app.selected = 0;
                                }
                                KeyCode::Char('h') => {
                                    app.filters.set_filter(FilterType::Host);
                                    app.selected = 0;
                                }
                                KeyCode::Down => {
                                    let max = app.visible_rows().len();
                                    if app.selected + 1 < max {
                                        app.selected += 1;
                                    }
                                }
                                KeyCode::Up => {
                                    if app.selected > 0 {
                                        app.selected -= 1;
                                    }
                                }
                                KeyCode::Right | KeyCode::Enter | KeyCode::Left => {
                                    app.toggle_fold_at(app.selected);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        terminal::LeaveAlternateScreen,
        ratatui::crossterm::cursor::Show
    )?;

    Ok(())
}
