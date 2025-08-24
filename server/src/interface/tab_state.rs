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

    pub fn handle_key_event(&mut self, app_state: &mut AppState) -> std::io::Result<()> {
        // If no key event available return
        if !event::poll(Duration::from_millis(100))? {
            return Ok(());
        }

        // Get the first available event
        let event = event::read()?;

        // We only handle key events
        let Event::Key(key) = event else {
            return Ok(());
        };

        match self.tab {
            Tab::Main => self.handle_key_event_main_view(key, app_state),
            Tab::Backends if matches!(self.render, TuiRender::Aggregator) => {
                self.handle_key_event_backends_view(key, app_state)
            }
            Tab::Backends => {
                // backends only exist for aggregator mode
            }
            Tab::Logs => self.handle_key_event_logs_view(key, app_state),
        };
        Ok(())
    }

    fn handle_key_event_main_view(&mut self, key: KeyEvent, app_state: &mut AppState) {
        if app_state.filters.is_active() {
            return self.handle_key_event_main_view_filtering(key, app_state);
        }

        match key.code {
            // cycle between tabs
            event::KeyCode::Tab => self.cycle(),
            event::KeyCode::Char('q') => std::process::exit(0),
            event::KeyCode::Char('/') => {
                app_state.filters.set_filter(FilterType::Tenant);
                app_state.reset_selected_row();
            }
            event::KeyCode::Char('h') => {
                app_state.filters.set_filter(FilterType::Host);
                app_state.reset_selected_row()
            }
            event::KeyCode::Up => app_state.decrease_selected_row(),
            event::KeyCode::Down => app_state.increase_selected_row(),

            _ => {}
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

    fn handle_key_event_backends_view(&mut self, key: KeyEvent, app_state: &mut AppState) {
        if app_state.rendering.adding_backend {
            self.handle_key_event_backends_view_adding_backend(key, app_state);
        }

        match key.code {
            event::KeyCode::Tab => self.cycle(),
            event::KeyCode::Char('q') => std::process::exit(0),
            event::KeyCode::Char('p') => {
                if let Some(ctx) = &self.poll_ctx {
                    let _ = if app_state.rendering.polling_paused {
                        ctx.send(PollControl::Pause)
                    } else {
                        ctx.send(PollControl::Resume)
                    };
                }
            }
            event::KeyCode::Char('a') => {
                app_state.rendering.adding_backend = true;
                app_state.rendering.add_buffer.clear();
            }
            event::KeyCode::Char('d') => {
                if app_state.backends.is_empty() {
                    return;
                }

                let (addr, port) = app_state.backends[app_state.rendering.backends_sel].clone();
                if let Some(ref txc) = self.poll_ctx {
                    let _ = txc.send(PollControl::RemoveBackend((addr.clone(), port)));
                }
                app_state.backends.remove(app_state.rendering.backends_sel);

                if app_state.rendering.backends_sel == 0 {
                    return;
                }

                if app_state.rendering.backends_sel >= app_state.backends.len() {
                    app_state.rendering.backends_sel -= 1;
                }
            }
            event::KeyCode::Down => {
                if app_state.rendering.backends_sel < app_state.backends.len() {
                    app_state.rendering.backends_sel += 1;
                }
            }
            event::KeyCode::Up => {
                if app_state.rendering.backends_sel > 0 {
                    app_state.rendering.backends_sel -= 1;
                }
            }
            _ => {}
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

    fn handle_key_event_logs_view(&self, key: KeyEvent, app_state: &mut AppState) {}
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
