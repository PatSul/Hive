//! Ratatui rendering for the Hive CLI.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ChatApp, Role};

const ACCENT: Color = Color::Rgb(0, 212, 255);
const DIM: Color = Color::Rgb(100, 100, 100);

pub fn draw_chat(frame: &mut Frame, app: &ChatApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());
    draw_messages(frame, app, chunks[0]);
    draw_input(frame, app, chunks[1]);
    draw_status_bar(frame, app, chunks[2]);
}

fn draw_messages(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    for msg in &app.messages {
        let (label_style, content_style) = match msg.role {
            Role::User => (
                Style::default().fg(ACCENT).bold(),
                Style::default().fg(Color::White),
            ),
            Role::Assistant => (
                Style::default().fg(Color::Green).bold(),
                Style::default().fg(Color::White),
            ),
            Role::System => (
                Style::default().fg(Color::Yellow).bold(),
                Style::default().fg(Color::Yellow),
            ),
        };
        lines.push(Line::from(vec![Span::styled(
            format!("[{}] ", msg.role.label()),
            label_style,
        )]));
        for text_line in msg.content.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", text_line),
                content_style,
            )));
        }
        lines.push(Line::from(""));
    }
    if app.waiting && !app.stream_buffer.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "[Hive] ",
            Style::default().fg(Color::Green).bold(),
        )]));
        for text_line in app.stream_buffer.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", text_line),
                Style::default().fg(Color::White),
            )));
        }
        lines.push(Line::from(""));
    } else if app.waiting {
        lines.push(Line::from(Span::styled(
            "  Thinking...",
            Style::default().fg(DIM).italic(),
        )));
    }
    let total_lines = lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = app.scroll_offset.min(max_scroll);
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(Span::styled(
                    " Hive Chat ",
                    Style::default().fg(ACCENT).bold(),
                )),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let input_text = if app.waiting {
        "Waiting for response...".to_string()
    } else {
        app.input.clone()
    };
    let input = Paragraph::new(input_text.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(if app.waiting {
                Style::default().fg(DIM)
            } else {
                Style::default().fg(ACCENT)
            })
            .title(Span::styled(" > ", Style::default().fg(ACCENT).bold())),
    );
    frame.render_widget(input, area);
    if !app.waiting {
        let cursor_x = area.x + 1 + app.cursor as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn draw_status_bar(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let left = format!(
        " Model: {} | Tier: {} | Tokens: {}",
        app.model, app.tier, app.session_tokens
    );
    let right = " Esc/Ctrl+C: Quit | Enter: Send ";
    let available_width = area.width as usize;
    let padding = available_width
        .saturating_sub(left.len())
        .saturating_sub(right.len());
    let bar_text = format!("{}{}{}", left, " ".repeat(padding), right);
    let bar =
        Paragraph::new(bar_text).style(Style::default().bg(Color::Rgb(30, 30, 40)).fg(ACCENT));
    frame.render_widget(bar, area);
}

pub fn print_login_banner() {
    println!();
    println!("  ============================");
    println!("       Hive Cloud Login");
    println!("  ============================");
    println!();
}

pub fn print_header(title: &str) {
    println!();
    println!("  --- {} ---", title);
    println!();
}
