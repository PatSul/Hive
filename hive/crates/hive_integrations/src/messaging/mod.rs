pub mod cross_channel;
pub mod discord;
pub mod google_chat;
pub mod hub;
#[cfg(target_os = "macos")]
pub mod imessage;
pub mod matrix;
pub mod provider;
pub mod signal;
pub mod slack;
pub mod teams;
pub mod telegram;
pub mod webchat;
pub mod whatsapp;

pub use cross_channel::CrossChannelService;
pub use discord::DiscordProvider;
pub use google_chat::GoogleChatProvider;
pub use hub::MessagingHub;
#[cfg(target_os = "macos")]
pub use imessage::IMessageProvider;
pub use matrix::MatrixProvider;
pub use provider::{
    Attachment, Channel, IncomingMessage, MessagingProvider, Platform, SentMessage,
};
pub use signal::SignalProvider;
pub use slack::SlackProvider;
pub use teams::TeamsProvider;
pub use telegram::TelegramProvider;
pub use webchat::WebChatProvider;
pub use whatsapp::WhatsAppProvider;
