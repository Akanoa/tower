use crate::interface::filters::Rowable;
use crate::interface::WatchItem;
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
