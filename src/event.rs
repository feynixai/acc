use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind, MouseButton};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::prelude::*;
use std::{io, time::Duration};

use crate::app::{App, InputMode};
use crate::handler;
use crate::command;
use crate::state::load_state;
use crate::tmux;
use crate::ui;

pub fn run_sidebar(workspace: &str) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let session = tmux::current_session();
    let mut app = App::new(load_state(workspace), session.clone(), workspace.to_string());

    app.sidebar_pane_id = tmux::sidebar_pane_id();

    // Reconnect folders to existing tmux windows
    tmux::reconnect_folders(&mut app.state.folders, &session);

    // Ensure current window has a folder
    handler::ensure_default_folder(&mut app);

    let tick_rate = Duration::from_millis(500);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match app.input_mode {
                        InputMode::AddFolder | InputMode::AddAgent => {
                            handler::handle_input_key(&mut app, key.code);
                        }
                        InputMode::Command => {
                            command::handle_command_key(&mut app, key.code);
                        }
                        InputMode::Search => {
                            command::handle_search_key(&mut app, key.code);
                        }
                        InputMode::SendPrompt => {
                            command::handle_send_key(&mut app, key.code);
                        }
                        InputMode::Normal => {
                            if app.confirm_delete.is_some() {
                                match key.code {
                                    KeyCode::Enter | KeyCode::Char('y') => handler::execute_delete(&mut app),
                                    KeyCode::Esc | KeyCode::Char('n') => { app.confirm_delete = None; }
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('q') => break,
                                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                                    KeyCode::Char(':') => command::start_command(&mut app),
                                    KeyCode::Char('/') => command::start_search(&mut app),
                                    KeyCode::Char('>') => command::start_send_prompt(&mut app),
                                    KeyCode::Char(c @ '1'..='9') => {
                                        command::jump_to_pane_number(&mut app, (c as u8 - b'0') as usize);
                                    }
                                    KeyCode::Char('f') => handler::start_add_folder(&mut app),
                                    KeyCode::Char('a') => handler::start_add_agent(&mut app),
                                    KeyCode::Char('h') => handler::split_agent(&mut app, 'h'),
                                    KeyCode::Char('v') => handler::split_agent(&mut app, 'v'),
                                    KeyCode::Char('d') => handler::close_selected_pane(&mut app),
                                    KeyCode::Char('s') => handler::stop_selected(&mut app),
                                    KeyCode::Char('r') => handler::restart_selected(&mut app),
                                    KeyCode::Char('x') => handler::remove_selected(&mut app),
                                    KeyCode::Char('u') => handler::undo_last(&mut app),
                                    KeyCode::Char('[') => handler::prev_window(&mut app),
                                    KeyCode::Char(']') => handler::next_window(&mut app),
                                    KeyCode::Char('n') => handler::next_pane(&mut app),
                                    KeyCode::Char('p') => handler::prev_pane(&mut app),
                                    KeyCode::Up => handler::move_selection(&mut app, -1),
                                    KeyCode::Down => handler::move_selection(&mut app, 1),
                                    KeyCode::Enter => handler::handle_enter(&mut app),
                                    _ => {}
                                }
                            }
                        }
                    }
                    if app.quit_requested { break; }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            if app.confirm_delete.is_some() {
                                app.confirm_delete = None;
                            } else if matches!(app.input_mode, InputMode::AddFolder | InputMode::AddAgent) {
                                let size = terminal.size()?;
                                let area_w = 48.min(size.width.saturating_sub(2));
                                let area_h = 7u16;
                                let px = (size.width.saturating_sub(area_w)) / 2;
                                let py = (size.height.saturating_sub(area_h)) / 2;
                                if mouse.column < px || mouse.column >= px + area_w
                                    || mouse.row < py || mouse.row >= py + area_h
                                {
                                    app.input_mode = InputMode::Normal;
                                    app.input_buf.clear();
                                }
                            } else if app.input_mode == InputMode::Normal {
                                handler::handle_click(&mut app, mouse.column, mouse.row, terminal.size()?);
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if app.input_mode == InputMode::Normal && app.confirm_delete.is_none() {
                                handler::move_selection(&mut app, -1);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            if app.input_mode == InputMode::Normal && app.confirm_delete.is_none() {
                                handler::move_selection(&mut app, 1);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        } else {
            // Tick: sync panes for ALL folders
            app.tick_message();
            handler::sync_all_folders(&mut app);
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
