use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
    style::{Color, Modifier, Style},
};
use crate::app::App;

const CYAN: Color = Color::Rgb(0, 212, 255);

fn format_bytes(b: u64) -> String {
    if b >= 1_073_741_824 { format!("{:.1} GB", b as f64 / 1_073_741_824.0) }
    else if b >= 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) }
    else if b >= 1_024 { format!("{:.1} KB", b as f64 / 1_024.0) }
    else { format!("{} B", b) }
}

fn stat_card(title: &str, value: &str) -> Paragraph<'static> {
    let text = vec![
        Line::from(Span::styled(title.to_string(), Style::default().fg(Color::Rgb(180, 180, 180)))),
        Line::from(Span::styled(value.to_string(), Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
    ];
    Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let stats = match &app.dashboard {
        Some(s) => s,
        None => {
            frame.render_widget(Paragraph::new("Loading..."), area);
            return;
        }
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(5), Constraint::Min(0)])
        .split(area);

    // Row 1: User stats
    let row1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
        .split(rows[0]);

    frame.render_widget(stat_card("Total Users", &stats.total_users.to_string()), row1[0]);
    frame.render_widget(stat_card("Free Tier", &stats.free_users.to_string()), row1[1]);
    frame.render_widget(stat_card("Pro Tier", &stats.pro_users.to_string()), row1[2]);
    frame.render_widget(stat_card("Team Tier", &stats.team_users.to_string()), row1[3]);

    // Row 2: Revenue and usage
    let row2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
        .split(rows[1]);

    frame.render_widget(stat_card("Est. Revenue", &format!("${:.0}", stats.revenue_estimate)), row2[0]);
    frame.render_widget(stat_card("Requests Today", &format!("{}", stats.gateway_requests_today)), row2[1]);
    frame.render_widget(stat_card("Relay Connections", &stats.active_relay_connections.to_string()), row2[2]);
    frame.render_widget(stat_card("Sync Storage", &format_bytes(stats.sync_storage_bytes)), row2[3]);

    // Row 3: Monthly summary
    let summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Monthly Requests: ", Style::default().fg(Color::Rgb(180, 180, 180))),
            Span::styled(format!("{}", stats.gateway_requests_month), Style::default().fg(CYAN)),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Monthly Summary "));
    frame.render_widget(summary, rows[2]);
}
