use super::*;

#[test]
fn undeclared_or_invalid_responses_capabilities_are_rejected_without_rewriting_files() {
    let (_directory, store) = store();
    let created = store
        .create_provider(codex_draft("Responses capabilities"))
        .unwrap();
    let path = file_for(&store, AppKind::Codex, &created.profile.id);
    let current: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for options in [
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!({"supportsWebsockets":false,"requestMode":"standard"})),
        Some(serde_json::json!({"supportsWebsockets":true,"requestMode":"minimal"})),
        Some(serde_json::json!({"supportsWebsockets":false,"requestMode":"compatibility"})),
    ] {
        let mut value = current.clone();
        match options {
            Some(options) => {
                value["responsesOptions"] = options;
            }
            None => {
                value.as_object_mut().unwrap().remove("responsesOptions");
            }
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn legacy_usage_query_files_are_rejected_without_a_write() {
    let (directory, store) = store();
    let mut draft = codex_draft("旧查询档案");
    draft.usage_query = Some(UsageQuery::Declarative {
        url: "{{baseUrl}}/usage".to_string(),
        remaining_path: Some("/remaining".to_string()),
        used_path: None,
        total_path: None,
        unit: None,
        refresh_interval_minutes: 30,
    });
    store.create_provider(draft).expect("create provider");
    let path = provider_files(&store, AppKind::Codex).remove(0);

    // Rewind the stored file to the previous contract: the interval line
    // simply did not exist.
    let current = fs::read_to_string(&path).unwrap();
    let legacy = current
        .replace("  \"refreshIntervalMinutes\": 30,\n", "")
        .replace(",\n    \"refreshIntervalMinutes\": 30", "");
    assert_ne!(legacy, current, "fixture must remove the interval field");
    fs::write(&path, &legacy).unwrap();

    assert!(matches!(
        store.list_providers().expect_err("legacy file must fail"),
        ProfileStoreError::Unsupported
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
    drop(directory);
}

#[test]
fn unknown_usage_query_shapes_stay_rejected_without_a_write() {
    let (directory, store) = store();
    let mut draft = codex_draft("坏查询档案");
    draft.usage_query = Some(UsageQuery::Script {
        source: "({ request() {}, extract() {} })".to_string(),
        refresh_interval_minutes: 0,
    });
    store.create_provider(draft).expect("create provider");
    let path = provider_files(&store, AppKind::Codex).remove(0);

    let current = fs::read_to_string(&path).unwrap();
    // An unknown key is not the previous shape; the upgrade must refuse
    // to touch the file and the load must fail loudly.
    let unknown = current.replace("\"refreshIntervalMinutes\": 0", "\"legacyTimer\": true");
    assert_ne!(unknown, current);
    fs::write(&path, &unknown).unwrap();

    assert!(store.list_providers().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), unknown);
    drop(directory);
}

#[test]
fn legacy_authentication_field_is_rejected_without_rewriting_the_provider() {
    let (_directory, store) = store();
    let created = store
        .create_provider(codex_draft("旧认证字段"))
        .expect("create provider");
    let path = file_for(&store, AppKind::Codex, &created.profile.id);
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("provider file"))
            .expect("provider JSON");
    value.as_object_mut().expect("provider object").insert(
        "authScheme".to_string(),
        serde_json::Value::String("xApiKey".to_string()),
    );
    fs::write(
        &path,
        serde_json::to_string_pretty(&value).expect("legacy JSON"),
    )
    .expect("write legacy provider");

    let before = fs::read_to_string(&path).unwrap();
    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    assert_eq!(fs::read_to_string(path).unwrap(), before);
}

#[test]
fn invalid_legacy_provider_is_not_rewritten_during_migration() {
    let (_directory, store) = store();
    let created = store
        .create_provider(codex_draft("无效旧认证字段"))
        .expect("create provider");
    let path = file_for(&store, AppKind::Codex, &created.profile.id);
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("provider file"))
            .expect("provider JSON");
    let provider = value.as_object_mut().expect("provider object");
    provider.insert(
        "upstreamProtocol".to_string(),
        serde_json::Value::String("anthropicMessages".to_string()),
    );
    provider.insert(
        "authScheme".to_string(),
        serde_json::Value::String("xApiKey".to_string()),
    );
    let legacy = serde_json::to_string_pretty(&value).expect("legacy JSON");
    fs::write(&path, &legacy).expect("write legacy provider");

    assert_eq!(store.list_providers(), Err(ProfileStoreError::Unsupported));
    assert_eq!(fs::read_to_string(path).expect("legacy provider"), legacy);
}
