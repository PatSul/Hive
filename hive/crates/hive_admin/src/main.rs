mod api;
mod app;
mod tabs;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use std::io::stdout;
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(name = "hive-admin", about = "Hive Cloud admin terminal dashboard")]
struct Cli {
    /// Server URL for the Hive Cloud API
    #[arg(long, default_value = "http://localhost:3000")]
    server_url: String,

    /// Admin JWT token for authentication
    #[arg(long, env = "HIVE_ADMIN_TOKEN", hide_env_values = true, default_value = "")]
    token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("hive_admin=debug")
        .init();

    tracing::info!("Starting Hive Admin TUI, server: {}", cli.server_url);

    let api_client = api::ApiClient::new(&cli.server_url, &cli.token);
    let mut app = App::new(api_client);
    app.refresh_data().await;

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_secs(5);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) if !app.search_active => break,
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,
                    (_, KeyCode::Tab) if !app.search_active => app.next_tab(),
                    (KeyModifiers::SHIFT, KeyCode::BackTab) if !app.search_active => app.prev_tab(),
                    (_, KeyCode::Char('r')) if !app.search_active => {
                        app.refresh_data().await;
                        last_tick = Instant::now();
                    }
                    (_, KeyCode::Up) if !app.search_active => app.select_prev(),
                    (_, KeyCode::Down) if !app.search_active => app.select_next(),
                    (_, KeyCode::Char('/')) if !app.search_active => app.toggle_search(),
                    (_, KeyCode::Esc) => app.cancel_search(),
                    (_, KeyCode::Backspace) if app.search_active => app.search_backspace(),
                    (_, KeyCode::Char(c)) if app.search_active => app.search_input(c),
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.refresh_data().await;
            last_tick = Instant::now();
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
