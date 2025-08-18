pub struct Watcher {
    watch_id: i64,
    interest: String,
}

impl Watcher {
    pub(crate) fn new(watch_id: i64, interest: &str) -> Self {
        Watcher {
            watch_id,
            interest: interest.into(),
        }
    }
}