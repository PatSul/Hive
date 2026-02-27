//! hive chat command - interactive TUI chat.

use std::io::Stdout;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;

use crate::api::{ChatResponse, CloudClient};
use crate::app::{ChatApp, SseEvent};
use crate::ui;

pub async fn run(model_override: Option<String>) -> Result<()> {
    let config = hive_core::HiveConfig::load()?;
    let client = CloudClient::new(
        config.cloud_api_url.as_deref(), config.cloud_jwt.as_deref(),
    );
    let model = model_override.unwrap_or_else(|| config.default_model.clone());
    let tier = config.cloud_tier.clone().unwrap_or_else(|| "free".into());
    let mut app = ChatApp::new(model, tier);
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_chat_loop(&mut terminal, &mut app, &client).await;
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

async fn run_chat_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut ChatApp,
    client: &CloudClient,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw_chat(f, app))?;
        if app.should_quit { break; }
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match handle_key(key, app) {
                    KeyAction::Quit => break,
                    KeyAction::Submit => {
                        if let Some(_text) = app.submit_input() {
                            app.waiting = true;
                            terminal.draw(|f| ui::draw_chat(f, app))?;
                            let messages = app.api_messages();
                            match client.chat_completion(&app.model, &messages).await {
                                Ok(resp) => process_sse_response(terminal, app, resp).await,
                                Err(e) => app.add_error(&e.to_string()),
                            }
                        }
                    }
                    KeyAction::Continue => {}
                }
            }
        }
    }
    Ok(())
}

enum KeyAction { Quit, Submit, Continue }

fn handle_key(key: KeyEvent, app: &mut ChatApp) -> KeyAction {
    if key.code == KeyCode::Esc
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c'))
    { return KeyAction::Quit; }
    if app.waiting { return KeyAction::Continue; }
    match key.code {
        KeyCode::Enter => KeyAction::Submit,
        KeyCode::Char(c) => { app.insert_char(c); KeyAction::Continue }
        KeyCode::Backspace => { app.delete_char_before(); KeyAction::Continue }
        KeyCode::Delete => { app.delete_char_at(); KeyAction::Continue }
        KeyCode::Left => { app.move_left(); KeyAction::Continue }
        KeyCode::Right => { app.move_right(); KeyAction::Continue }
        KeyCode::Home => { app.move_home(); KeyAction::Continue }
        KeyCode::End => { app.move_end(); KeyAction::Continue }
        KeyCode::Up => { app.scroll_offset = app.scroll_offset.saturating_add(1); KeyAction::Continue }
        KeyCode::Down => { app.scroll_offset = app.scroll_offset.saturating_sub(1); KeyAction::Continue }
        _ => KeyAction::Continue,
    }
}

async fn process_sse_response(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut ChatApp,
    response: reqwest::Response,
) {
    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => { app.add_error(&format!("Failed to read response: {}", e)); return; }
    };
    let mut final_response: Option<ChatResponse> = None;
    for line in body.lines() {
        match ChatApp::parse_sse_line(line) {
            Some(SseEvent::Chunk(chunk)) => {
                app.append_stream_chunk(&chunk);
                let _ = terminal.draw(|f| ui::draw_chat(f, app));
            }
            Some(SseEvent::Complete(resp)) => { final_response = Some(resp); }
            _ => {}
        }
    }
    app.finalize_stream(final_response.as_ref());
}
