use crate::app::App;
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

const CYAN: Color = Color::Rgb(0, 212, 255);

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Search bar
    let search_text = if app.search_active {
        format!("Filter: {}_", app.search_query)
    } else if !app.search_query.is_empty() {
        format!("Filter: {}", app.search_query)
    } else {
        "Press / to filter by email".to_string()
    };
    let search =
        Paragraph::new(search_text).block(Block::default().borders(Borders::ALL).title(" Search "));
    frame.render_widget(search, chunks[0]);

    let filtered = app.filtered_users();
    let header = Row::new(vec![
        "ID",
        "Email",
        "Tier",
        "Created",
        "Last Login",
        "Tokens Used",
    ])
    .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = filtered
        .iter()
        .map(|u| {
            let _tier_style = match u.tier.as_str() {
                "Pro" => Style::default().fg(Color::Yellow),
                "Team" => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::White),
            };
            Row::new(vec![
                u.id.clone(),
                u.email.clone(),
                u.tier.clone(),
                u.created_at.format("%Y-%m-%d").to_string(),
                u.last_login.format("%Y-%m-%d %H:%M").to_string(),
                format_tokens(u.usage_tokens),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Min(20),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(18),
        Constraint::Length(12),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Users "))
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(table, chunks[1], &mut app.table_state);
}

fn format_tokens(t: u64) -> String {
    if t >= 1_000_000 {
        format!("{:.1}M", t as f64 / 1_000_000.0)
    } else if t >= 1_000 {
        format!("{:.1}K", t as f64 / 1_000.0)
    } else {
        t.to_string()
    }
}
