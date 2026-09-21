//! Unambiguous display and catalog lookup for decoded document keys.

pub(crate) fn append_key(parent: &str, key: &str) -> String {
    let segment = if !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')) {
        key.to_string()
    } else {
        serde_json::to_string(key).expect("string serialization")
    };
    if parent.is_empty() { segment } else { format!("{parent}.{segment}") }
}

pub(crate) fn from_keys(keys: &[String]) -> String {
    keys.iter().fold(String::new(), |parent, key| append_key(&parent, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_keys_cannot_alias_nested_paths_or_array_indices() {
        assert_eq!(append_key("agents", "enabled"), "agents.enabled");
        assert_eq!(append_key("", "agents.enabled"), "\"agents.enabled\"");
        assert_eq!(append_key("rules", "0"), "rules.0");
        assert_eq!(append_key("", "rules[0]"), "\"rules[0]\"");
        assert_eq!(append_key("", ""), "\"\"");
    }
}
