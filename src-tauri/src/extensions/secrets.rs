//! Extension secrets in the system credential store.
//!
//! Definitions only ever hold references ([`asb_core::extensions::contracts::SecretValue::SecretRef`]);
//! the concrete values live in the OS credential manager / Keychain /
//! secret service under the `Agent Switchboard` service. Plain reads never
//! return them; only plan preparation resolves them into rendered client
//! documents, which the preview then redacts.

/// The system credential service name for extension secrets.
#[cfg(not(feature = "desktop-e2e"))]
pub const SECRET_SERVICE: &str = "Agent Switchboard";

/// Desktop E2E uses the real OS store with a separate service namespace.
#[cfg(feature = "desktop-e2e")]
pub const SECRET_SERVICE: &str = "Agent Switchboard E2E";

/// Errors from the secret store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("系统凭据存储不可用：{0}")]
    Unavailable(String),
    #[error("凭据引用 {0} 不存在；请在扩展库中重新保存该凭据")]
    Missing(String),
}

/// The credential backend boundary. Production uses the system store;
/// tests inject an in-memory backend so verification never depends on a
/// desktop keychain.
pub trait SecretBackend {
    fn put(&self, reference: &str, value: &str) -> Result<(), SecretError>;
    fn get(&self, reference: &str) -> Result<String, SecretError>;
}

/// The system credential store implementation.
pub struct SystemSecrets;

fn system_entry(reference: &str) -> Result<keyring::Entry, SecretError> {
    // Windows enumeration supports prefix filters only. The E2E namespace
    // lets the external runner find orphaned saves without reading production
    // credential metadata or blobs.
    #[cfg(all(windows, feature = "desktop-e2e"))]
    let entry = keyring::Entry::new_with_target(
        &format!("{SECRET_SERVICE}.{reference}"),
        SECRET_SERVICE,
        reference,
    );
    #[cfg(not(all(windows, feature = "desktop-e2e")))]
    let entry = keyring::Entry::new(SECRET_SERVICE, reference);
    entry.map_err(|error| SecretError::Unavailable(error.to_string()))
}

impl SecretBackend for SystemSecrets {
    fn put(&self, reference: &str, value: &str) -> Result<(), SecretError> {
        let entry = system_entry(reference)?;
        entry
            .set_password(value)
            .map_err(|error| SecretError::Unavailable(error.to_string()))
    }

    fn get(&self, reference: &str) -> Result<String, SecretError> {
        let entry = system_entry(reference)?;
        entry.get_password().map_err(|error| match error {
            keyring::Error::NoEntry => SecretError::Missing(reference.to_string()),
            other => SecretError::Unavailable(other.to_string()),
        })
    }
}

/// A resolver closure for the core render functions.
#[cfg(test)]
pub fn resolver_for<'a>(backend: &'a impl SecretBackend) -> impl Fn(&str) -> Option<String> + 'a {
    move |reference: &str| backend.get(reference).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::extensions::contracts::SecretValue;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    pub struct MemorySecrets {
        entries: Mutex<BTreeMap<String, String>>,
    }

    impl MemorySecrets {
        pub fn new() -> Self {
            Self {
                entries: Mutex::new(BTreeMap::new()),
            }
        }
    }

    impl SecretBackend for MemorySecrets {
        fn put(&self, reference: &str, value: &str) -> Result<(), SecretError> {
            self.entries
                .lock()
                .unwrap()
                .insert(reference.to_string(), value.to_string());
            Ok(())
        }

        fn get(&self, reference: &str) -> Result<String, SecretError> {
            self.entries
                .lock()
                .unwrap()
                .get(reference)
                .cloned()
                .ok_or_else(|| SecretError::Missing(reference.to_string()))
        }
    }

    #[test]
    fn resolver_feeds_core_renders_without_exposing_values() {
        let backend = MemorySecrets::new();
        backend.put("ref-1", "concrete-token").unwrap();
        let resolve = resolver_for(&backend);
        let definition = asb_core::extensions::contracts::McpDefinition::Http {
            url: "https://mcp.example.com".to_string(),
            headers: Default::default(),
            bearer: Some(SecretValue::SecretRef {
                reference: "ref-1".to_string(),
            }),
        };
        let render = asb_core::extensions::mcp::render_claude(&definition, &resolve).unwrap();
        match render {
            asb_core::extensions::mcp::ClaudeServerRender::Http { headers, .. } => {
                assert_eq!(
                    headers.get("Authorization").map(String::as_str),
                    Some("Bearer concrete-token")
                );
            }
            other => panic!("expected http render, got {other:?}"),
        }
    }

    #[test]
    fn missing_references_error_at_plan_time() {
        let backend = MemorySecrets::new();
        let error = backend.get("missing").unwrap_err();
        assert!(matches!(error, SecretError::Missing(_)));
    }
}
