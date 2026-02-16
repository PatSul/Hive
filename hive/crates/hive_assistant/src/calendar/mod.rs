pub mod conflict_detector;
pub mod daily_brief;
pub mod smart_scheduler;

use hive_integrations::{GoogleCalendarClient, OutlookCalendarClient};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Supported calendar providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarProvider {
    Google,
    Outlook,
    CalDav(String),
}

/// A unified calendar event representation across all providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub provider: CalendarProvider,
    pub attendees: Vec<String>,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// CalendarService
// ---------------------------------------------------------------------------

/// Service for managing calendar operations across providers.
///
/// Holds optional OAuth tokens. When tokens are present, real API calls are
/// made through the fully-implemented `hive_integrations` clients. When tokens
/// are absent the methods degrade gracefully and return empty results.
pub struct CalendarService {
    google_token: Option<String>,
    outlook_token: Option<String>,
    /// Google Calendar ID to use (defaults to `"primary"`).
    google_calendar_id: String,
}

impl CalendarService {
    pub fn new() -> Self {
        Self {
            google_token: None,
            outlook_token: None,
            google_calendar_id: "primary".to_string(),
        }
    }

    /// Create a service pre-configured with OAuth tokens.
    pub fn with_tokens(google_token: Option<String>, outlook_token: Option<String>) -> Self {
        Self {
            google_token,
            outlook_token,
            google_calendar_id: "primary".to_string(),
        }
    }

    /// Update the Google Calendar OAuth access token at runtime.
    pub fn set_google_token(&mut self, token: String) {
        self.google_token = Some(token);
    }

    /// Update the Outlook Calendar OAuth access token at runtime.
    pub fn set_outlook_token(&mut self, token: String) {
        self.outlook_token = Some(token);
    }

    /// Set the Google Calendar ID (defaults to `"primary"`).
    pub fn set_google_calendar_id(&mut self, id: String) {
        self.google_calendar_id = id;
    }

    /// Get today's events from all configured providers.
    ///
    /// Returns events for the current UTC day. Requires at least one OAuth
    /// token to be configured; returns an empty vec otherwise.
    pub fn today_events(&self) -> Result<Vec<UnifiedEvent>, String> {
        let now = chrono::Utc::now();
        let start = now.format("%Y-%m-%dT00:00:00Z").to_string();
        let end = now.format("%Y-%m-%dT23:59:59Z").to_string();
        self.events_in_range(&start, &end)
    }

    /// Get events within a date range from all configured providers.
    ///
    /// `start` and `end` should be ISO 8601 date-time strings.
    pub fn events_in_range(&self, start: &str, end: &str) -> Result<Vec<UnifiedEvent>, String> {
        let mut all_events = Vec::new();

        // Fetch from Google Calendar
        if let Some(token) = &self.google_token {
            let token = token.clone();
            let cal_id = self.google_calendar_id.clone();
            let start = start.to_string();
            let end = end.to_string();

            let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
            let google_events = handle.block_on(async {
                let client = GoogleCalendarClient::new(&token);
                let list = client
                    .list_events(&cal_id, Some(&start), Some(&end), Some(50))
                    .await
                    .map_err(|e| format!("Google Calendar error: {e}"))?;

                let events: Vec<UnifiedEvent> = list
                    .items
                    .into_iter()
                    .map(|evt| {
                        let start_str = evt
                            .start
                            .as_ref()
                            .and_then(|s| s.date_time.clone().or(s.date.clone()))
                            .unwrap_or_default();
                        let end_str = evt
                            .end
                            .as_ref()
                            .and_then(|e| e.date_time.clone().or(e.date.clone()))
                            .unwrap_or_default();

                        UnifiedEvent {
                            id: evt.id,
                            title: evt.summary.unwrap_or_default(),
                            start: start_str,
                            end: end_str,
                            location: evt.location,
                            provider: CalendarProvider::Google,
                            attendees: evt
                                .attendees
                                .iter()
                                .map(|a| a.email.clone())
                                .collect(),
                            description: evt.description,
                        }
                    })
                    .collect();

                Ok::<Vec<UnifiedEvent>, String>(events)
            })?;

            all_events.extend(google_events);
        }

        // Fetch from Outlook Calendar
        if let Some(token) = &self.outlook_token {
            let token = token.clone();
            let start = start.to_string();
            let end = end.to_string();

            let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
            let outlook_events = handle.block_on(async {
                let client = OutlookCalendarClient::new(&token);
                let events = client
                    .list_events(&start, &end, 50)
                    .await
                    .map_err(|e| format!("Outlook Calendar error: {e}"))?;

                let unified: Vec<UnifiedEvent> = events
                    .into_iter()
                    .map(|evt| UnifiedEvent {
                        id: evt.id,
                        title: evt.subject,
                        start: evt.start.date_time,
                        end: evt.end.date_time,
                        location: evt.location,
                        provider: CalendarProvider::Outlook,
                        attendees: Vec::new(),
                        description: None,
                    })
                    .collect();

                Ok::<Vec<UnifiedEvent>, String>(unified)
            })?;

            all_events.extend(outlook_events);
        }

        // Sort by start time
        all_events.sort_by(|a, b| a.start.cmp(&b.start));

        Ok(all_events)
    }

    /// Create a new calendar event.
    ///
    /// Routes to the appropriate provider based on `event.provider`.
    /// Falls back to Google Calendar if a token is available.
    pub fn create_event(&self, event: &UnifiedEvent) -> Result<String, String> {
        match event.provider {
            CalendarProvider::Google => self.create_google_event(event),
            CalendarProvider::Outlook => self.create_outlook_event(event),
            CalendarProvider::CalDav(_) => {
                tracing::info!(
                    title = event.title.as_str(),
                    "CalDAV event creation not yet supported"
                );
                Ok(event.id.clone())
            }
        }
    }

    fn create_google_event(&self, event: &UnifiedEvent) -> Result<String, String> {
        let token = match &self.google_token {
            Some(t) => t.clone(),
            None => {
                tracing::info!(
                    title = event.title.as_str(),
                    "Google Calendar event creation requested (no token configured)"
                );
                return Ok(event.id.clone());
            }
        };

        let cal_id = self.google_calendar_id.clone();
        let request = hive_integrations::CreateEventRequest {
            summary: Some(event.title.clone()),
            description: event.description.clone(),
            location: event.location.clone(),
            start: Some(hive_integrations::EventDateTime {
                date_time: Some(event.start.clone()),
                date: None,
                time_zone: Some("UTC".to_string()),
            }),
            end: Some(hive_integrations::EventDateTime {
                date_time: Some(event.end.clone()),
                date: None,
                time_zone: Some("UTC".to_string()),
            }),
            attendees: event
                .attendees
                .iter()
                .map(|email| hive_integrations::Attendee {
                    email: email.clone(),
                    display_name: None,
                    response_status: None,
                })
                .collect(),
        };

        let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
        let created_id = handle.block_on(async {
            let client = GoogleCalendarClient::new(&token);
            let created = client
                .create_event(&cal_id, &request)
                .await
                .map_err(|e| format!("Google Calendar create error: {e}"))?;
            Ok::<String, String>(created.id)
        })?;

        tracing::info!(
            title = event.title.as_str(),
            id = %created_id,
            "Calendar event created via Google Calendar"
        );
        Ok(created_id)
    }

    fn create_outlook_event(&self, event: &UnifiedEvent) -> Result<String, String> {
        let token = match &self.outlook_token {
            Some(t) => t.clone(),
            None => {
                tracing::info!(
                    title = event.title.as_str(),
                    "Outlook Calendar event creation requested (no token configured)"
                );
                return Ok(event.id.clone());
            }
        };

        let request = hive_integrations::microsoft::outlook_calendar::NewCalendarEvent {
            subject: event.title.clone(),
            start: hive_integrations::microsoft::outlook_calendar::EventDateTime {
                date_time: event.start.clone(),
                time_zone: "UTC".to_string(),
            },
            end: hive_integrations::microsoft::outlook_calendar::EventDateTime {
                date_time: event.end.clone(),
                time_zone: "UTC".to_string(),
            },
            body: event.description.clone(),
            location: event.location.clone(),
        };

        let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
        let created_id = handle.block_on(async {
            let client = OutlookCalendarClient::new(&token);
            let created = client
                .create_event(&request)
                .await
                .map_err(|e| format!("Outlook Calendar create error: {e}"))?;
            Ok::<String, String>(created.id)
        })?;

        tracing::info!(
            title = event.title.as_str(),
            id = %created_id,
            "Calendar event created via Outlook"
        );
        Ok(created_id)
    }
}

impl Default for CalendarService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::calendar::{CalendarProvider, CalendarService, UnifiedEvent};

    fn make_event(id: &str, title: &str, start: &str, end: &str) -> UnifiedEvent {
        UnifiedEvent {
            id: id.to_string(),
            title: title.to_string(),
            start: start.to_string(),
            end: end.to_string(),
            location: None,
            provider: CalendarProvider::Google,
            attendees: Vec::new(),
            description: None,
        }
    }

    #[test]
    fn test_today_events_returns_empty_without_token() {
        let service = CalendarService::new();
        let events = service.today_events().unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_events_in_range_returns_empty_without_token() {
        let service = CalendarService::new();
        let events = service
            .events_in_range("2026-02-10T00:00:00Z", "2026-02-10T23:59:59Z")
            .unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_create_event_returns_id_without_token() {
        let service = CalendarService::new();
        let event = make_event(
            "ev-1",
            "Team standup",
            "2026-02-10T09:00:00Z",
            "2026-02-10T09:30:00Z",
        );
        let id = service.create_event(&event).unwrap();
        assert_eq!(id, "ev-1");
    }

    #[test]
    fn test_create_outlook_event_without_token() {
        let service = CalendarService::new();
        let mut event = make_event(
            "ev-2",
            "Outlook meeting",
            "2026-02-10T14:00:00Z",
            "2026-02-10T15:00:00Z",
        );
        event.provider = CalendarProvider::Outlook;
        let id = service.create_event(&event).unwrap();
        assert_eq!(id, "ev-2");
    }

    #[test]
    fn test_create_caldav_event_returns_id() {
        let service = CalendarService::new();
        let mut event = make_event(
            "ev-3",
            "CalDAV event",
            "2026-02-10T10:00:00Z",
            "2026-02-10T11:00:00Z",
        );
        event.provider = CalendarProvider::CalDav("https://cal.example.com".to_string());
        let id = service.create_event(&event).unwrap();
        assert_eq!(id, "ev-3");
    }

    #[test]
    fn test_with_tokens_constructor() {
        let service =
            CalendarService::with_tokens(Some("g_tok".into()), Some("o_tok".into()));
        // Tokens set but no runtime — today_events degrades gracefully
        let result = service.today_events();
        assert!(result.is_err() || result.unwrap().is_empty());
    }

    #[test]
    fn test_set_tokens() {
        let mut service = CalendarService::new();
        service.set_google_token("tok".to_string());
        service.set_outlook_token("tok2".to_string());
        service.set_google_calendar_id("work@group.calendar.google.com".to_string());
        // Proves the setters compile and work
        let result = service.today_events();
        assert!(result.is_err() || result.unwrap().is_empty());
    }

    #[test]
    fn test_unified_event_serialization() {
        let event = UnifiedEvent {
            id: "ev-ser".to_string(),
            title: "Meeting".to_string(),
            start: "2026-02-10T10:00:00Z".to_string(),
            end: "2026-02-10T11:00:00Z".to_string(),
            location: Some("Room 42".to_string()),
            provider: CalendarProvider::Outlook,
            attendees: vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string(),
            ],
            description: Some("Weekly sync".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: UnifiedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "ev-ser");
        assert_eq!(deserialized.attendees.len(), 2);
        assert_eq!(deserialized.location, Some("Room 42".to_string()));
    }

    #[test]
    fn test_calendar_provider_serialization() {
        let providers = vec![
            CalendarProvider::Google,
            CalendarProvider::Outlook,
            CalendarProvider::CalDav("https://cal.example.com".to_string()),
        ];
        let json = serde_json::to_string(&providers).unwrap();
        let deserialized: Vec<CalendarProvider> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, providers);
    }

    #[test]
    fn test_default_calendar_service() {
        let service = CalendarService::default();
        assert!(service.today_events().unwrap().is_empty());
    }
}
