use crate::interface::tabs::tab_main_view::executor_item::ExecutorItem;
use crate::interface::tabs::tab_main_view::filters::FilterType;
use crate::interface::tabs::{Tab, TabEvent};
use crate::interface::{maths, AppState, ExecutorKey, RowPos};
use ratatui::crossterm::event;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Line, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Sparkline, Table};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tenant_item::TenantItem;

mod executor_item;
mod filters;
mod tenant_item;
mod watch_item;

pub enum MainViewEvent {
    SwitchToFilterHostMode,
    SwitchToFilterTenantMode,
    SwitchToDisplayMode,
}

enum Mode {
    View,
    Filter,
}

pub struct TabMainView {
    tenants: BTreeMap<String, TenantItem>,
    // index of the currently selected visible row in flat view
    selected_row: usize,
    // vertical scroll offset for the main table
    pub scroll_offset: usize,
    mode: Mode,
    filterer: TabMainViewFilter,
    readonly_app_state: &'static AppState,
}

impl TabMainView {
    /// Reset the visible row cursor
    fn reset_selected_row(&mut self) {
        self.selected_row = 0;
    }

    fn increase_selected_row(&mut self) {
        let max = self.visible_rows().len();
        if self.selected_row < max {
            self.selected_row += 1;
        }
    }

    fn decrease_selected_row(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        }
    }

    // Resolve the current selection to a watcher identifier if selection points to a watcher row
    pub fn selected_watch_ids(&self) -> Option<(String, (i64, String), i64)> {
        match self.readonly_app_state.visible_row_at(self.selected_row) {
            Some(RowPos::Watcher {
                tenant_name,
                exec_id,
                watch_id,
            }) => Some((tenant_name, exec_id, watch_id)),
            _ => None,
        }
    }

    // If selection is on a tenant row, return its name; otherwise None
    pub fn selected_tenant_name(&self) -> Option<String> {
        match self.readonly_app_state.visible_row_at(self.selected_row) {
            Some(RowPos::Tenant { tenant_name }) => Some(tenant_name),
            _ => None,
        }
    }

    /// Build visible rows respecting folding
    pub fn visible_rows(&self) -> Vec<Row<'static>> {
        // accumulate visible rows
        let mut rows = Vec::new();

        for (_tenant_name, tenant) in &self.tenants {
            // Filter out tenants
            if !self
                .filterer
                .filters
                .is_row_visible(tenant, FilterType::Tenant)
            {
                continue;
            }
            // Compute which executors are visible under the host filter
            let exec_iter = tenant.executors.iter().filter(|(_id, exec)| {
                self.filterer
                    .filters
                    .is_row_visible(*exec, FilterType::Host)
            });
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

            for (_exec_id, exec) in visible_execs {
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

    fn handle_filter_event(&mut self, event: TabEvent) -> TabEvent {
        match &event {
            TabEvent::Main(MainViewEvent::SwitchToFilterHostMode) => {
                self.mode = Mode::Filter;
                self.filterer.filters.set_filter(FilterType::Host);
            }
            TabEvent::Main(MainViewEvent::SwitchToFilterTenantMode) => {
                self.mode = Mode::Filter;
                self.filterer.filters.set_filter(FilterType::Tenant);
            }
            TabEvent::Main(MainViewEvent::SwitchToDisplayMode) => {
                self.mode = Mode::View;
            }
            _ => {}
        }
        event
    }
}

impl Tab for TabMainView {
    fn get_title(&self) -> String {
        let header_title = format!(
            "Watchers by Tenant/Executor (q quit, / tenant filter, h host filter, Esc/Enter exit filter, arrows navigate, Enter/Right fold/unfold) | {}",
            self.filterer.filters.to_string()
        );
        header_title
    }
    fn render(&mut self, frame: &mut ratatui::Frame, chunk: Rect) {
        let selected_watch = self.selected_watch_ids();
        let selected_tenant = self.selected_tenant_name();

        // Build all rows
        let rows = self.visible_rows();

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
            self.render_watcher_details(
                frame,
                chunk,
                tenant_name,
                exec_key,
                watch_id,
                &widths,
                rows,
                &selected_style,
            );
        } else if let Some(ref tenant_name) = selected_tenant {
            self.render_tenant_details(frame, chunk, &widths, rows, &selected_style)
        } else {
            self.render_table_without_details(frame, chunk, &widths, rows, &selected_style)
        }
    }

    fn update(&mut self, key: KeyEvent) -> TabEvent {
        if let Mode::Filter = self.mode {
            let event = self.filterer.update(key);
            return self.handle_filter_event(event);
        }

        match key.code {
            event::KeyCode::Tab => TabEvent::Cycle,
            event::KeyCode::Char('q') => TabEvent::Quit,
            event::KeyCode::Char('/') => TabEvent::Main(MainViewEvent::SwitchToFilterTenantMode),
            event::KeyCode::Char('h') => TabEvent::Main(MainViewEvent::SwitchToFilterHostMode),
            event::KeyCode::Up => {
                self.decrease_selected_row();
                TabEvent::None
            }
            event::KeyCode::Down => {
                self.increase_selected_row();
                TabEvent::None
            }
            _ => TabEvent::None,
        }
    }
}

impl TabMainView {
    fn clamp_scroll_offset(&mut self, page_size: usize) {
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        } else if self.selected_row >= self.scroll_offset + page_size {
            self.scroll_offset = self.selected_row + 1 - page_size;
        }
    }

    fn render_watcher_details(
        &self,
        frame: &mut ratatui::Frame,
        chunk: Rect,
        tenant_name: &str,
        exec_key: ExecutorKey,
        watch_id: i64,
        widths: &[Constraint; 5],
        rows_full: Vec<Row<'static>>,
        selected_style: &Style,
    ) {
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(chunk);

        // render paginated table in top part
        {
            let table_area = main_chunks[0];
            // approximate inner height: 2 for borders, 1 for header row
            let page_size = table_area.height.saturating_sub(3) as usize;

            let rows_slice: Vec<Row<'static>> = rows_full
                .iter()
                .cloned()
                .enumerate()
                .skip(self.scroll_offset)
                .take(page_size)
                .map(|(i, row)| {
                    if i == self.selected_row {
                        row.clone().style(*selected_style)
                    } else {
                        row
                    }
                })
                .collect();
            let table = Table::new(rows_slice, widths)
                .header(
                    Row::new(vec![
                        Line::from("Name"),
                        Line::from("Lag"),
                        Line::from("Exec Time"),
                        Line::from("Updated"),
                        Line::from("Host"),
                    ])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                )
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(table, table_area);
        }

        if let Some(watch) =
            self.readonly_app_state
                .get_watcher(tenant_name, exec_key.0, &exec_key.1, watch_id)
        {
            let detail_title = format!(
                "Details: {tenant_name} / Exec #{} @ {} / Watch #{} | Interest: {}",
                exec_key.0,
                exec_key.1,
                watch_id,
                if watch.interest.is_empty() {
                    "<unknown>"
                } else {
                    &watch.interest
                }
            );
            let detail_block = Block::default().title(detail_title).borders(Borders::ALL);

            // Split details area into two charts: Lag and Exec Time
            let detail_inner = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(main_chunks[1]);

            // Shared X range (unix seconds)
            let x_min_ts = watch.time_hist.front().copied().unwrap_or_else(|| {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();
                (now - 10.0).max(0.0)
            });
            let x_max_ts = watch.time_hist.back().copied().unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            });
            let x_upper = if x_max_ts > x_min_ts {
                x_max_ts
            } else {
                x_min_ts + 1.0
            };
            // Render details header above the lag chart
            let lag_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(detail_inner[0]);
            frame.render_widget(detail_block.clone(), lag_chunks[0]);

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
                .constraints(
                    [
                        Constraint::Length(1),
                        Constraint::Min(0),
                        Constraint::Length(1),
                    ]
                    .as_ref(),
                )
                .split(lag_layout[0]);
            let top_lbl = format!("{:.0}", y_max_lag.max(1.0));
            let mid_lbl = format!("{:.0}", y_max_lag.max(1.0) / 2.0);
            let bot_lbl = String::from("0");
            frame.render_widget(Paragraph::new(Line::from(top_lbl)), y_left[0]);

            // Center the middle label vertically within its area
            let mid_center = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(0),
                        Constraint::Length(1),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(y_left[1]);
            frame.render_widget(Paragraph::new(Line::from(mid_lbl)), mid_center[1]);
            frame.render_widget(Paragraph::new(Line::from(bot_lbl)), y_left[2]);
            // Plot area with sparkline
            let lag_block = Block::default()
                .title("Lag message over time")
                .borders(Borders::ALL);
            let lag_inner = lag_block.inner(lag_layout[1]);
            frame.render_widget(lag_block, lag_layout[1]);
            let lag_vals_src: Vec<u64> = watch.lag_hist.iter().copied().collect();
            let lag_max = lag_vals_src.iter().copied().max().unwrap_or(1);
            let lag_vals = maths::resample_to_width_u64(&lag_vals_src, lag_inner.width as usize);
            let lag_spark = Sparkline::default()
                .data(&lag_vals)
                .max(lag_max)
                .style(Style::default().fg(Color::Red));
            frame.render_widget(lag_spark, lag_inner);

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
                .constraints(
                    [
                        Constraint::Length(1),
                        Constraint::Min(0),
                        Constraint::Length(1),
                    ]
                    .as_ref(),
                )
                .split(exec_row[0]);
            let top_e = format!("{:.0}", y_max_exec.max(1.0));
            let mid_e = format!("{:.0}", y_max_exec.max(1.0) / 2.0);
            let bot_e = String::from("0");
            frame.render_widget(Paragraph::new(Line::from(top_e)), y_left_e[0]);

            // Center the middle label vertically within its area
            let mid_center_e = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Min(0),
                        Constraint::Length(1),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(y_left_e[1]);
            frame.render_widget(Paragraph::new(Line::from(mid_e)), mid_center_e[1]);
            frame.render_widget(Paragraph::new(Line::from(bot_e)), y_left_e[2]);

            // Sparkline plot with block
            let exec_block = Block::default()
                .title("Exec Time (ms)")
                .borders(Borders::ALL);
            let exec_inner = exec_block.inner(exec_row[1]);
            frame.render_widget(exec_block, exec_row[1]);
            let exec_vals_src: Vec<u64> = watch
                .exec_hist
                .iter()
                .map(|v| v.max(0.0).round() as u64)
                .collect();
            let exec_max = exec_vals_src.iter().copied().max().unwrap_or(1);
            let exec_vals = maths::resample_to_width_u64(&exec_vals_src, exec_inner.width as usize);
            let exec_spark = Sparkline::default()
                .data(&exec_vals)
                .max(exec_max)
                .style(Style::default().fg(Color::Green));
            frame.render_widget(exec_spark, exec_inner);

            // X-axis human-readable time labels under the plot, centered in thirds
            use chrono::{Local, TimeZone};
            let fmt_time = |ts: f64| -> String {
                let secs = ts.floor() as i64;
                let dt = match Local.timestamp_opt(secs, 0) {
                    chrono::LocalResult::Single(dt) => dt,
                    _ => Local.timestamp(0, 0),
                };
                dt.format("%H:%M:%S").to_string()
            };
            let mid_ts = (x_min_ts + x_upper) / 2.0;
            let labels_row = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(33),
                        Constraint::Percentage(34),
                        Constraint::Percentage(33),
                    ]
                    .as_ref(),
                )
                .split(exec_v[1]);
            let left_lbl =
                Paragraph::new(Line::from(fmt_time(x_min_ts))).alignment(Alignment::Center);
            let mid_lbl = Paragraph::new(Line::from(fmt_time(mid_ts))).alignment(Alignment::Center);
            let right_lbl =
                Paragraph::new(Line::from(fmt_time(x_upper))).alignment(Alignment::Center);
            frame.render_widget(left_lbl, labels_row[0]);
            frame.render_widget(mid_lbl, labels_row[1]);
            frame.render_widget(right_lbl, labels_row[2]);
        }
    }

    fn render_tenant_details(
        &self,
        frame: &mut ratatui::Frame,
        chunk: Rect,
        widths: &[Constraint; 5],
        rows: Vec<Row<'static>>,
        selected_style: &Style,
    ) {
        // Tenant selected: show details summary below table
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(chunk);

        // render paginated table in top part
        let table_area = main_chunks[0];
        let page_size = table_area.height.saturating_sub(3) as usize;
        if page_size == 0 {
            let empty: Vec<Row<'static>> = Vec::new();
            let table = Table::new(empty, widths)
                .header(
                    Row::new(vec![
                        Line::from("Name"),
                        Line::from("Lag"),
                        Line::from("Exec Time"),
                        Line::from("Updated"),
                        Line::from("Host"),
                    ])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                )
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(table, table_area);
        } else {
            let rows_slice: Vec<Row<'static>> = rows
                .iter()
                .cloned()
                .enumerate()
                .skip(self.scroll_offset)
                .take(page_size)
                .map(|(i, row)| {
                    if i == self.selected_row {
                        row.clone().style(*selected_style)
                    } else {
                        row
                    }
                })
                .collect();
            let table = Table::new(rows_slice, widths)
                .header(
                    Row::new(vec![
                        Line::from("Name"),
                        Line::from("Lag"),
                        Line::from("Exec Time"),
                        Line::from("Updated"),
                        Line::from("Host"),
                    ])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                )
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(table, table_area);
        }
    }

    fn render_table_without_details(
        &self,
        frame: &mut ratatui::Frame,
        chunk: Rect,
        widths: &[Constraint; 5],
        rows: Vec<Row<'static>>,
        selected_style: &Style,
    ) {
        // No watcher or tenant selected, render full-height table (paginated)
        let table_area = chunk;
        let page_size = table_area.height.saturating_sub(3) as usize;
        if page_size == 0 {
            let empty: Vec<Row<'static>> = Vec::new();
            let table = Table::new(empty, widths)
                .header(
                    Row::new(vec![
                        Line::from("Name"),
                        Line::from("Lag"),
                        Line::from("Exec Time"),
                        Line::from("Updated"),
                        Line::from("Host"),
                    ])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                )
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(table, table_area);
        } else {
            let rows_slice: Vec<Row<'static>> = rows
                .iter()
                .cloned()
                .enumerate()
                .skip(self.scroll_offset)
                .take(page_size)
                .map(|(i, row)| {
                    if i == self.selected_row {
                        row.clone().style(*selected_style)
                    } else {
                        row
                    }
                })
                .collect();
            let table = Table::new(rows_slice, widths)
                .header(
                    Row::new(vec![
                        Line::from("Name"),
                        Line::from("Lag"),
                        Line::from("Exec Time"),
                        Line::from("Updated"),
                        Line::from("Host"),
                    ])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                )
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(table, table_area);
        }
    }
}

struct TabMainViewFilter {
    filters: filters::Filters,
}

impl TabMainViewFilter {
    fn update(&mut self, key: KeyEvent) -> TabEvent {
        match key.code {
            event::KeyCode::Enter => {
                self.filters.disable_filter();
                TabEvent::None
            }
            event::KeyCode::Backspace => {
                self.filters.pop_char();
                TabEvent::None
            }
            event::KeyCode::Char(c) => {
                self.filters.push_char(c);
                TabEvent::None
            }
            _ => TabEvent::None,
        }
    }
}
