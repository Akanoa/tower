use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Row, Table};
use ratatui::Terminal;
use tokio::sync::mpsc;

use protocol::{Message, MessageRegister, MessageReport, MessageUnregister};

pub type MessageReceiver = mpsc::UnboundedReceiver<Message>;

#[derive(Debug, Clone)]
pub struct WatchItem {
    pub watch_id: i64,
    pub lag: u64,
    pub execution_time: f64,
    pub updated_at: Instant,
}

#[derive(Debug, Default, Clone)]
pub struct ExecutorItem {
    pub executor_id: i64,
    pub folded: bool,
    pub watchers: BTreeMap<i64, WatchItem>,
}

#[derive(Debug, Default, Clone)]
pub struct TenantItem {
    pub tenant: String,
    pub folded: bool,
    pub executors: BTreeMap<i64, ExecutorItem>,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub tenants: BTreeMap<String, TenantItem>,
    // index of currently selected visible row in flat view
    pub selected: usize,
    // current tenant filter (case-insensitive contains)
    pub filter: String,
}

impl AppState {
    pub fn apply_report(&mut self, report: MessageReport) {
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
            .entry(report.executor_id)
            .or_insert_with(|| ExecutorItem {
                executor_id: report.executor_id,
                folded: false,
                watchers: BTreeMap::new(),
            });
        let watch = exec_entry
            .watchers
            .entry(report.watch_id)
            .or_insert_with(|| WatchItem {
                watch_id: report.watch_id,
                lag: 0,
                execution_time: 0.0,
                updated_at: Instant::now(),
            });
        watch.lag = report.lag;
        watch.execution_time = report.execution_time;
        watch.updated_at = Instant::now();
    }

    pub fn apply_register(&mut self, reg: MessageRegister) {
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
            .entry(reg.executor_id)
            .or_insert_with(|| ExecutorItem {
                executor_id: reg.executor_id,
                folded: false,
                watchers: BTreeMap::new(),
            });
        exec_entry.watchers.entry(reg.watch_id).or_insert_with(|| WatchItem {
            watch_id: reg.watch_id,
            lag: 0,
            execution_time: 0.0,
            updated_at: Instant::now(),
        });
    }

    pub fn apply_unregister(&mut self, unreg: MessageUnregister) {
        if let Some(tenant) = self.tenants.get_mut(&unreg.tenant) {
            if let Some(exec) = tenant.executors.get_mut(&unreg.executor_id) {
                exec.watchers.remove(&unreg.watch_id);
            }
        }
    }

    // Build visible rows respecting folding
    pub fn visible_rows(&self) -> Vec<Row<'static>> {
        let mut rows = Vec::new();
        let filt = if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.to_lowercase())
        };
        for (tenant_name, tenant) in &self.tenants {
            if let Some(ref f) = filt {
                if !tenant_name.to_lowercase().contains(f) {
                    continue;
                }
            }
            let t_prefix = if tenant.folded { "▸" } else { "▾" };
            rows.push(
                Row::new(vec![
                    Line::from(format!("{t_prefix} Tenant: {tenant_name}")),
                    Line::from(""),
                    Line::from(""),
                    Line::from(""),
                ])
                .style(Style::default().add_modifier(Modifier::BOLD)),
            );
            if tenant.folded {
                continue;
            }
            for (exec_id, exec) in &tenant.executors {
                let e_prefix = if exec.folded { "  ▸" } else { "  ▾" };
                rows.push(Row::new(vec![
                    Line::from(format!("{e_prefix} Executor #{exec_id}")),
                    Line::from(""),
                    Line::from(""),
                    Line::from(""),
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
                    ]));
                }
            }
        }
        rows
    }

    // Toggle fold state for the item at the visible index, if it's a tenant or executor row
    pub fn toggle_fold_at(&mut self, index: usize) {
        let mut i = 0usize;
        for (_tenant_name, tenant) in self.tenants.iter_mut() {
            if i == index {
                tenant.folded = !tenant.folded;
                return;
            }
            i += 1;
            if tenant.folded {
                continue;
            }
            for (_exec_id, exec) in tenant.executors.iter_mut() {
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

    // UI loop
    loop {
        // Drain incoming messages without blocking UI
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Message::Report(r) => app.apply_report(r),
                Message::Register(r) => app.apply_register(r),
                Message::Unregister(u) => app.apply_unregister(u),
            }
        }

        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(size);

            let header_title = format!(
                "Watchers by Tenant/Executor (q quit, / toggle filter, Esc/Enter exit filter, arrows navigate, Enter/Right fold/unfold) | Filter{}: {}",
                if filtering { " [typing]" } else { "" },
                if app.filter.is_empty() { "<none>".to_string() } else { app.filter.clone() }
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
                Constraint::Percentage(50),
                Constraint::Percentage(10),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ];

            let table = Table::new(rows, widths)
                .header(Row::new(vec![
                    Line::from("Name"),
                    Line::from("Lag"),
                    Line::from("Exec Time"),
                    Line::from("Updated"),
                ]).style(Style::default().add_modifier(Modifier::BOLD)))
                .block(Block::default().borders(Borders::ALL));

            f.render_widget(table, chunks[1]);
        })?;

        // Input handling with small timeout to keep UI responsive
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    use ratatui::crossterm::event::KeyCode;
                    if filtering {
                        match key.code {
                            KeyCode::Esc => {
                                filtering = false;
                            }
                            KeyCode::Enter => {
                                filtering = false;
                            }
                            KeyCode::Char('/') => {
                                // Toggle filtering off with '/'
                                filtering = false;
                            }
                            KeyCode::Char('q') => {
                                // Do not quit app while typing; just exit filtering mode
                                filtering = false;
                            }
                            KeyCode::Backspace => {
                                app.filter.pop();
                                app.selected = 0;
                            }
                            KeyCode::Char(c) => {
                                // Accept most visible characters for the filter
                                if !c.is_control() {
                                    app.filter.push(c);
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
                                // keep existing filter so user can refine; to start fresh uncomment next line
                                // app.filter.clear();
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
