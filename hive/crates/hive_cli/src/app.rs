//! Main TUI app state for the chat command.

use crate::api::{ChatMessage, ChatResponse, SseChunk};

#[derive(Debug, Clone)]
pub struct Message { pub role: Role, pub content: String }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role { User, Assistant, System }

impl Role {
    pub fn label(&self) -> &'static str {
        match self {
            Role::User => "You",
            Role::Assistant => "Hive",
            Role::System => "System",
        }
    }
}

pub struct ChatApp {
    pub input: String,
    pub cursor: usize,
    pub messages: Vec<Message>,
    pub model: String,
    pub tier: String,
    pub session_tokens: i32,
    pub waiting: bool,
    pub scroll_offset: u16,
    pub should_quit: bool,
    pub stream_buffer: String,
}

impl ChatApp {
    pub fn new(model: String, tier: String) -> Self {
        let mut app = Self {
            input: String::new(), cursor: 0, messages: Vec::new(),
            model, tier, session_tokens: 0, waiting: false,
            scroll_offset: 0, should_quit: false, stream_buffer: String::new(),
        };
        app.messages.push(Message {
            role: Role::System,
            content: "Welcome to Hive Chat. Type a message and press Enter. Esc/Ctrl+C to quit.".into(),
        });
        app
    }

    pub fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            let prev = self.input[..self.cursor].char_indices()
                .next_back().map(|(i, _)| i).unwrap_or(0);
            self.input.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub fn delete_char_at(&mut self) {
        if self.cursor < self.input.len() {
            let next = self.input[self.cursor..].char_indices()
                .nth(1).map(|(i, _)| self.cursor + i).unwrap_or(self.input.len());
            self.input.drain(self.cursor..next);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor].char_indices()
                .next_back().map(|(i, _)| i).unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor = self.input[self.cursor..].char_indices()
                .nth(1).map(|(i, _)| self.cursor + i).unwrap_or(self.input.len());
        }
    }

    pub fn move_home(&mut self) { self.cursor = 0; }
    pub fn move_end(&mut self) { self.cursor = self.input.len(); }

    pub fn submit_input(&mut self) -> Option<String> {
        let text = self.input.trim().to_string();
        if text.is_empty() { return None; }
        self.input.clear();
        self.cursor = 0;
        self.messages.push(Message { role: Role::User, content: text.clone() });
        self.scroll_offset = 0;
        Some(text)
    }

    pub fn api_messages(&self) -> Vec<ChatMessage> {
        self.messages.iter().filter(|m| m.role != Role::System)
            .map(|m| ChatMessage {
                role: match m.role {
                    Role::User => "user".into(),
                    Role::Assistant => "assistant".into(),
                    Role::System => "system".into(),
                }, content: m.content.clone(),
            }).collect()
    }

    pub fn append_stream_chunk(&mut self, chunk: &str) {
        self.stream_buffer.push_str(chunk);
    }

    pub fn finalize_stream(&mut self, response: Option<&ChatResponse>) {
        let content = if self.stream_buffer.is_empty() {
            response.map(|r| r.content.clone()).unwrap_or_else(|| "(empty)".into())
        } else { std::mem::take(&mut self.stream_buffer) };
        self.messages.push(Message { role: Role::Assistant, content });
        if let Some(r) = response {
            self.session_tokens += r.usage.input_tokens + r.usage.output_tokens;
        }
        self.waiting = false;
        self.scroll_offset = 0;
    }

    pub fn add_error(&mut self, err: &str) {
        self.messages.push(Message {
            role: Role::System, content: format!("Error: {}", err),
        });
        self.waiting = false;
        self.stream_buffer.clear();
    }

    pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') { return None; }
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" { return Some(SseEvent::Done); }
            if let Ok(ch) = serde_json::from_str::<SseChunk>(data) {
                return Some(SseEvent::Chunk(ch.chunk));
            }
            if let Ok(r) = serde_json::from_str::<ChatResponse>(data) {
                return Some(SseEvent::Complete(r));
            }
        }
        if let Some(rest) = line.strip_prefix("event: ") {
            if rest.trim() == "done" { return Some(SseEvent::EventType("done".into())); }
        }
        None
    }
}

pub enum SseEvent {
    Chunk(String),
    Complete(ChatResponse),
    EventType(String),
    Done,
}
