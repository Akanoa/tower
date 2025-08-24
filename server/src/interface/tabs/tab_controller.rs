use crate::interface::tabs::tab_backends::TabBackends;
use crate::interface::tabs::tab_logs::TabLogs;
use crate::interface::tabs::tab_main_view::TabMainView;
use crate::interface::tabs::{Tab, TabEvent};
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders};
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
    fn map_current_tab<F, R>(&mut self, mut f: F) -> Option<R>
    where
        F: FnMut(&mut dyn Tab) -> R,
    {
        match self.current_tab {
            TabKind::Main => Some(f(&mut self.tab_main_view as &mut dyn Tab)),
            TabKind::Backends => {
                if let Some(backends_tab) = &mut self.backends_tab {
                    Some(f(backends_tab as &mut dyn Tab))
                } else {
                    None
                }
            }
            TabKind::Logs => Some(f(&mut self.logs_tab as &mut dyn Tab)),
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(size);

        let title = self.map_current_tab(|tab| tab.get_title());

        if let Some(title) = title {
            let header = Block::default().title(title).borders(Borders::ALL);
            frame.render_widget(header, chunks[0]);
        }

        self.map_current_tab(|tab| tab.render(frame, chunks[1]));
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
