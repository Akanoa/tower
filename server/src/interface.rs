use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Row, Table};
use ratatui::Terminal;
use tokio::sync::mpsc;

use protocol::{Message, MessageBody, MessageRegister, MessageReport, MessageUnregister};

pub type MessageReceiver = mpsc::UnboundedReceiver<Message>;

#[derive(Debug, Clone)]
pub struct WatchItem {
    pub watch_id: i64,
    pub lag: u64,
    pub execution_time: f64,
    pub updated_at: Instant,
    pub interest: String,
    pub lag_hist: VecDeque<u64>,
    pub exec_hist: VecDeque<f64>,
    pub time_hist: VecDeque<f64>,
}

#[derive(Debug, Default, Clone)]
pub struct ExecutorItem {
    pub executor_id: i64,
    pub folded: bool,
    pub watchers: BTreeMap<i64, WatchItem>,
    pub host: String,
}

#[derive(Debug, Default, Clone)]
pub struct TenantItem {
    pub tenant: String,
    pub folded: bool,
    pub executors: BTreeMap<(i64, String), ExecutorItem>,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub tenants: BTreeMap<String, TenantItem>,
    // index of currently selected visible row in flat view
    pub selected: usize,
    // current tenant filter (case-insensitive contains)
    pub filter: String,
    // current host filter (case-insensitive contains)
    pub host_filter: String,
}

impl AppState {
    pub fn apply_report(&mut self, host: String, report: MessageReport) {
        let tenant_entry = self
            .tenants
            .entry(report.tenant.clone())
            .or_insert_with(|| TenantItem {
                tenant: report.tenant.clone(),
                folded: false,
                executors: BTreeMap::new(),
            });
        let exec_entry = tenant_entry
            .executors
            .entry((report.executor_id, host.clone()))
            .or_insert_with(|| ExecutorItem {
                executor_id: report.executor_id,
                folded: false,
                watchers: BTreeMap::new(),
                host: host.clone(),
            });
        // keep host updated
        exec_entry.host = host.clone();
        let watch = exec_entry
            .watchers
            .entry(report.watch_id)
            .or_insert_with(|| WatchItem {
                watch_id: report.watch_id,
                lag: 0,
                execution_time: 0.0,
                updated_at: Instant::now(),
                interest: report.interest.clone(),
                lag_hist: VecDeque::new(),
                exec_hist: VecDeque::new(),
                time_hist: VecDeque::new(),
            });
        // Update fields
        watch.lag = report.lag;
        watch.execution_time = report.execution_time;
        watch.updated_at = Instant::now();
        // Update interest if it changed or was empty
        if watch.interest.is_empty() {
            watch.interest = report.interest.clone();
        }
        // Append to history with a cap to avoid unbounded growth
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

    pub fn apply_register(&mut self, host: String, reg: MessageRegister) {
        let tenant_entry = self
            .tenants
            .entry(reg.tenant.clone())
            .or_insert_with(|| TenantItem {
                tenant: reg.tenant.clone(),
                folded: false,
                executors: BTreeMap::new(),
            });
        let exec_entry = tenant_entry
            .executors
            .entry((reg.executor_id, host.clone()))
            .or_insert_with(|| ExecutorItem {
                executor_id: reg.executor_id,
                folded: false,
                watchers: BTreeMap::new(),
                host: host.clone(),
            });
        exec_entry.host = host.clone();
        exec_entry
            .watchers
            .entry(reg.watch_id)
            .or_insert_with(|| WatchItem {
                watch_id: reg.watch_id,
                lag: 0,
                execution_time: 0.0,
                updated_at: Instant::now(),
                interest: String::new(),
                lag_hist: VecDeque::new(),
                exec_hist: VecDeque::new(),
                time_hist: VecDeque::new(),
            });
    }

    pub fn apply_unregister(&mut self, host: String, unreg: MessageUnregister) {
        if let Some(tenant) = self.tenants.get_mut(&unreg.tenant) {
            if let Some(exec) = tenant.executors.get_mut(&(unreg.executor_id, host.clone())) {
                exec.host = host;
                exec.watchers.remove(&unreg.watch_id);
            }
        }
    }

    // Remove inactive watchers (> max_age since last update) and empty executors/tenants
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

    // Build visible rows respecting folding
    pub fn visible_rows(&self) -> Vec<Row<'static>> {
        let mut rows = Vec::new();
        let tenant_filt = if self.filter.is_empty() { None } else { Some(self.filter.to_lowercase()) };
        let host_filt = if self.host_filter.is_empty() { None } else { Some(self.host_filter.to_lowercase()) };
        for (tenant_name, tenant) in &self.tenants {
            if let Some(ref f) = tenant_filt {
                if !tenant_name.to_lowercase().contains(f) {
                    continue;
                }
            }
            // Compute which executors are visible under host filter
            let exec_iter = tenant.executors.iter().filter(|(_id, exec)| {
                if let Some(ref hf) = host_filt {
                    exec.host.to_lowercase().contains(hf)
                } else {
                    true
                }
            });
            // We need to peek whether there is any executor to show; collect ids temporarily
            let visible_execs: Vec<(&(i64, String), &ExecutorItem)> = exec_iter.collect();
            if visible_execs.is_empty() {
                // Skip tenant entirely if no executor matches host filter
                continue;
            }
            let t_prefix = if tenant.folded { "▸" } else { "▾" };
            // Compute tenant-level means based on means of its visible executors (with watchers)
            let (tenant_mean_lag, tenant_mean_exec) = {
                let mut sum_mean_lag = 0.0_f64;
                let mut sum_mean_exec = 0.0_f64;
                let mut count = 0.0_f64;
                for (_key, exec) in &visible_execs {
                    let n = exec.watchers.len() as f64;
                    if n > 0.0 {
                        let sum_lag: u128 = exec.watchers.values().map(|w| w.lag as u128).sum();
                        let sum_exec: f64 = exec.watchers.values().map(|w| w.execution_time).sum();
                        let mean_lag = sum_lag as f64 / n;
                        let mean_exec = sum_exec / n;
                        sum_mean_lag += mean_lag;
                        sum_mean_exec += mean_exec;
                        count += 1.0;
                    }
                }
                if count > 0.0 {
                    ((sum_mean_lag / count).round() as u64, sum_mean_exec / count)
                } else {
                    (0u64, 0.0f64)
                }
            };
            rows.push(
                Row::new(vec![
                    Line::from(format!("{t_prefix} Tenant: {tenant_name}")),
                    Line::from(format!("{}", tenant_mean_lag)),
                    Line::from(format!("{:.3} ms", tenant_mean_exec)),
                    Line::from(""),
                    Line::from(""),
                ])
                .style(Style::default().add_modifier(Modifier::BOLD)),
            );
            if tenant.folded {
                continue;
            }
            for (exec_id, exec) in visible_execs {
                let e_prefix = if exec.folded { "  ▸" } else { "  ▾" };
                // Compute means from current watcher values
                let n = exec.watchers.len() as f64;
                let (mean_lag, mean_exec) = if n > 0.0 {
                    let sum_lag: u128 = exec.watchers.values().map(|w| w.lag as u128).sum();
                    let sum_exec: f64 = exec.watchers.values().map(|w| w.execution_time).sum();
                    ((sum_lag as f64 / n).round() as u64, sum_exec / n)
                } else {
                    (0u64, 0.0f64)
                };
                rows.push(Row::new(vec![
                    Line::from(format!("{e_prefix} Executor #{}", exec_id.0)), 
                    Line::from(format!("{}", mean_lag)),
                    Line::from(format!("{:.3} ms", mean_exec)),
                    Line::from(""),
                    Line::from(exec.host.clone()),
                ]));
                if exec.folded {
                    continue;
                }
                for (_watch_id, watch) in &exec.watchers {
                    rows.push(Row::new(vec![
                        Line::from(format!("      Watch #{:}", watch.watch_id)),
                        Line::from(format!("{}", watch.lag)),
                        Line::from(format!("{:.3} ms", watch.execution_time)),
                        Line::from(format!(
                            "{:?}",
                            Instant::now().saturating_duration_since(watch.updated_at)
                        )),
                        Line::from(""),
                    ]));
                }
            }
        }
        rows
    }

    // Toggle fold state for the item at the visible index, if it's a tenant or executor row
    pub fn toggle_fold_at(&mut self, index: usize) {
        let mut i = 0usize;
        let tenant_filt = if self.filter.is_empty() { None } else { Some(self.filter.to_lowercase()) };
        let host_filt = if self.host_filter.is_empty() { None } else { Some(self.host_filter.to_lowercase()) };
        for (tenant_name, tenant) in self.tenants.iter_mut() {
            if let Some(ref f) = tenant_filt {
                if !tenant_name.to_lowercase().contains(f) { continue; }
            }
            // determine if tenant has any execs visible under host filter
            let mut any_exec_visible = false;
            for (_eid, e) in tenant.executors.iter() {
                if host_filt.as_ref().map(|hf| e.host.to_lowercase().contains(hf)).unwrap_or(true) {
                    any_exec_visible = true;
                    break;
                }
            }
            if !any_exec_visible { continue; }
            if i == index {
                tenant.folded = !tenant.folded;
                return;
            }
            i += 1;
            if tenant.folded { continue; }
            for (_exec_id, exec) in tenant.executors.iter_mut() {
                if let Some(ref hf) = host_filt {
                    if !exec.host.to_lowercase().contains(hf) { continue; }
                }
                if i == index {
                    exec.folded = !exec.folded;
                    return;
                }
                i += 1;
                if exec.folded { continue; }
                // skip watcher rows
                i += exec.watchers.len();
            }
        }
    }

    // Resolve current selection to a watcher identifier if selection points to a watcher row
    pub fn selected_watch_ids(&self) -> Option<(String, (i64, String), i64)> {
        let mut i = 0usize;
        let tenant_filt = if self.filter.is_empty() { None } else { Some(self.filter.to_lowercase()) };
        let host_filt = if self.host_filter.is_empty() { None } else { Some(self.host_filter.to_lowercase()) };
        for (tenant_name, tenant) in &self.tenants {
            if let Some(ref f) = tenant_filt {
                if !tenant_name.to_lowercase().contains(f) {
                    continue;
                }
            }
            // Only consider tenants with at least one visible executor under host filter
            let mut any_exec_visible = false;
            for (_eid, e) in tenant.executors.iter() {
                if host_filt.as_ref().map(|hf| e.host.to_lowercase().contains(hf)).unwrap_or(true) {
                    any_exec_visible = true; break;
                }
            }
            if !any_exec_visible { continue; }
            if i == self.selected {
                // tenant row selected
                return None;
            }
            i += 1;
            if tenant.folded {
                continue;
            }
            for (exec_id, exec) in &tenant.executors {
                if let Some(ref hf) = host_filt {
                    if !exec.host.to_lowercase().contains(hf) { continue; }
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
}

pub async fn run_tui(
    mut rx: MessageReceiver,
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
    let mut filtering = false;
    let mut filtering_host = false;
    let mut last_prune = Instant::now();

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

        let selected_watch = app.selected_watch_ids();
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(size);

            let header_title = format!(
                "Watchers by Tenant/Executor (q quit, / tenant filter, h host filter, Esc/Enter exit filter, arrows navigate, Enter/Right fold/unfold) | Tenant{}: {} | Host{}: {}",
                if filtering { " [typing]" } else { "" },
                if app.filter.is_empty() { "<none>".to_string() } else { app.filter.clone() },
                if filtering_host { " [typing]" } else { "" },
                if app.host_filter.is_empty() { "<none>".to_string() } else { app.host_filter.clone() }
            );
            let header = Block::default().title(header_title).borders(Borders::ALL);
            f.render_widget(header, chunks[0]);

            // Build rows and apply highlight to the selected one
            let mut rows = app.visible_rows();
            // Clamp selection if rows shrink (e.g., after folding)
            if !rows.is_empty() && app.selected >= rows.len() {
                app.selected = rows.len() - 1;
            }
            let selected_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD);
            let rows: Vec<Row<'static>> = rows
                .into_iter()
                .enumerate()
                .map(|(i, row)| if i == app.selected { row.style(selected_style) } else { row })
                .collect();

            let widths = [
                Constraint::Percentage(40),
                Constraint::Percentage(10),
                Constraint::Percentage(15),
                Constraint::Percentage(20),
                Constraint::Percentage(15),
            ];

            let table = Table::new(rows, widths)
                .header(Row::new(vec![
                    Line::from("Name"),
                    Line::from("Lag"),
                    Line::from("Exec Time"),
                    Line::from("Updated"),
                    Line::from("Host"),
                ]).style(Style::default().add_modifier(Modifier::BOLD)))
                .block(Block::default().borders(Borders::ALL));

            // If a watcher is selected, show details pane with charts below the table
            if let Some((ref tenant_name, exec_key, watch_id)) = selected_watch {
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
                    .split(chunks[1]);
                // render table in top part
                f.render_widget(&table, main_chunks[0]);

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

                            // Lag chart (no X labels to avoid duplication; Y labels shown)
                            let y_max_lag = watch.lag_hist.iter().copied().max().unwrap_or(1) as f64;
                            let lag_chart = if lag_points.is_empty() {
                                Chart::new(vec![] as Vec<Dataset>)
                                    .block(Block::default().title("Lag message over time").borders(Borders::ALL))
                                    .x_axis(
                                        Axis::default()
                                            .bounds([
                                                (x_upper - 10.0).max(0.0),
                                                x_upper,
                                            ])
                                    )
                                    .y_axis(
                                        Axis::default()
                                            .bounds([0.0, 1.0])
                                            .labels(vec![Span::from("0"), Span::from("0.5"), Span::from("1.0")])
                                    )
                            } else {
                                Chart::new(vec![
                                    Dataset::default()
                                        .name("Lag")
                                        .graph_type(GraphType::Line)
                                        .data(&lag_points),
                                ])
                                .block(Block::default().title("Lag message over time").borders(Borders::ALL))
                                .x_axis(
                                    Axis::default()
                                        .bounds([x_min_ts, x_upper])
                                )
                                .y_axis(
                                    Axis::default()
                                        .bounds([0.0, y_max_lag.max(1.0)])
                                        .labels({
                                            let max = y_max_lag.max(1.0);
                                            vec![Span::from("0"), Span::from(format!("{:.0}", max/2.0)), Span::from(format!("{:.0}", max))]
                                        })
                                )
                            };
                            f.render_widget(lag_chart, lag_chunks[1]);

                            // Exec time chart (shows human-readable time on X axis)
                            let y_max_exec = watch.exec_hist.iter().cloned().fold(0.0_f64, f64::max);
                            let exec_chart = if exec_points.is_empty() {
                                Chart::new(vec![] as Vec<Dataset>)
                                    .block(Block::default().title("Exec Time (ms)").borders(Borders::ALL))
                                    .x_axis(
                                        Axis::default()
                                            .bounds([
                                                (x_upper - 10.0).max(0.0),
                                                x_upper,
                                            ])
                                            .labels({
                                                use chrono::{Local, TimeZone};
                                                let x0 = (x_upper - 10.0).max(0.0);
                                                let mid = (x0 + x_upper) / 2.0;
                                                let fmt = |ts: f64| -> String {
                                                    let secs = ts.floor() as i64;
                                                    let dt = match Local.timestamp_opt(secs, 0) { chrono::LocalResult::Single(dt) => dt, _ => Local.timestamp(0, 0) };
                                                    dt.format("%H:%M:%S").to_string()
                                                };
                                                vec![Span::from(fmt(x0)), Span::from(fmt(mid)), Span::from(fmt(x_upper))]
                                            })
                                    )
                                    .y_axis(
                                        Axis::default()
                                            .bounds([0.0, 1.0])
                                            .labels(vec![Span::from("0"), Span::from("0.5"), Span::from("1.0")])
                                    )
                            } else {
                                Chart::new(vec![
                                    Dataset::default()
                                        .name("Exec ms")
                                        .graph_type(GraphType::Line)
                                        .data(&exec_points),
                                ])
                                .block(Block::default().title("Exec Time (ms)").borders(Borders::ALL))
                                .x_axis(
                                    Axis::default()
                                        .bounds([x_min_ts, x_upper])
                                        .labels({
                                            use chrono::{Local, TimeZone};
                                            let mid = (x_min_ts + x_upper) / 2.0;
                                            let fmt = |ts: f64| -> String {
                                                let secs = ts.floor() as i64;
                                                let dt = match Local.timestamp_opt(secs, 0) { chrono::LocalResult::Single(dt) => dt, _ => Local.timestamp(0, 0) };
                                                dt.format("%H:%M:%S").to_string()
                                            };
                                            vec![Span::from(fmt(x_min_ts)), Span::from(fmt(mid)), Span::from(fmt(x_upper))]
                                        })
                                )
                                .y_axis(
                                    Axis::default()
                                        .bounds([0.0, y_max_exec.max(1.0)])
                                        .labels({
                                            let max = y_max_exec.max(1.0);
                                            vec![Span::from("0"), Span::from(format!("{:.0}", max/2.0)), Span::from(format!("{:.0}", max))]
                                        })
                                )
                            };
                            f.render_widget(exec_chart, detail_inner[1]);
                        } else {
                            // watcher missing
                        }
                    } else {
                        // executor missing
                    }
                } else {
                    // tenant missing
                }
            } else {
                // No watcher selected, render full-height table
                f.render_widget(&table, chunks[1]);
            }
        })?;

        // Input handling with small timeout to keep UI responsive
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    use ratatui::crossterm::event::KeyCode;
                    if filtering || filtering_host {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('/') | KeyCode::Char('q') => {
                                filtering = false;
                                filtering_host = false;
                            }
                            KeyCode::Backspace => {
                                if filtering_host {
                                    app.host_filter.pop();
                                } else {
                                    app.filter.pop();
                                }
                                app.selected = 0;
                            }
                            KeyCode::Char(c) => {
                                if !c.is_control() {
                                    if filtering_host {
                                        app.host_filter.push(c);
                                    } else {
                                        app.filter.push(c);
                                    }
                                    app.selected = 0;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('/') => {
                                filtering = true;
                                filtering_host = false;
                                app.selected = 0;
                            }
                            KeyCode::Char('h') => {
                                filtering_host = true;
                                filtering = false;
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
