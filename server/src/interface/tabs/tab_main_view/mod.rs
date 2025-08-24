use crate::interface::tabs::tab_main_view::executor_item::ExecutorItem;
use crate::interface::tabs::tab_main_view::filters::FilterType;
use crate::interface::tabs::{Tab, TabEvent};
use crate::interface::ExecutorKey;
use ratatui::crossterm::event;
use ratatui::crossterm::event::KeyEvent;
use ratatui::widgets::Row;
use std::collections::BTreeMap;
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
    selected_row: usize,
    mode: Mode,
    filterer: TabMainViewFilter,
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
    fn render(&self) {
        todo!()
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
