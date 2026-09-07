use super::{EncryptedBackup, AAD, ENCRYPTION_VERSION, KEY_LENGTH, NONCE_LENGTH, SALT_LENGTH};
use crate::config_store::snapshot::ConfigurationSnapshot;
use aes_gcm::aead::{
    rand_core::{OsRng, RngCore},
    Aead, KeyInit, Payload,
};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};

pub(super) fn encrypt(
    snapshot: &ConfigurationSnapshot,
    password: &str,
) -> Result<EncryptedBackup, String> {
    let mut cleartext =
        serde_json::to_vec(snapshot).map_err(|_| "配置快照序列化失败".to_string())?;
    encrypt_cleartext(&mut cleartext, password)
}

pub(super) fn encrypt_cleartext(
    cleartext: &mut [u8],
    password: &str,
) -> Result<EncryptedBackup, String> {
    let mut salt = [0u8; SALT_LENGTH];
    let mut nonce = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let mut key = match derive_key(password, &salt) {
        Ok(key) => key,
        Err(error) => {
            cleartext.fill(0);
            return Err(error);
        }
    };
    let cipher = match Aes256Gcm::new_from_slice(&key) {
        Ok(cipher) => cipher,
        Err(_) => {
            key.fill(0);
            cleartext.fill(0);
            return Err("无法初始化备份加密器".to_string());
        }
    };
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: cleartext,
                aad: AAD,
            },
        )
        .map_err(|_| "无法加密云端备份".to_string());
    key.fill(0);
    cleartext.fill(0);
    let ciphertext = encrypted?;
    Ok(EncryptedBackup {
        version: ENCRYPTION_VERSION,
        salt: STANDARD_NO_PAD.encode(salt),
        nonce: STANDARD_NO_PAD.encode(nonce),
        ciphertext: STANDARD_NO_PAD.encode(ciphertext),
    })
}

pub(super) fn decrypt(payload: &EncryptedBackup, password: &str) -> Result<Vec<u8>, String> {
    if payload.version != ENCRYPTION_VERSION {
        return Err("云端备份加密版本不受支持".to_string());
    }
    let salt = decode_fixed(&payload.salt, SALT_LENGTH, "云端备份数据无效")?;
    let nonce = decode_fixed(&payload.nonce, NONCE_LENGTH, "云端备份数据无效")?;
    let ciphertext = STANDARD_NO_PAD
        .decode(&payload.ciphertext)
        .map_err(|_| "云端备份数据无效".to_string())?;
    let mut key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "无法初始化备份解密器".to_string())?;
    let cleartext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: AAD,
            },
        )
        .map_err(|_| "备份密码不正确或云端备份已损坏".to_string());
    key.fill(0);
    cleartext
}

fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LENGTH], String> {
    let params = Params::new(19 * 1024, 2, 1, Some(KEY_LENGTH))
        .map_err(|_| "无法配置备份密钥派生".to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LENGTH];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| "无法派生备份加密密钥".to_string())?;
    Ok(key)
}

fn decode_fixed(value: &str, expected_length: usize, message: &str) -> Result<Vec<u8>, String> {
    let decoded = STANDARD_NO_PAD
        .decode(value)
        .map_err(|_| message.to_string())?;
    if decoded.len() == expected_length {
        Ok(decoded)
    } else {
        Err(message.to_string())
    }
}
