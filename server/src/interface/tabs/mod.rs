use crate::interface::tabs::tab_backends::BackendEvent;
use crate::interface::tabs::tab_main_view::MainViewEvent;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

mod tab_backends;
mod tab_controller;
mod tab_logs;
mod tab_main_view;

pub enum TabEvent {
    None,
    Cycle,
    Quit,
    Backend(BackendEvent),
    Main(MainViewEvent),
}

trait Tab {
    fn get_title(&self) -> String;
    fn render(&mut self, frame: &mut ratatui::Frame, chunk: Rect);
    fn update(&mut self, key: KeyEvent) -> TabEvent;
}
