use crate::interface::filters::Rowable;
use crate::interface::ExecutorItem;
use std::collections::BTreeMap;

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

impl TenantItem {
    /// Compute tenant-level means based on means of its visible executors (with watchers)
    pub fn get_statistics(&self, executors: &[&ExecutorItem]) -> TenantStatistics {
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
}

impl Rowable for TenantItem {
    fn get_filterable_data(&self) -> &str {
        self.tenant.as_str()
    }
}
