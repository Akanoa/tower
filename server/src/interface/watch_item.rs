use ratatui::prelude::{Color, Line, Style};
use ratatui::widgets::{Cell, Row};
use std::collections::VecDeque;
use std::time::Instant;

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

impl WatchItem {
    pub fn as_row(&self) -> Row<'static> {
        Row::new(vec![
            Cell::from(Line::from(format!("      Watch #{:}", self.watch_id))),
            {
                let style = if self.lag > LAG_THRESHOLD {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                Cell::from(Line::from(format!("{}", self.lag))).style(style)
            },
            {
                let style = if self.execution_time > EXECUTION_TIME_THRESHOLD {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                Cell::from(Line::from(format!("{:.3} ms", self.execution_time))).style(style)
            },
            Cell::from(Line::from(format!(
                "{:?}",
                Instant::now().saturating_duration_since(self.updated_at)
            ))),
            Cell::from(Line::from("")),
        ])
    }
}
