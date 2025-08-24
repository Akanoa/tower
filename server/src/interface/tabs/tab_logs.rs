use crate::interface::tabs::{Tab, TabEvent};
use ratatui::crossterm::event;
use ratatui::crossterm::event::KeyEvent;

pub struct TabLogs {
    logs: Vec<String>,
    offset: usize,
}

impl Tab for TabLogs {
    fn render(&self) {
        todo!()
    }

    fn update(&mut self, key: KeyEvent) -> TabEvent {
        match key.code {
            event::KeyCode::Tab => TabEvent::Cycle,
            event::KeyCode::Char('q') => TabEvent::Quit,
            event::KeyCode::Up => {
                self.offset = self.offset.saturating_sub(1);
                TabEvent::None
            }
            event::KeyCode::Down => {
                if self.offset < self.logs.len() {
                    self.offset += 1;
                }
                TabEvent::None
            }
            _ => TabEvent::None,
        }
    }
}
