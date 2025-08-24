use crate::interface::tabs::tab_main_view::filters::Rowable;
use crate::interface::watch_item::WatchItem;
use ratatui::prelude::{Color, Line, Style};
use ratatui::widgets::{Cell, Row};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct ExecutorItem {
    pub executor_id: i64,
    pub folded: bool,
    pub watchers: BTreeMap<i64, WatchItem>,
    pub host: String,
}

impl Rowable for ExecutorItem {
    fn get_filterable_data(&self) -> &str {
        self.host.as_str()
    }
}

struct ExecutorStatistics {
    mean_lag: u64,
    mean_execution_time: f64,
}

impl ExecutorItem {
    /// Compute means from current watcher values
    fn get_statistics(&self) -> ExecutorStatistics {
        let watcher_count = self.watchers.len() as f64;
        let (mean_lag, mean_execution_time) = if watcher_count > 0.0 {
            let sum_lag: u128 = self.watchers.values().map(|w| w.lag as u128).sum();
            let sum_exec: f64 = self.watchers.values().map(|w| w.execution_time).sum();
            (
                (sum_lag as f64 / watcher_count).round() as u64,
                sum_exec / watcher_count,
            )
        } else {
            (0u64, 0.0f64)
        };

        ExecutorStatistics {
            mean_lag,
            mean_execution_time,
        }
    }

    pub fn as_row(&self) -> Row<'static> {
        let ExecutorStatistics {
            mean_execution_time: mean_exec,
            mean_lag,
        } = self.get_statistics();

        let e_prefix = if self.folded { "  ▸" } else { "  ▾" };

        Row::new(vec![
            Cell::from(Line::from(format!(
                "{e_prefix} Executor #{}",
                self.executor_id
            ))),
            {
                let style = if mean_lag > 10_000 {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                Cell::from(Line::from(format!("{}", mean_lag))).style(style)
            },
            {
                let style = if mean_exec > 1000.0 {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                Cell::from(Line::from(format!("{:.3} ms", mean_exec))).style(style)
            },
            Cell::from(Line::from("")),
            Cell::from(Line::from(self.host.to_string())),
        ])
    }
}
