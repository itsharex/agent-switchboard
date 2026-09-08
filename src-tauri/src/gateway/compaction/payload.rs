use crate::gateway::transform::TransformError;
use aes_gcm::aead::{
    rand_core::{OsRng, RngCore},
    Aead, KeyInit, Payload,
};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

const PREFIX: &str = "asb-compaction-v1.";
const DOMAIN: &[u8] = b"agent-switchboard/compaction/v1";

fn cipher(key: &[u8; 32]) -> Aes256Gcm {
    let mut digest = Sha256::new();
    digest.update(DOMAIN);
    digest.update(key);
    Aes256Gcm::new(&digest.finalize())
}

pub(super) fn seal(summary: &str, key: &[u8; 32]) -> Result<String, TransformError> {
    let mut nonce = [0; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher(key)
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: summary.as_bytes(),
                aad: DOMAIN,
            },
        )
        .map_err(|_| TransformError("无法加密压缩摘要".into()))?;
    let mut payload = nonce.to_vec();
    payload.extend(encrypted);
    Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(payload)))
}

pub(super) fn open(content: &str, key: &[u8; 32]) -> Result<String, TransformError> {
    let failure = || TransformError("压缩载荷不属于当前档案和后端；请使用原后端继续会话".into());
    let encoded = content.strip_prefix(PREFIX).ok_or_else(failure)?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| failure())?;
    let (nonce, encrypted) = bytes.split_at_checked(12).ok_or_else(failure)?;
    let clear = cipher(key)
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: encrypted,
                aad: DOMAIN,
            },
        )
        .map_err(|_| failure())?;
    let summary = String::from_utf8(clear).map_err(|_| failure())?;
    if summary.trim().is_empty() {
        return Err(failure());
    }
    Ok(summary)
}
