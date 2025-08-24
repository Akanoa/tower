use crate::interface::tabs::{Tab, TabEvent};
use ratatui::crossterm::event;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct TabLogs {
    logs: Vec<String>,
    offset: usize,
}

impl Tab for TabLogs {
    fn get_title(&self) -> String {
        "Logs [Tab switch | Up/Down scroll | c clear | q quit]".to_string()
    }
    fn render(&mut self, frame: &mut ratatui::Frame, chunk: Rect) {
        // Determine visible window
        let area = chunk;
        let inner_height = area.height.saturating_sub(2) as usize; // rough estimate
        let total = self.logs.len();
        let start = if self.offset == 0 {
            total.saturating_sub(inner_height)
        } else {
            total.saturating_sub(inner_height + self.offset)
        };
        let mut display: String = String::new();
        for line in self.logs.iter().skip(start).take(inner_height) {
            display.push_str(line);
            display.push('\n');
        }
        let paragraph = Paragraph::new(display)
            .block(Block::default().title("Recent logs").borders(Borders::ALL));
        frame.render_widget(paragraph, chunk);
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
