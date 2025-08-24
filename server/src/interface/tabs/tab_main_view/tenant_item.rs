use crate::interface::tabs::tab_main_view::executor_item::ExecutorItem;
use crate::interface::tabs::tab_main_view::filters::Rowable;
use ratatui::prelude::{Color, Line, Modifier, Style};
use ratatui::widgets::{Cell, Row};
use std::collections::BTreeMap;

/// A constant representing the threshold for lag detection.
///
/// `LAG_THRESHOLD` is used to determine if a process or operation is lagging.
/// The value is defined as `10_000` (units depend on the context, such as milliseconds or microseconds),
/// and can be compared against measured durations to decide if the lag exceeds acceptable limits.
///
/// This threshold is typically used in systems where performance monitoring or
/// latency evaluation is critical.
const LAG_THRESHOLD: u64 = 10_000;
/// A constant that defines the threshold for execution time.
///
/// `EXECUTION_TIME_THRESHOLD` is used as a reference point to indicate the maximum
/// acceptable execution time (in milliseconds) for a specific operation or task.
/// If the execution time surpasses this threshold, it may trigger warnings,
/// logging, or other corrective actions.
///
/// Value: `1000.0` (equivalent to 1 second).
const EXECUTION_TIME_THRESHOLD: f64 = 1000.0;

#[derive(Debug, Default, Clone)]
pub struct TenantItem {
    pub tenant: String,
    pub folded: bool,
    pub executors: BTreeMap<(i64, String), ExecutorItem>,
}

pub struct TenantStatistics {
    pub mean_lag: u64,
    pub mean_execution_time: f64,
}

impl Rowable for TenantItem {
    fn get_filterable_data(&self) -> &str {
        self.tenant.as_str()
    }
}

impl TenantItem {
    /// Compute tenant-level means based on means of its visible executors (with watchers)
    fn get_statistics(&self, executors: &[&ExecutorItem]) -> TenantStatistics {
        let mut sum_mean_lag = 0.0_f64;
        let mut sum_mean_exec = 0.0_f64;
        let mut count = 0.0_f64;
        for exec in executors {
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
        let (mean_lag, mean_execution_time) = if count > 0.0 {
            ((sum_mean_lag / count).round() as u64, sum_mean_exec / count)
        } else {
            (0u64, 0.0f64)
        };

        TenantStatistics {
            mean_lag,
            mean_execution_time,
        }
    }

    /// Convert a [TenantItem] as a [Row]
    pub fn as_row(&self, executors: &[&ExecutorItem]) -> Row<'static> {
        let toggle_symbol = if self.folded { "▸" } else { "▾" };

        let stats = self.get_statistics(&executors);

        let TenantStatistics {
            mean_lag: tenant_mean_lag,
            mean_execution_time: tenant_mean_exec,
        } = stats;

        Row::new(vec![
            Cell::from(Line::from(format!(
                "{toggle_symbol} Tenant: {}",
                self.tenant
            ))),
            {
                let style = if tenant_mean_lag > LAG_THRESHOLD {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                Cell::from(Line::from(format!("{}", tenant_mean_lag))).style(style)
            },
            {
                let style = if tenant_mean_exec > EXECUTION_TIME_THRESHOLD {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                Cell::from(Line::from(format!("{:.3} ms", tenant_mean_exec))).style(style)
            },
            Cell::from(Line::from("")),
            Cell::from(Line::from("")),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD))
    }
}
