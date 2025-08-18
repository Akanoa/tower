use crate::watcher::Watcher;
use std::collections::BTreeMap;

pub struct Executor {
    executor_id: i64,
    pub(crate) watchers: BTreeMap<i64, Watcher>,
    pub tenant: String,
}

impl Executor {
    pub(crate) fn new(tenant: &str, executor_id: i64) -> Self {
        Executor {
            executor_id,
            watchers: BTreeMap::new(),
            tenant: tenant.to_string(),
        }
    }
}
