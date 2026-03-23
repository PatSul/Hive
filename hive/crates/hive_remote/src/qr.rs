use anyhow::{Result, anyhow};
use qrcode::QrCode;
use qrcode::render::svg;
use serde::{Deserialize, Serialize};

/// Payload embedded in a QR code for device pairing.
///
/// Serialized as a `hive://pair?...` URL so that mobile apps
/// can scan and immediately start the key-exchange handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingQrPayload {
    pub session_id: String,
    pub public_key_b64: String,
    pub lan_addr: Option<String>,
    pub relay_url: Option<String>,
    pub version: u8,
}

impl PairingQrPayload {
    /// Encode the payload as a `hive://pair?...` URL.
    pub fn to_url(&self) -> String {
        let mut params = vec![
            format!("id={}", urlenc(&self.session_id)),
            format!("pk={}", urlenc(&self.public_key_b64)),
        ];
        if let Some(ref addr) = self.lan_addr {
            params.push(format!("addr={}", urlenc(addr)));
        }
        if let Some(ref relay) = self.relay_url {
            params.push(format!("relay={}", urlenc(relay)));
        }
        params.push(format!("v={}", self.version));
        format!("hive://pair?{}", params.join("&"))
    }

    /// Parse a `hive://pair?...` URL back into a payload.
    pub fn from_url(url: &str) -> Result<Self> {
        let query = url
            .strip_prefix("hive://pair?")
            .ok_or_else(|| anyhow!("Invalid hive pairing URL"))?;

        let mut id = None;
        let mut pk = None;
        let mut addr = None;
        let mut relay = None;
        let mut version = 1u8;

        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let val = parts.next().unwrap_or("");
            let decoded = urldec(val);
            match key {
                "id" => id = Some(decoded),
                "pk" => pk = Some(decoded),
                "addr" => addr = Some(decoded),
                "relay" => relay = Some(decoded),
                "v" => version = decoded.parse().unwrap_or(1),
                _ => {}
            }
        }

        Ok(Self {
            session_id: id.ok_or_else(|| anyhow!("Missing session id"))?,
            public_key_b64: pk.ok_or_else(|| anyhow!("Missing public key"))?,
            lan_addr: addr,
            relay_url: relay,
            version,
        })
    }
}

/// Generate an SVG QR code from a [`PairingQrPayload`].
///
/// Returns the SVG markup as a `String`.
pub fn generate_pairing_qr(payload: &PairingQrPayload) -> Result<String> {
    let url = payload.to_url();
    let code = QrCode::new(url.as_bytes()).map_err(|e| anyhow!("QR generation failed: {}", e))?;
    let svg_string = code.render::<svg::Color>().min_dimensions(200, 200).build();
    Ok(svg_string)
}

// ---------------------------------------------------------------------------
// Minimal percent-encoding helpers (no extra dependency needed)
// ---------------------------------------------------------------------------

/// Percent-encode a string, leaving unreserved characters untouched
/// (RFC 3986 unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~").
fn urlenc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(hex_digit(b >> 4));
                out.push(hex_digit(b & 0x0F));
            }
        }
    }
    out
}

/// Decode a percent-encoded string.
fn urldec(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'A' + n - 10) as char,
        _ => '0',
    }
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
