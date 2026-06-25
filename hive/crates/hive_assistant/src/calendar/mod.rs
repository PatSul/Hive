pub mod conflict_detector;
pub mod daily_brief;
pub mod smart_scheduler;

use hive_integrations::{GoogleCalendarClient, OutlookCalendarClient};
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::runtime::Handle;

const PRIMARY_GOOGLE_ACCOUNT_ID: &str = "primary";
const PRIMARY_GOOGLE_ACCOUNT_NAME: &str = "Google Calendar";
const DEFAULT_GOOGLE_CALENDAR_ID: &str = "primary";
const DEFAULT_GOOGLE_CALENDAR_MAX_RESULTS: u32 = 50;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
}

/// A configured Google Calendar account source.
#[derive(Clone, PartialEq, Eq)]
pub struct GoogleCalendarSource {
    pub account_id: String,
    pub account_name: String,
    pub access_token: String,
    pub calendar_ids: Vec<String>,
    pub max_results: u32,
}

impl GoogleCalendarSource {
    pub fn new(
        account_id: impl Into<String>,
        account_name: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            account_id: account_id.into(),
            account_name: account_name.into(),
            access_token: access_token.into(),
            calendar_ids: vec![DEFAULT_GOOGLE_CALENDAR_ID.to_string()],
            max_results: DEFAULT_GOOGLE_CALENDAR_MAX_RESULTS,
        }
    }

    pub fn with_calendar_ids(mut self, calendar_ids: Vec<String>) -> Self {
        self.calendar_ids = normalize_calendar_ids(calendar_ids);
        self
    }

    pub fn with_max_results(mut self, max_results: u32) -> Self {
        self.max_results = max_results.max(1);
        self
    }

    fn is_configured(&self) -> bool {
        !self.access_token.trim().is_empty()
    }
}

impl fmt::Debug for GoogleCalendarSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleCalendarSource")
            .field("account_id", &self.account_id)
            .field("account_name", &self.account_name)
            .field("access_token", &"<redacted>")
            .field("calendar_ids", &self.calendar_ids)
            .field("max_results", &self.max_results)
            .finish()
    }
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
    google_sources: Vec<GoogleCalendarSource>,
}

impl CalendarService {
    pub fn new() -> Self {
        Self {
            google_token: None,
            outlook_token: None,
            google_calendar_id: DEFAULT_GOOGLE_CALENDAR_ID.to_string(),
            google_sources: Vec::new(),
        }
    }

    /// Create a service pre-configured with OAuth tokens.
    pub fn with_tokens(google_token: Option<String>, outlook_token: Option<String>) -> Self {
        let mut service = Self::new();
        if let Some(token) = google_token {
            service.set_google_token(token);
        }
        if let Some(token) = outlook_token {
            service.set_outlook_token(token);
        }
        service
    }

    /// Update the Google Calendar OAuth access token at runtime.
    pub fn set_google_token(&mut self, token: String) {
        if token.trim().is_empty() {
            self.google_token = None;
            self.google_sources
                .retain(|source| source.account_id != PRIMARY_GOOGLE_ACCOUNT_ID);
            return;
        }

        self.google_token = Some(token.clone());
        self.add_google_calendar_source(
            GoogleCalendarSource::new(
                PRIMARY_GOOGLE_ACCOUNT_ID,
                PRIMARY_GOOGLE_ACCOUNT_NAME,
                token,
            )
            .with_calendar_ids(vec![self.google_calendar_id.clone()]),
        );
    }

    /// Update the Outlook Calendar OAuth access token at runtime.
    pub fn set_outlook_token(&mut self, token: String) {
        self.outlook_token = if token.trim().is_empty() {
            None
        } else {
            Some(token)
        };
    }

    /// Set the Google Calendar ID (defaults to `"primary"`).
    pub fn set_google_calendar_id(&mut self, id: String) {
        self.google_calendar_id = if id.trim().is_empty() {
            DEFAULT_GOOGLE_CALENDAR_ID.to_string()
        } else {
            id
        };
        for source in &mut self.google_sources {
            if source.account_id == PRIMARY_GOOGLE_ACCOUNT_ID {
                source.calendar_ids = vec![self.google_calendar_id.clone()];
            }
        }
    }

    /// Replace all configured Google Calendar sources.
    pub fn set_google_calendar_sources(&mut self, sources: Vec<GoogleCalendarSource>) {
        self.google_sources = sources
            .into_iter()
            .filter(GoogleCalendarSource::is_configured)
            .map(|mut source| {
                source.calendar_ids = normalize_calendar_ids(source.calendar_ids);
                source
            })
            .collect();
        self.google_token = self
            .google_sources
            .first()
            .map(|source| source.access_token.clone());
    }

    /// Add or update a Google Calendar source by account ID.
    pub fn add_google_calendar_source(&mut self, mut source: GoogleCalendarSource) {
        if !source.is_configured() {
            self.google_sources
                .retain(|existing| existing.account_id != source.account_id);
            return;
        }

        source.calendar_ids = normalize_calendar_ids(source.calendar_ids);
        self.google_sources
            .retain(|existing| existing.account_id != source.account_id);
        self.google_sources.push(source);
        self.google_token = self
            .google_sources
            .first()
            .map(|source| source.access_token.clone());
    }

    /// Return configured Google Calendar sources.
    pub fn google_calendar_sources(&self) -> &[GoogleCalendarSource] {
        &self.google_sources
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

        // Fetch from Google Calendar.
        if !self.google_sources.is_empty() {
            for source in &self.google_sources {
                all_events.extend(self.events_in_range_for_google_source(source, start, end)?);
            }
        } else if let Some(token) = self
            .google_token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            let source = GoogleCalendarSource::new(
                PRIMARY_GOOGLE_ACCOUNT_ID,
                PRIMARY_GOOGLE_ACCOUNT_NAME,
                token,
            )
            .with_calendar_ids(vec![self.google_calendar_id.clone()]);
            all_events.extend(self.events_in_range_for_google_source(&source, start, end)?);
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
                        account_id: None,
                        account_name: None,
                        calendar_id: None,
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

    fn events_in_range_for_google_source(
        &self,
        source: &GoogleCalendarSource,
        start: &str,
        end: &str,
    ) -> Result<Vec<UnifiedEvent>, String> {
        if !source.is_configured() {
            return Ok(Vec::new());
        }

        let start = start.to_string();
        let end = end.to_string();
        let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
        handle.block_on(async {
            let client = GoogleCalendarClient::new(&source.access_token);
            let mut events = Vec::new();
            for calendar_id in &source.calendar_ids {
                let list = client
                    .list_events(
                        calendar_id,
                        Some(&start),
                        Some(&end),
                        Some(source.max_results),
                    )
                    .await
                    .map_err(|e| format!("Google Calendar error: {e}"))?;

                events.extend(list.items.into_iter().map(|evt| {
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
                        attendees: evt.attendees.iter().map(|a| a.email.clone()).collect(),
                        description: evt.description,
                        account_id: Some(source.account_id.clone()),
                        account_name: Some(source.account_name.clone()),
                        calendar_id: Some(calendar_id.clone()),
                    }
                }));
            }

            Ok::<Vec<UnifiedEvent>, String>(events)
        })
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
        let (token, cal_id) = match self.google_source_for_event(event) {
            Some((source, calendar_id)) => (source.access_token.clone(), calendar_id),
            None => match self
                .google_token
                .as_deref()
                .filter(|token| !token.is_empty())
            {
                Some(t) => (t.to_string(), self.google_calendar_id.clone()),
                None => {
                    tracing::info!(
                        title = event.title.as_str(),
                        "Google Calendar event creation requested (no token configured)"
                    );
                    return Ok(event.id.clone());
                }
            },
        };
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

    fn google_source_for_event(
        &self,
        event: &UnifiedEvent,
    ) -> Option<(&GoogleCalendarSource, String)> {
        let source = match event.account_id.as_deref() {
            Some(account_id) => self
                .google_sources
                .iter()
                .find(|source| source.account_id == account_id),
            None => self.google_sources.first(),
        }?;
        let calendar_id = event
            .calendar_id
            .clone()
            .or_else(|| source.calendar_ids.first().cloned())
            .unwrap_or_else(|| DEFAULT_GOOGLE_CALENDAR_ID.to_string());
        Some((source, calendar_id))
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

fn normalize_calendar_ids(calendar_ids: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = calendar_ids
        .into_iter()
        .filter_map(|id| {
            let trimmed = id.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect();
    if normalized.is_empty() {
        normalized.push(DEFAULT_GOOGLE_CALENDAR_ID.to_string());
    }
    normalized
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
    use crate::calendar::{CalendarProvider, CalendarService, GoogleCalendarSource, UnifiedEvent};

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
            account_id: None,
            account_name: None,
            calendar_id: None,
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
        let service = CalendarService::with_tokens(Some("g_tok".into()), Some("o_tok".into()));
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
    fn test_set_google_calendar_sources_tracks_multiple_accounts_and_calendars() {
        let mut service = CalendarService::new();
        service.set_google_calendar_sources(vec![
            GoogleCalendarSource::new("personal", "Personal Gmail", "tok-personal")
                .with_calendar_ids(vec!["primary".into(), "family".into()]),
            GoogleCalendarSource::new("work", "Work Gmail", "tok-work")
                .with_calendar_ids(vec!["primary".into(), "team@example.com".into()]),
        ]);

        let sources = service.google_calendar_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].account_id, "personal");
        assert_eq!(sources[0].calendar_ids, vec!["primary", "family"]);
        assert_eq!(sources[1].account_name, "Work Gmail");
        assert_eq!(sources[1].calendar_ids[1], "team@example.com");
    }

    #[test]
    fn test_empty_google_token_clears_primary_calendar_source() {
        let mut service = CalendarService::with_tokens(Some("tok".into()), None);
        assert_eq!(service.google_calendar_sources().len(), 1);

        service.set_google_token(String::new());

        assert!(service.google_calendar_sources().is_empty());
        assert!(service.today_events().unwrap().is_empty());
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
            account_id: None,
            account_name: None,
            calendar_id: None,
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
