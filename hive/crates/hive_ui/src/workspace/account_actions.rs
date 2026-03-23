use gpui::*;
use tracing::{error, info, warn};

use super::{
    AccountConnectPlatform, AccountDisconnectPlatform, AppAssistant, AppConfig, AppNotification,
    AppNotifications, HiveWorkspace, NotificationType,
};

pub(super) fn handle_account_connect_platform(
    _workspace: &mut HiveWorkspace,
    action: &AccountConnectPlatform,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let platform_str = action.platform.clone();
    let Some(platform) = hive_core::config::AccountPlatform::parse_platform(&platform_str) else {
        warn!("OAuth: unknown platform '{platform_str}'");
        return;
    };

    info!("OAuth: initiating connect for {platform_str}");

    let config = if cx.has_global::<AppConfig>() {
        cx.global::<AppConfig>().0.get()
    } else {
        hive_core::config::HiveConfig::default()
    };

    let oauth_config = oauth_config_for_platform(platform, &config);
    if oauth_config.client_id.is_empty() {
        warn!(
            "OAuth: no client_id configured for {platform_str}. Please set it in Settings -> Connected Accounts."
        );
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>().0.push(
                AppNotification::new(
                    NotificationType::Warning,
                    format!(
                        "No OAuth Client ID configured for {platform_str}. Go to Settings -> Connected Accounts to set it up."
                    ),
                )
                .with_title("OAuth Setup Required"),
            );
        }
        return;
    }

    let oauth_client = hive_integrations::OAuthClient::new(oauth_config);
    let (auth_url, _state) = oauth_client.authorization_url();

    if let Err(e) = open_url_in_browser(&auth_url) {
        error!("OAuth: failed to open browser: {e}");
        if cx.has_global::<AppNotifications>() {
            cx.global_mut::<AppNotifications>().0.push(
                AppNotification::new(
                    NotificationType::Error,
                    format!("Failed to open browser for {platform_str} authentication: {e}"),
                )
                .with_title("OAuth Error"),
            );
        }
        return;
    }

    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>().0.push(
            AppNotification::new(
                NotificationType::Info,
                format!(
                    "Opening browser for {platform_str} authentication. Complete the sign-in flow and paste the authorization code."
                ),
            )
            .with_title("OAuth: Browser Opened"),
        );
    }

    let platform_for_thread = platform;
    let platform_label = platform_str.clone();
    let result_flag = std::sync::Arc::new(std::sync::Mutex::new(
        None::<Result<hive_integrations::OAuthToken, String>>,
    ));
    let result_for_thread = std::sync::Arc::clone(&result_flag);

    std::thread::spawn(move || {
        let listener = match std::net::TcpListener::bind("127.0.0.1:8742") {
            Ok(listener) => listener,
            Err(e) => {
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Err(format!("Failed to start callback server: {e}")));
                return;
            }
        };

        let _ = listener.set_nonblocking(false);
        let timeout = std::time::Duration::from_secs(300);
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Err("OAuth callback timed out after 5 minutes".to_string()));
                return;
            }

            match listener.accept() {
                Ok((mut stream, _addr)) => {
                    use std::io::{Read, Write};

                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let request_str = String::from_utf8_lossy(&buf[..n]);

                    if let Some(code) = extract_oauth_code(&request_str) {
                        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
                                <html><body><h1>Authorization successful!</h1>\
                                <p>You can close this tab and return to Hive.</p></body></html>";
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();

                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build();
                        match runtime {
                            Ok(runtime) => {
                                let exchange_result = runtime.block_on(oauth_client.exchange_code(&code));
                                match exchange_result {
                                    Ok(token) => {
                                        *result_for_thread
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner()) = Some(Ok(token));
                                    }
                                    Err(e) => {
                                        *result_for_thread
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner()) =
                                            Some(Err(format!("Token exchange failed: {e}")));
                                    }
                                }
                            }
                            Err(e) => {
                                *result_for_thread.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(Err(format!(
                                        "Failed to create runtime for token exchange: {e}"
                                    )));
                            }
                        }
                        return;
                    }

                    let response = "HTTP/1.1 404 Not Found\r\n\r\nNot found";
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });

    let result_for_ui = std::sync::Arc::clone(&result_flag);
    let platform_for_ui = platform_for_thread;
    let platform_label_ui = platform_label;

    cx.spawn(async move |_this, app: &mut AsyncApp| {
        loop {
            if let Some(result) = result_for_ui
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                let _ = app.update(|cx| match result {
                    Ok(token) => {
                        info!("OAuth: successfully connected {platform_label_ui}");

                        if cx.has_global::<AppConfig>() {
                            let token_data = hive_core::config::OAuthTokenData {
                                access_token: token.access_token.clone(),
                                refresh_token: token.refresh_token.clone(),
                                expires_at: token.expires_at.map(|time| time.to_rfc3339()),
                            };
                            let _ = cx
                                .global::<AppConfig>()
                                .0
                                .set_oauth_token(platform_for_ui, &token_data);

                            let account = hive_core::config::ConnectedAccount {
                                platform: platform_for_ui,
                                account_name: platform_label_ui.clone(),
                                account_id: "oauth".to_string(),
                                scopes: Vec::new(),
                                connected_at: chrono::Utc::now().to_rfc3339(),
                                last_synced: None,
                                settings: hive_core::config::AccountSettings::default(),
                            };
                            let _ = cx.global::<AppConfig>().0.add_connected_account(account);
                        }

                        if cx.has_global::<AppAssistant>() {
                            let access = token.access_token.clone();
                            let assistant = &mut cx.global_mut::<AppAssistant>().0;
                            match platform_for_ui {
                                hive_core::config::AccountPlatform::Google => {
                                    assistant.set_gmail_token(access.clone());
                                    assistant.set_google_calendar_token(access);
                                }
                                hive_core::config::AccountPlatform::Microsoft => {
                                    assistant.set_outlook_token(access.clone());
                                    assistant.set_outlook_calendar_token(access);
                                }
                                _ => {}
                            }
                            info!(
                                "OAuth: injected token into assistant service for {platform_label_ui}"
                            );
                        }

                        if cx.has_global::<AppNotifications>() {
                            cx.global_mut::<AppNotifications>().0.push(
                                AppNotification::new(
                                    NotificationType::Success,
                                    format!(
                                        "{platform_label_ui} account connected successfully!"
                                    ),
                                )
                                .with_title("Account Connected"),
                            );
                        }
                    }
                    Err(e) => {
                        error!("OAuth: connection failed for {platform_label_ui}: {e}");
                        if cx.has_global::<AppNotifications>() {
                            cx.global_mut::<AppNotifications>().0.push(
                                AppNotification::new(
                                    NotificationType::Error,
                                    format!("{platform_label_ui} connection failed: {e}"),
                                )
                                .with_title("OAuth Error"),
                            );
                        }
                    }
                });
                break;
            }

            app.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;
        }
    })
    .detach();
}

pub(super) fn handle_account_disconnect_platform(
    _workspace: &mut HiveWorkspace,
    action: &AccountDisconnectPlatform,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let platform_str = action.platform.clone();
    let Some(platform) = hive_core::config::AccountPlatform::parse_platform(&platform_str) else {
        warn!("OAuth disconnect: unknown platform '{platform_str}'");
        return;
    };

    info!("OAuth: disconnecting {platform_str}");

    if cx.has_global::<AppConfig>() {
        let config = &cx.global::<AppConfig>().0;
        let _ = config.remove_oauth_token(platform);
        let _ = config.remove_connected_account(platform);
    }

    if cx.has_global::<AppAssistant>() {
        let assistant = &mut cx.global_mut::<AppAssistant>().0;
        match platform {
            hive_core::config::AccountPlatform::Google => {
                assistant.set_gmail_token(String::new());
                assistant.set_google_calendar_token(String::new());
            }
            hive_core::config::AccountPlatform::Microsoft => {
                assistant.set_outlook_token(String::new());
                assistant.set_outlook_calendar_token(String::new());
            }
            _ => {}
        }
    }

    if cx.has_global::<AppNotifications>() {
        cx.global_mut::<AppNotifications>().0.push(
            AppNotification::new(
                NotificationType::Info,
                format!("{platform_str} account disconnected."),
            )
            .with_title("Account Disconnected"),
        );
    }

    info!("OAuth: successfully disconnected {platform_str}");
}

fn extract_oauth_code(request: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for param in query.split('&') {
        if let Some(value) = param.strip_prefix("code=") {
            let decoded = value
                .replace("%3D", "=")
                .replace("%2F", "/")
                .replace("%2B", "+")
                .replace("%20", " ")
                .replace('+', " ");
            return Some(decoded);
        }
    }
    None
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    Ok(())
}

fn oauth_config_for_platform(
    platform: hive_core::config::AccountPlatform,
    config: &hive_core::config::HiveConfig,
) -> hive_integrations::OAuthConfig {
    use hive_core::config::AccountPlatform;

    let client_id = platform.client_id_from_config(config).unwrap_or_default();
    match platform {
        AccountPlatform::Google => hive_integrations::OAuthConfig {
            client_id: client_id.clone(),
            client_secret: None,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/gmail.readonly".to_string(),
                "https://www.googleapis.com/auth/calendar.readonly".to_string(),
            ],
        },
        AccountPlatform::Microsoft => hive_integrations::OAuthConfig {
            client_id: client_id.clone(),
            client_secret: None,
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                .to_string(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
            scopes: vec!["Mail.Read".to_string(), "Calendars.Read".to_string()],
        },
        AccountPlatform::GitHub => hive_integrations::OAuthConfig {
            client_id: client_id.clone(),
            client_secret: None,
            auth_url: "https://github.com/login/oauth/authorize".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
            scopes: vec!["repo".to_string(), "read:user".to_string()],
        },
        AccountPlatform::Slack => hive_integrations::OAuthConfig {
            client_id: client_id.clone(),
            client_secret: None,
            auth_url: "https://slack.com/oauth/v2/authorize".to_string(),
            token_url: "https://slack.com/api/oauth.v2.access".to_string(),
            redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
            scopes: vec!["channels:read".to_string(), "chat:write".to_string()],
        },
        AccountPlatform::Discord => hive_integrations::OAuthConfig {
            client_id: client_id.clone(),
            client_secret: None,
            auth_url: "https://discord.com/api/oauth2/authorize".to_string(),
            token_url: "https://discord.com/api/oauth2/token".to_string(),
            redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
            scopes: vec!["identify".to_string(), "guilds".to_string()],
        },
        AccountPlatform::Telegram => hive_integrations::OAuthConfig {
            client_id: client_id.clone(),
            client_secret: None,
            auth_url: "https://oauth.telegram.org/auth".to_string(),
            token_url: "https://oauth.telegram.org/auth".to_string(),
            redirect_uri: "http://127.0.0.1:8742/callback".to_string(),
            scopes: Vec::new(),
        },
    }
}
