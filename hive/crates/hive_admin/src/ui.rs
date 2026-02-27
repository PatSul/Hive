use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Tabs as RatatuiTabs},
    style::{Color, Modifier, Style},
};
use crate::app::{App, Tab};
use crate::tabs;

const CYAN: Color = Color::Rgb(0, 212, 255);
const DIM_WHITE: Color = Color::Rgb(180, 180, 180);

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_tabs(frame, app, chunks[0]);
    draw_content(frame, app, chunks[1]);
    draw_help_bar(frame, app, chunks[2]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<String> = Tab::ALL.iter().map(|t| t.label().to_string()).collect();
    let selected = Tab::ALL.iter().position(|t| *t == app.current_tab).unwrap_or(0);
    let tabs = RatatuiTabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Hive Cloud Admin "))
        .select(selected)
        .style(Style::default().fg(DIM_WHITE))
        .highlight_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn draw_content(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.current_tab {
        Tab::Dashboard => tabs::dashboard::draw(frame, app, area),
        Tab::Users => tabs::users::draw(frame, app, area),
        Tab::Gateway => tabs::gateway::draw(frame, app, area),
        Tab::Relay => tabs::relay::draw(frame, app, area),
        Tab::Sync => tabs::sync::draw(frame, app, area),
        Tab::Teams => tabs::teams::draw(frame, app, area),
    }
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help = if app.search_active {
        format!(" Search: {} | ESC cancel", app.search_query)
    } else {
        " Tab/Shift+Tab: switch | Up/Down: navigate | /: search | r: refresh | q: quit".to_string()
    };
    let p = Paragraph::new(help).style(Style::default().fg(DIM_WHITE).bg(Color::Rgb(30, 30, 30)));
    frame.render_widget(p, area);
}
