use crate::app::App;
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

const CYAN: Color = Color::Rgb(0, 212, 255);

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let stats = match &app.gateway {
        Some(s) => s,
        None => {
            frame.render_widget(Paragraph::new("Loading..."), area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(8),
            Constraint::Length(6),
        ])
        .split(area);

    let summary = Paragraph::new(format!(
        "Total Requests: {}  |  Total Tokens: {}M",
        stats.total_requests,
        stats.total_tokens / 1_000_000
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Gateway Overview "),
    )
    .style(Style::default().fg(CYAN));
    frame.render_widget(summary, chunks[0]);

    let header = Row::new(vec!["Model", "Requests", "Tokens In", "Tokens Out"])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let model_rows: Vec<Row> = stats
        .models
        .iter()
        .map(|m| {
            Row::new(vec![
                m.model.clone(),
                m.requests.to_string(),
                format!("{}M", m.tokens_in / 1_000_000),
                format!("{}M", m.tokens_out / 1_000_000),
            ])
        })
        .collect();
    let widths = [
        Constraint::Min(30),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
    ];
    let model_table = Table::new(model_rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Per-Model Usage "),
        )
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)));
    frame.render_stateful_widget(model_table, chunks[1], &mut app.table_state);

    let pheader = Row::new(vec!["Provider", "Requests", "Cost (USD)"])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let provider_rows: Vec<Row> = stats
        .providers
        .iter()
        .map(|p| {
            Row::new(vec![
                p.provider.clone(),
                p.requests.to_string(),
                format!("${:.2}", p.cost_usd),
            ])
        })
        .collect();
    let pwidths = [
        Constraint::Min(15),
        Constraint::Length(12),
        Constraint::Length(15),
    ];
    let provider_table = Table::new(provider_rows, pwidths).header(pheader).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Provider Costs "),
    );
    frame.render_widget(provider_table, chunks[2]);

    let alert_rows: Vec<Row> = stats
        .budget_alerts
        .iter()
        .map(|a| {
            let color = if a.used_pct >= 90.0 {
                Color::Red
            } else if a.used_pct >= 75.0 {
                Color::Yellow
            } else {
                Color::White
            };
            Row::new(vec![
                a.user_email.clone(),
                format!("{:.0}%", a.used_pct),
                a.tier.clone(),
            ])
            .style(Style::default().fg(color))
        })
        .collect();
    let awidths = [
        Constraint::Min(25),
        Constraint::Length(10),
        Constraint::Length(8),
    ];
    let aheader = Row::new(vec!["User", "Used %", "Tier"])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let alert_table = Table::new(alert_rows, awidths).header(aheader).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Budget Alerts "),
    );
    frame.render_widget(alert_table, chunks[3]);
}
