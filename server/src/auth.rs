//! Who is asking: every request that changes something is signed.
//!
//! A player is a key pair made on their machine. Each change is signed over
//! the method, the path, a timestamp and the body, so a signature cannot be
//! reused for another request or replayed later than five minutes on. The
//! server keeps only the public half.

use axum::http::HeaderMap;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

pub const PLAYER: &str = "x-todora-player";
pub const TIME: &str = "x-todora-time";
pub const SIGNATURE: &str = "x-todora-signature";
/// How far a request's clock may be from the server's.
const SKEW: i64 = 300;

/// What was signed.
pub fn message(method: &str, path: &str, time: i64, body: &[u8]) -> Vec<u8> {
    let mut out = format!("{method} {path}\n{time}\n").into_bytes();
    out.extend_from_slice(body);
    out
}

pub fn key(bytes: &[u8]) -> Result<VerifyingKey, &'static str> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| "a public key is 32 bytes")?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "not a public key")
}

pub fn decode(text: &str) -> Result<Vec<u8>, &'static str> {
    STANDARD.decode(text.trim()).map_err(|_| "not base64")
}

/// Check the signature on a request against `key`.
pub fn check(
    key: &VerifyingKey,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &[u8],
    now: i64,
) -> Result<(), &'static str> {
    let header = |name| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or("unsigned request")
    };
    let time: i64 = header(TIME)?.parse().map_err(|_| "bad time")?;
    if (time - now).abs() > SKEW {
        return Err("request too old or from the future");
    }
    let signature = decode(header(SIGNATURE)?)?;
    let signature = Signature::from_slice(&signature).map_err(|_| "not a signature")?;
    key.verify(&message(method, path, time, body), &signature)
        .map_err(|_| "bad signature")
}

pub fn player(headers: &HeaderMap) -> Option<&str> {
    headers.get(PLAYER).and_then(|v| v.to_str().ok())
}

/// A fresh random id, as hex.
pub fn id() -> String {
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
