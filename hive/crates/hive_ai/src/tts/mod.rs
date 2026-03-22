//! Text-to-Speech provider trait, types, and service orchestrator.
//!
//! Follows the same async-trait provider pattern used by `hive_ai::providers`.
//! Each TTS backend implements [`TtsProvider`]; the [`TtsService`] routes
//! requests, manages playback, and caches synthesised audio.

pub mod elevenlabs;
pub mod f5;
pub mod openai_tts;
pub mod qwen3;
pub mod service;
pub mod telnyx;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that any TTS provider may return.
#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Invalid API key")]
    InvalidKey,

    #[error("Rate limited")]
    RateLimit,

    #[error("Voice not found: {0}")]
    VoiceNotFound(String),

    #[error("Voice cloning not supported by this provider")]
    CloningNotSupported,

    #[error("Provider unavailable: {0}")]
    Unavailable(String),

    #[error("Audio format error: {0}")]
    AudioFormat(String),

    #[error("TTS error: {0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Identifies which TTS provider to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsProviderType {
    ElevenLabs,
    OpenAi,
    Qwen3,
    F5Tts,
    Telnyx,
}

impl TtsProviderType {
    /// Parse from a config string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "elevenlabs" | "eleven_labs" => Some(Self::ElevenLabs),
            "openai" | "openai_tts" => Some(Self::OpenAi),
            "qwen3" | "qwen3_tts" => Some(Self::Qwen3),
            "f5" | "f5_tts" | "f5tts" => Some(Self::F5Tts),
            "telnyx" | "telnyx_naturalhd" => Some(Self::Telnyx),
            _ => None,
        }
    }

    /// Config-friendly string identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ElevenLabs => "elevenlabs",
            Self::OpenAi => "openai",
            Self::Qwen3 => "qwen3",
            Self::F5Tts => "f5",
            Self::Telnyx => "telnyx",
        }
    }
}

/// Output audio format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Wav,
    Mp3,
    Opus,
    Pcm,
    Flac,
    Aac,
}

impl AudioFormat {
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::Mp3 => "audio/mpeg",
            Self::Opus => "audio/opus",
            Self::Pcm => "audio/pcm",
            Self::Flac => "audio/flac",
            Self::Aac => "audio/aac",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Pcm => "pcm",
            Self::Flac => "flac",
            Self::Aac => "aac",
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Metadata about a voice available from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
    pub preview_url: Option<String>,
    pub is_cloned: bool,
}

/// Request to synthesise speech.
#[derive(Debug, Clone)]
pub struct TtsRequest {
    pub text: String,
    pub voice_id: String,
    pub speed: f32,
    pub format: AudioFormat,
}

impl TtsRequest {
    pub fn new(text: impl Into<String>, voice_id: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice_id: voice_id.into(),
            speed: 1.0,
            format: AudioFormat::Mp3,
        }
    }

    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed.clamp(0.25, 4.0);
        self
    }

    pub fn with_format(mut self, format: AudioFormat) -> Self {
        self.format = format;
        self
    }
}

/// Synthesised audio data returned by a provider.
#[derive(Debug, Clone)]
pub struct AudioData {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
    pub sample_rate: u32,
}

impl AudioData {
    /// Play the audio data using the platform's native audio playback.
    ///
    /// Writes the audio bytes to a temporary file and shells out to a
    /// platform command. This is a blocking call — run on a background
    /// thread to avoid stalling the UI.
    pub fn play(&self) -> Result<(), TtsError> {
        play_audio_data(self)
    }
}

/// Play synthesized audio bytes via the platform's native audio command.
///
/// The audio is written to a temp file under `~/.hive/tts_cache/` (or the
/// system temp dir as fallback) and played with a platform command:
/// - **Windows**: `powershell -c (New-Object Media.SoundPlayer '<path>').PlaySync()`
///   for WAV, or `Start-Process` for other formats via the default media player.
/// - **macOS**: `afplay`
/// - **Linux**: `aplay` (WAV) or `ffplay`/`paplay` (other formats)
///
/// This function blocks until playback finishes. Call from a background
/// thread to avoid blocking the UI.
pub fn play_audio_data(audio: &AudioData) -> Result<(), TtsError> {
    use std::io::Write;

    // Write to a temp file.
    let cache_dir = hive_core::config::HiveConfig::base_dir()
        .map(|d| d.join("tts_cache"))
        .unwrap_or_else(|_| std::env::temp_dir().join("hive_tts"));
    let _ = std::fs::create_dir_all(&cache_dir);

    let file_name = format!("playback_{}.{}", std::process::id(), audio.format.extension());
    let file_path = cache_dir.join(&file_name);

    {
        let mut f = std::fs::File::create(&file_path).map_err(|e| {
            TtsError::Other(format!("Failed to create temp audio file: {e}"))
        })?;
        f.write_all(&audio.bytes).map_err(|e| {
            TtsError::Other(format!("Failed to write audio data: {e}"))
        })?;
    }

    let path_str = file_path.to_string_lossy().to_string();
    tracing::debug!("Playing TTS audio: {path_str}");

    let result = play_file_platform(&path_str, &audio.format);

    // Clean up temp file (best-effort).
    let _ = std::fs::remove_file(&file_path);

    result
}

#[cfg(target_os = "windows")]
fn play_file_platform(path: &str, format: &AudioFormat) -> Result<(), TtsError> {
    // For WAV files, use SoundPlayer which blocks until done.
    // For other formats, use Start-Process with the default media handler.
    let status = match format {
        AudioFormat::Wav => std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(New-Object Media.SoundPlayer '{}').PlaySync()",
                    path.replace('\'', "''")
                ),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
        _ => {
            // For mp3/opus/etc., use Windows Media Player via wmplayer or
            // ffplay if available; fall back to Start-Process.
            std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Add-Type -AssemblyName presentationCore; \
                         $p = New-Object System.Windows.Media.MediaPlayer; \
                         $p.Open([Uri]'{}'); \
                         $p.Play(); \
                         Start-Sleep -Milliseconds 500; \
                         while ($p.Position -lt $p.NaturalDuration.TimeSpan) {{ Start-Sleep -Milliseconds 200 }}; \
                         $p.Close()",
                        path.replace('\'', "''")
                    ),
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
        }
    };

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(TtsError::Other(format!("Audio player exited with {s}"))),
        Err(e) => Err(TtsError::Other(format!("Failed to launch audio player: {e}"))),
    }
}

#[cfg(target_os = "macos")]
fn play_file_platform(path: &str, _format: &AudioFormat) -> Result<(), TtsError> {
    let status = std::process::Command::new("afplay")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(TtsError::Other(format!("afplay exited with {s}"))),
        Err(e) => Err(TtsError::Other(format!("Failed to run afplay: {e}"))),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn play_file_platform(path: &str, format: &AudioFormat) -> Result<(), TtsError> {
    // Try aplay for WAV, then ffplay, then paplay.
    let cmd = match format {
        AudioFormat::Wav => ("aplay", vec![path.to_string()]),
        _ => ("ffplay", vec!["-nodisp".into(), "-autoexit".into(), path.to_string()]),
    };
    let status = std::process::Command::new(cmd.0)
        .args(&cmd.1)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(TtsError::Other(format!("{} exited with {s}", cmd.0))),
        Err(e) => Err(TtsError::Other(format!("Failed to run {}: {e}", cmd.0))),
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Unified interface for all TTS backends (cloud and local).
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Which kind of provider this is.
    fn provider_type(&self) -> TtsProviderType;

    /// Human-readable display name.
    fn name(&self) -> &str;

    /// Quick health-check (e.g. ping the API or check for a local model).
    async fn is_available(&self) -> bool;

    /// List voices the provider currently exposes.
    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, TtsError>;

    /// Synthesise speech from text.
    async fn synthesize(&self, request: &TtsRequest) -> Result<AudioData, TtsError>;

    /// Whether this provider supports voice cloning.
    fn supports_cloning(&self) -> bool;

    /// Clone a voice from reference audio samples.
    /// Returns the new voice's info (including its new ID).
    async fn clone_voice(&self, _name: &str, _samples: &[Vec<u8>]) -> Result<VoiceInfo, TtsError> {
        Err(TtsError::CloningNotSupported)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_type_from_str_loose() {
        assert_eq!(
            TtsProviderType::from_str_loose("elevenlabs"),
            Some(TtsProviderType::ElevenLabs)
        );
        assert_eq!(
            TtsProviderType::from_str_loose("openai"),
            Some(TtsProviderType::OpenAi)
        );
        assert_eq!(
            TtsProviderType::from_str_loose("qwen3"),
            Some(TtsProviderType::Qwen3)
        );
        assert_eq!(
            TtsProviderType::from_str_loose("f5"),
            Some(TtsProviderType::F5Tts)
        );
        assert_eq!(
            TtsProviderType::from_str_loose("telnyx"),
            Some(TtsProviderType::Telnyx)
        );
        assert_eq!(
            TtsProviderType::from_str_loose("ELEVENLABS"),
            Some(TtsProviderType::ElevenLabs)
        );
        assert_eq!(TtsProviderType::from_str_loose("unknown"), None);
    }

    #[test]
    fn provider_type_round_trip() {
        for ty in [
            TtsProviderType::ElevenLabs,
            TtsProviderType::OpenAi,
            TtsProviderType::Qwen3,
            TtsProviderType::F5Tts,
            TtsProviderType::Telnyx,
        ] {
            assert_eq!(TtsProviderType::from_str_loose(ty.as_str()), Some(ty));
        }
    }

    #[test]
    fn audio_format_content_type() {
        assert_eq!(AudioFormat::Wav.content_type(), "audio/wav");
        assert_eq!(AudioFormat::Mp3.content_type(), "audio/mpeg");
        assert_eq!(AudioFormat::Opus.content_type(), "audio/opus");
    }

    #[test]
    fn audio_format_extension() {
        assert_eq!(AudioFormat::Wav.extension(), "wav");
        assert_eq!(AudioFormat::Mp3.extension(), "mp3");
    }

    #[test]
    fn tts_request_defaults() {
        let req = TtsRequest::new("hello", "voice-1");
        assert_eq!(req.text, "hello");
        assert_eq!(req.voice_id, "voice-1");
        assert!((req.speed - 1.0).abs() < f32::EPSILON);
        assert_eq!(req.format, AudioFormat::Mp3);
    }

    #[test]
    fn tts_request_speed_clamped() {
        let req = TtsRequest::new("test", "v").with_speed(10.0);
        assert!((req.speed - 4.0).abs() < f32::EPSILON);

        let req = TtsRequest::new("test", "v").with_speed(0.01);
        assert!((req.speed - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn tts_provider_type_serde_round_trip() {
        for ty in [
            TtsProviderType::ElevenLabs,
            TtsProviderType::OpenAi,
            TtsProviderType::Qwen3,
            TtsProviderType::F5Tts,
            TtsProviderType::Telnyx,
        ] {
            let json = serde_json::to_string(&ty).unwrap();
            let parsed: TtsProviderType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, ty);
        }
    }

    #[test]
    fn voice_info_serde_round_trip() {
        let info = VoiceInfo {
            id: "v-1".into(),
            name: "Test Voice".into(),
            language: Some("en".into()),
            preview_url: None,
            is_cloned: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: VoiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "v-1");
        assert_eq!(parsed.name, "Test Voice");
        assert!(!parsed.is_cloned);
    }
}
