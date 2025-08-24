use crate::interface::filters::FilterType;
use crate::interface::AppState;
use crate::tcp_server::PollControl;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{Event, KeyEvent};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

#[derive(PartialEq)]
pub enum Tab {
    Main,
    Backends,
    Logs,
}

pub enum TuiRender {
    Local,
    Aggregator,
}

pub struct TabState {
    pub tab: Tab,
    pub render: TuiRender,
    poll_ctx: Option<UnboundedSender<PollControl>>,
}

impl TabState {
    pub(crate) fn new(
        poll_ctx: Option<UnboundedSender<PollControl>>,
        tui_render: TuiRender,
    ) -> Self {
        Self {
            tab: Tab::Main,
            render: tui_render,
            poll_ctx,
        }
    }
}

impl TabState {
    pub fn cycle(&mut self) {
        self.tab = match self.tab {
            Tab::Main => match self.render {
                TuiRender::Local => Tab::Logs,
                TuiRender::Aggregator => Tab::Backends,
            },
            Tab::Backends => Tab::Logs,
            Tab::Logs => Tab::Main,
        }
    }

    pub fn is_actual_tab(&self, tab: Tab) -> bool {
        self.tab == tab
    }

    /// Handles key events for the application, delegating specific actions
    /// based on the current `Tab` and `AppState`.
    ///
    /// This function listens for key events using a timeout, determines if the key
    /// event is relevant, and processes the event accordingly. If no key event
    /// is detected within the timeout period, it returns indicating no activity.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of the
    ///   application handler, which manages state and input handling.
    /// * `app_state: &mut AppState` - A mutable reference to the application's global
    ///   state, which might be altered based on the received key event.
    ///
    /// # Returns
    ///
    /// Returns an ` std::io::Result < bool >`:
    /// * `Ok(false)` - Indicates that no event was handled or no action resulted in quitting.
    /// * `Ok(true)` - Indicates an action was handled and requires the application to quit.
    /// * `Err(std::io::Error)` - Propagates any I/O errors encountered during polling
    ///   or reading key events.
    ///
    /// # Behavior
    ///
    /// - If no key event is polled within the 100 ms timeout duration, the function returns `Ok(false)`.
    /// - If a non-key event is received, the function ignores it and returns `Ok(false)`.
    /// - Depending on the current `Tab` the application is on, the key event is delegated
    ///   to specific handler functions:
    ///     - `handle_key_event_main_view`: Processes events in the `Main` tab.
    ///     - `handle_key_event_backends_view`: Processes events in the `Backends` tab
    ///       (only applicable for `TuiRender::Aggregator` mode).
    ///     - `handle_key_event_logs_view`: Processes events in the `Logs` tab.
    /// - If the `Tab::Backends` tab is selected but the `TuiRender` mode is not
    ///   `Aggregator`, no action is taken for the key event.
    ///
    /// # Errors
    ///
    /// The function may return an `Err` in case of I/O-related issues:
    /// * During the polling of an event (`event::poll`).
    /// * While reading the event (`event::read`).
    pub fn handle_key_event(&mut self, app_state: &mut AppState) -> std::io::Result<bool> {
        // If no key event available return
        if !event::poll(Duration::from_millis(100))? {
            return Ok(false);
        }

        // Get the first available event
        let event = event::read()?;

        // We only handle key events
        let Event::Key(key) = event else {
            return Ok(false);
        };

        let should_quit = match self.tab {
            Tab::Main => self.handle_key_event_main_view(key, app_state),
            Tab::Backends if matches!(self.render, TuiRender::Aggregator) => {
                self.handle_key_event_backends_view(key, app_state)
            }
            Tab::Backends => {
                // backends only exist for aggregator mode
                false
            }
            Tab::Logs => self.handle_key_event_logs_view(key, app_state),
        };
        Ok(should_quit)
    }

    fn handle_key_event_main_view(&mut self, key: KeyEvent, app_state: &mut AppState) -> bool {
        if app_state.filters.is_active() {
            self.handle_key_event_main_view_filtering(key, app_state);
            return false;
        }

        match key.code {
            // cycle between tabs
            event::KeyCode::Tab => {
                self.cycle();
                false
            }
            event::KeyCode::Char('q') => true,
            event::KeyCode::Char('/') => {
                app_state.filters.set_filter(FilterType::Tenant);
                app_state.reset_selected_row();
                false
            }
            event::KeyCode::Char('h') => {
                app_state.filters.set_filter(FilterType::Host);
                app_state.reset_selected_row();
                false
            }
            event::KeyCode::Up => {
                app_state.decrease_selected_row();
                false
            }
            event::KeyCode::Down => {
                app_state.increase_selected_row();
                false
            }
            _ => false,
        }
    }

    fn handle_key_event_main_view_filtering(&self, key: KeyEvent, app_state: &mut AppState) {
        match key.code {
            event::KeyCode::Enter => app_state.filters.disable_filter(),
            event::KeyCode::Backspace => {
                app_state.filters.pop_char();
                app_state.reset_selected_row();
            }
            event::KeyCode::Char(c) => {
                if !c.is_control() {
                    app_state.filters.push_char(c);
                    app_state.reset_selected_row();
                }
            }
            _ => {}
        }
    }

    fn handle_key_event_backends_view(&mut self, key: KeyEvent, app_state: &mut AppState) -> bool {
        if app_state.rendering.adding_backend {
            self.handle_key_event_backends_view_adding_backend(key, app_state);
        }

        match key.code {
            event::KeyCode::Tab => {
                self.cycle();
                false
            }
            event::KeyCode::Char('q') => true,
            event::KeyCode::Char('p') => {
                if let Some(ctx) = &self.poll_ctx {
                    let _ = if app_state.rendering.polling_paused {
                        ctx.send(PollControl::Pause)
                    } else {
                        ctx.send(PollControl::Resume)
                    };
                }
                false
            }
            event::KeyCode::Char('a') => {
                app_state.rendering.adding_backend = true;
                app_state.rendering.add_buffer.clear();
                false
            }
            event::KeyCode::Char('d') => {
                if app_state.backends.is_empty() {
                    return false;
                }

                let (addr, port) = app_state.backends[app_state.rendering.backends_sel].clone();
                if let Some(ref txc) = self.poll_ctx {
                    let _ = txc.send(PollControl::RemoveBackend((addr.clone(), port)));
                }
                app_state.backends.remove(app_state.rendering.backends_sel);

                if app_state.rendering.backends_sel == 0 {
                    return false;
                }

                if app_state.rendering.backends_sel >= app_state.backends.len() {
                    app_state.rendering.backends_sel -= 1;
                }
                false
            }
            event::KeyCode::Down => {
                if app_state.rendering.backends_sel < app_state.backends.len() {
                    app_state.rendering.backends_sel += 1;
                }
                false
            }
            event::KeyCode::Up => {
                if app_state.rendering.backends_sel > 0 {
                    app_state.rendering.backends_sel -= 1;
                }
                false
            }
            _ => false,
        }
    }

    fn handle_key_event_backends_view_adding_backend(
        &self,
        key: KeyEvent,
        app_state: &mut AppState,
    ) {
        match key.code {
            event::KeyCode::Esc => {
                app_state.rendering.adding_backend = false;
                app_state.rendering.add_buffer.clear();
            }
            event::KeyCode::Char(c) => {
                app_state.rendering.add_buffer.push(c);
            }
            event::KeyCode::Backspace => {
                app_state.rendering.add_buffer.pop();
            }
            event::KeyCode::Enter => {
                // The backend must have a port and a host "host:port"
                let Some((addr_s, port_s)) = app_state.rendering.add_buffer.split_once(':') else {
                    app_state.rendering.add_buffer.clear();
                    app_state.rendering.adding_backend = false;
                    return;
                };

                // Parse port as number
                let Ok(port) = port_s.parse::<u16>() else {
                    return;
                };

                // Check if backend already in the list
                if app_state
                    .backends
                    .iter()
                    .any(|(a, p)| a == &addr_s && *p == port)
                {
                    return;
                }

                app_state.backends.push((addr_s.to_string(), port));

                if let Some(ref txc) = self.poll_ctx {
                    let _ = txc.send(PollControl::AddBackend((addr_s.to_string(), port)));
                }

                // keep the cursor at the same row than before adding
                app_state.rendering.backends_sel = app_state.backends.len().saturating_sub(1);

                app_state.rendering.add_buffer.clear();
                app_state.rendering.adding_backend = false;
            }
            _ => {}
        }
    }

    fn handle_key_event_logs_view(&mut self, key: KeyEvent, _app_state: &mut AppState) -> bool {
        match key.code {
            event::KeyCode::Char('q') => true,
            event::KeyCode::Tab => {
                self.cycle();
                false
            }
            _ => false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Rendering {
    pub polling_paused: bool,
    pub backends_sel: usize,
    pub adding_backend: bool,
    pub add_buffer: String,
    pub logs: VecDeque<String>,
    pub logs_offset: usize,
}
