use crate::interface::tabs::tab_backends::TabBackends;
use crate::interface::tabs::tab_logs::TabLogs;
use crate::interface::tabs::tab_main_view::TabMainView;
use crate::interface::tabs::{Tab, TabEvent};
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use std::time::Duration;

#[derive(PartialEq)]
pub enum TabKind {
    Main,
    Backends,
    Logs,
}

pub struct TabController {
    pub tab_main_view: TabMainView,
    pub backends_tab: Option<TabBackends>,
    pub logs_tab: TabLogs,
    pub current_tab: TabKind,
}

impl TabController {
    fn cycle(&mut self) {
        self.current_tab = match self.current_tab {
            TabKind::Main => match self.backends_tab {
                None => TabKind::Logs,
                Some(_) => TabKind::Backends,
            },
            TabKind::Backends => TabKind::Logs,
            TabKind::Logs => TabKind::Main,
        }
    }

    fn handle_tab_event(&mut self, event: TabEvent) -> TabEvent {
        match &event {
            TabEvent::Cycle => {
                self.cycle();
            }
            _ => {}
        };
        event
    }
}

impl TabController {
    fn render(&self) {
        match self.current_tab {
            TabKind::Main => self.tab_main_view.render(),
            TabKind::Backends => {
                if let Some(backends_tab) = &self.backends_tab {
                    backends_tab.render()
                } else {
                }
            }
            TabKind::Logs => self.logs_tab.render(),
        }
    }

    fn update(&mut self) -> std::io::Result<TabEvent> {
        if !event::poll(Duration::from_millis(100))? {
            return Ok(TabEvent::None);
        }

        // Get the first available event
        let event = event::read()?;

        // We only handle key events
        let Event::Key(key) = event else {
            return Ok(TabEvent::None);
        };

        let event = match self.current_tab {
            TabKind::Main => self.tab_main_view.update(key),
            TabKind::Backends => {
                if let Some(backends_tab) = &mut self.backends_tab {
                    backends_tab.update(key)
                } else {
                    TabEvent::None
                }
            }
            TabKind::Logs => self.logs_tab.update(key),
        };

        Ok(self.handle_tab_event(event))
    }
}
