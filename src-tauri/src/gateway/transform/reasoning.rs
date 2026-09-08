use super::TransformError;
use aes_gcm::aead::{
    rand_core::{OsRng, RngCore},
    Aead, KeyInit, Payload,
};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

const CONTINUATION_PREFIX: &str = "asb-reasoning-v2.";
const AAD: &[u8] = b"agent-switchboard/reasoning-continuation/v2";
const NONCE_BYTES: usize = 12;

/// The gateway-local representation of one upstream reasoning trace. The
/// cleartext is only used while rendering a Chat Completions request; the
/// opaque continuation is the only form sent to Codex or Claude Code.
#[derive(Clone)]
pub(crate) struct Reasoning {
    pub(crate) content: String,
    pub(crate) continuation: String,
}

/// Uses a backend-bound continuation key independent of access capabilities.
#[derive(Clone)]
pub(crate) struct ReasoningTransport {
    key: [u8; 32],
    continuation_key: [u8; 32],
}

impl ReasoningTransport {
    pub(crate) fn from_continuation_key(continuation_key: [u8; 32]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"agent-switchboard/reasoning-key/v2:");
        hasher.update(continuation_key);
        Self {
            key: hasher.finalize().into(),
            continuation_key,
        }
    }

    pub(crate) fn continuation_key(&self) -> &[u8; 32] {
        &self.continuation_key
    }

    pub(crate) fn from_chat_content(&self, content: String) -> Result<Reasoning, TransformError> {
        if content.is_empty() {
            return Err(TransformError("推理内容不能为空".to_string()));
        }
        Ok(Reasoning {
            continuation: self.seal(content.as_bytes())?,
            content,
        })
    }

    pub(crate) fn from_continuation(
        &self,
        continuation: String,
    ) -> Result<Reasoning, TransformError> {
        let content = String::from_utf8(self.open(&continuation)?)
            .map_err(|_| TransformError("推理续接载荷不是 UTF-8 文本".to_string()))?;
        if content.is_empty() {
            return Err(TransformError("推理续接载荷不含内容".to_string()));
        }
        Ok(Reasoning {
            content,
            continuation,
        })
    }

    fn seal(&self, content: &[u8]) -> Result<String, TransformError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| TransformError("无法初始化推理续接加密器".to_string()))?;
        let mut nonce = [0_u8; NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce);
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: content,
                    aad: AAD,
                },
            )
            .map_err(|_| TransformError("无法加密推理续接内容".to_string()))?;
        let mut payload = Vec::with_capacity(nonce.len() + encrypted.len());
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&encrypted);
        Ok(format!(
            "{CONTINUATION_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(payload)
        ))
    }

    fn open(&self, continuation: &str) -> Result<Vec<u8>, TransformError> {
        let encoded = continuation
            .strip_prefix(CONTINUATION_PREFIX)
            .ok_or_else(|| TransformError("推理续接载荷不属于当前本机网关".to_string()))?;
        let payload = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| {
            TransformError("推理续接载荷格式无效；请从同一供应商路由继续会话".to_string())
        })?;
        let (nonce, encrypted) = payload.split_at_checked(NONCE_BYTES).ok_or_else(|| {
            TransformError("推理续接载荷格式无效；请从同一供应商路由继续会话".to_string())
        })?;
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| TransformError("无法初始化推理续接解密器".to_string()))?;
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: encrypted,
                    aad: AAD,
                },
            )
            .map_err(|_| TransformError("推理续接载荷无效；请从同一供应商路由继续会话".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_round_trips_without_exposing_reasoning() {
        let transport = ReasoningTransport::from_continuation_key([1; 32]);
        let reasoning = transport
            .from_chat_content("private chain of thought".to_string())
            .expect("encrypt reasoning");

        assert!(!reasoning.continuation.contains(&reasoning.content));
        assert_eq!(
            transport
                .from_continuation(reasoning.continuation)
                .expect("decrypt reasoning")
                .content,
            "private chain of thought"
        );
    }

    #[test]
    fn continuation_is_bound_to_its_loopback_route() {
        let first = ReasoningTransport::from_continuation_key([2; 32]);
        let second = ReasoningTransport::from_continuation_key([3; 32]);
        let reasoning = first
            .from_chat_content("private chain of thought".to_string())
            .expect("encrypt reasoning");

        assert!(second.from_continuation(reasoning.continuation).is_err());
    }
}
