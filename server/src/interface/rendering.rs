use std::collections::VecDeque;

pub enum Tab {
    Main,
    Backends,
    Logs,
}

pub struct TabState {
    pub tab: Tab,
}

impl Default for TabState {
    fn default() -> Self {
        Self { tab: Tab::Main }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Rendering {
    pub in_backends_tab: bool,
    pub in_logs_tab: bool,
    pub polling_paused: bool,
    pub backends_sel: usize,
    pub adding_backend: bool,
    pub add_buffer: String,
    pub logs: VecDeque<String>,
    pub logs_offset: usize,
}
