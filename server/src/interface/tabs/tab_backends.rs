use crate::interface::tabs::{Tab, TabEvent};
use crate::tcp_server::PollControl;
use ratatui::crossterm::event;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::{Line, Modifier, Style};
use ratatui::widgets::{Block, Borders, Row, Table};
use tokio::sync::mpsc::UnboundedSender;

pub enum BackendEvent {
    AddBackend(BackendIdentifier),
    SwitchToAddMode,
    SwitchToViewMode,
}

pub type BackendIdentifier = (String, u16);

#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    View,
    Add,
}

pub struct TabBackends {
    mode: Mode,
    backends: Vec<BackendIdentifier>,
    polling_paused: bool,
    selected_index: usize,
    poll_control: UnboundedSender<PollControl>,
    adder: TabBackendsAdd,
}

impl TabBackends {
    fn handle_tab_event(&mut self, event: TabEvent) -> TabEvent {
        match &event {
            TabEvent::Backend(BackendEvent::AddBackend(identifier)) => {
                let _ = self
                    .poll_control
                    .send(PollControl::AddBackend(identifier.clone()));
                self.backends.push(identifier.clone());
            }
            TabEvent::Backend(BackendEvent::SwitchToAddMode) => {
                self.mode = Mode::Add;
            }
            TabEvent::Backend(BackendEvent::SwitchToViewMode) => {
                self.mode = Mode::View;
            }
            _ => {}
        };
        event
    }
}

impl Tab for TabBackends {
    fn get_title(&self) -> String {
        // Backends management tab
        let title = if self.mode == Mode::Add {
            format!(
                "Backends Management [Tab switch | p {} | ENTER submit | ESC cancel] Add backend: {}",
                if self.polling_paused {
                    "resume"
                } else {
                    "pause"
                },
                self.adder.add_new_backend_buffer
            )
        } else {
            format!(
                "Backends Management [Tab switch | p {} | a add | d delete]",
                if self.polling_paused {
                    "resume"
                } else {
                    "pause"
                }
            )
        };
        title
    }
    fn render(&mut self, frame: &mut ratatui::Frame, chunk: Rect) {
        // List backends and current state
        let mut rows: Vec<Row<'static>> = Vec::new();
        rows.push(
            Row::new(vec![
                Line::from("Backend"),
                Line::from("Port"),
                Line::from("State"),
            ])
            .style(Style::default().add_modifier(Modifier::BOLD)),
        );

        let sel_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD);
        for (i, (addr, port)) in self.backends.iter().enumerate() {
            let mut row = Row::new(vec![
                Line::from(addr.clone()),
                Line::from(port.to_string()),
                Line::from(
                    if self.polling_paused {
                        "paused"
                    } else {
                        "running"
                    }
                    .to_string(),
                ),
            ]);
            if i == self.selected_index {
                row = row.style(sel_style);
            }
            rows.push(row);
        }
        let table = Table::new(
            rows,
            [
                Constraint::Percentage(60),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ],
        )
        .block(
            Block::default()
                .title("Configured backends")
                .borders(Borders::ALL),
        );
        frame.render_widget(table, chunk);
    }

    fn update(&mut self, key: KeyEvent) -> TabEvent {
        if let Mode::Add = self.mode {
            let event = self.adder.update(key);
            return self.handle_tab_event(event);
        }

        match key.code {
            event::KeyCode::Tab => TabEvent::Cycle,
            event::KeyCode::Char('q') => TabEvent::Quit,
            event::KeyCode::Char('p') => {
                let _ = self.poll_control.send(PollControl::Pause);
                TabEvent::None
            }
            event::KeyCode::Char('a') => TabEvent::Backend(BackendEvent::SwitchToAddMode),
            event::KeyCode::Char('d') => {
                if self.backends.is_empty() {
                    // do nothing because there is no backend to delete
                    return TabEvent::None;
                }

                if self.selected_index > self.backends.len() - 1 {
                    // invalid state resetting selecting backend
                    self.selected_index = 0;
                    return TabEvent::None;
                }

                let Some(identifier) = self.backends.get(self.selected_index).cloned() else {
                    // Backend can't be found at index
                    return TabEvent::None;
                };

                let _ = self
                    .poll_control
                    .send(PollControl::RemoveBackend(identifier));
                self.backends.remove(self.selected_index);

                // increase the selected index by one if the last backend has been removed
                if self.selected_index > self.backends.len() {
                    self.selected_index -= 1;
                }

                TabEvent::None
            }
            event::KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                TabEvent::None
            }
            event::KeyCode::Down => {
                if self.selected_index < self.backends.len() - 1 {
                    self.selected_index += 1;
                }
                TabEvent::None
            }

            _ => TabEvent::None,
        }
    }
}

struct TabBackendsAdd {
    add_new_backend_buffer: String,
}

impl TabBackendsAdd {
    fn update(&mut self, key: KeyEvent) -> TabEvent {
        match key.code {
            event::KeyCode::Esc => TabEvent::Backend(BackendEvent::SwitchToViewMode),
            event::KeyCode::Enter => {
                let Some((address, port)) = self.add_new_backend_buffer.split_once(":") else {
                    return TabEvent::None;
                };

                let Ok(port) = port.parse::<u16>() else {
                    return TabEvent::None;
                };

                let event =
                    TabEvent::Backend(BackendEvent::AddBackend((address.to_string(), port)));
                self.add_new_backend_buffer.clear();
                event
            }
            _ => TabEvent::None,
        }
    }
}
