use super::*;

#[test]
fn model_usage_report_serializes_as_a_read_only_client_model_summary() {
    let report = ModelUsageReport {
        range: ModelUsageRange::Last7Days,
        generated_at: "2026-09-03T08:00:00Z".to_string(),
        groups: vec![ModelUsageGroup {
            app: AppKind::Codex,
            model: Some("gpt-5.6-codex".to_string()),
            input_tokens: 120,
            cache_read_input_tokens: 40,
            cache_creation_input_tokens: 20,
            output_tokens: 30,
            total_tokens: 210,
            session_count: 2,
        }],
        days: vec![ModelUsageDay {
            date: "2026-09-03".to_string(),
            input_tokens: 120,
            cache_read_input_tokens: 40,
            cache_creation_input_tokens: 20,
            output_tokens: 30,
            total_tokens: 210,
        }],
        unassigned_tokens: ModelUsageTokens::default(),
        issues: vec![ModelUsageIssue {
            app: AppKind::Claude,
            message: "当前客户端未提供可解析的 token 记录".to_string(),
        }],
    };

    let value = serde_json::to_value(&report).expect("report serializes");
    assert_eq!(value["range"], "last7Days");
    assert_eq!(value["groups"][0]["cacheReadInputTokens"], 40);
    assert_eq!(value["days"][0]["date"], "2026-09-03");
    assert_eq!(value["unassignedTokens"]["totalTokens"], 0);
    assert_eq!(value["issues"][0]["app"], "claude");
}

#[test]
fn model_usage_read_contract_requires_an_explicit_cache_policy() {
    let request = ModelUsageRequest {
        range: ModelUsageRange::Last7Days,
        force_refresh: true,
    };
    assert_eq!(
        serde_json::to_value(request).expect("request serializes"),
        serde_json::json!({"range":"last7Days","forceRefresh":true})
    );
    assert!(
        serde_json::from_value::<ModelUsageRequest>(serde_json::json!({
            "range":"today"
        }))
        .is_err()
    );

    let read = ModelUsageRead {
        report: ModelUsageReport {
            range: ModelUsageRange::Today,
            generated_at: "2026-09-04T08:00:00Z".to_string(),
            groups: Vec::new(),
            days: Vec::new(),
            unassigned_tokens: ModelUsageTokens::default(),
            issues: Vec::new(),
        },
        freshness: ModelUsageFreshness::Cached,
        refresh_after: "2026-09-04T08:05:00Z".to_string(),
        cache_warning: None,
    };
    let value = serde_json::to_value(read).expect("read serializes");
    assert_eq!(value["freshness"], "cached");
    assert_eq!(value["refreshAfter"], "2026-09-04T08:05:00Z");
    assert!(value.get("cacheWarning").is_some());
}

#[test]
fn usage_history_contract_exposes_only_the_requested_scope_and_series_points() {
    let request = UsageHistoryRequest::Provider {
        profile_id: "profile-1".to_string(),
    };
    let series = UsageHistorySeries {
        id: "provider-123".to_string(),
        label: "默认方案余额".to_string(),
        unit: Some("次".to_string()),
        metric: UsageHistoryMetric::Remaining,
        points: vec![UsageHistoryPoint {
            at: "2026-09-03T08:00:00.000Z".to_string(),
            value: 12.5,
        }],
    };

    assert_eq!(
        serde_json::to_value(request).expect("request serializes"),
        serde_json::json!({"kind":"provider","profileId":"profile-1"})
    );
    let value = serde_json::to_value(series).expect("series serializes");
    assert_eq!(value["metric"], "remaining");
    assert_eq!(value["points"][0]["value"], 12.5);
}

#[test]
fn profiles_missing_the_current_protocol_contract_are_rejected() {
    let current = r#"{
            "id": "p1", "app": "codex", "routeMode": "custom", "name": "中转",
            "model": null, "baseUrl": "https://relay.example/v1", "apiKey": "sk-x",
            "notes": null, "websiteUrl": null
        }"#;
    assert!(serde_json::from_str::<ProviderProfile>(current).is_err());
}

#[test]
fn usage_query_round_trips_as_the_only_tagged_contract() {
    let json = r#"{
            "kind": "declarative",
            "url": "{{baseUrl}}/user/balance",
            "remainingPath": "data/balance",
            "totalPath": "data/total",
            "unit": "USD",
            "refreshIntervalMinutes": 0
        }"#;
    let query: UsageQuery = serde_json::from_str(json).expect("usage query");
    assert!(matches!(
        query,
        UsageQuery::Declarative {
            remaining_path: Some(ref path),
            ..
        } if path == "data/balance"
    ));
    assert!(serde_json::from_str::<UsageQuery>(r#"{"url":"u","remainingPath":"x"}"#).is_err());
    assert!(
        serde_json::from_str::<UsageQuery>(r#"{"kind":"declarative","url":"u","bogus":1}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<UsageQuery>(r#"{"kind":"script","source":"({})","url":"u"}"#)
            .is_err()
    );
}

#[test]
fn usage_query_auto_refresh_interval_is_required_and_round_trips() {
    // The interval is a strict required field; upgrading files saved
    // before it existed belongs to the store, not to the contract.
    assert!(serde_json::from_str::<UsageQuery>(
        r#"{"kind":"declarative","url":"u","remainingPath":"x"}"#
    )
    .is_err());
    assert!(serde_json::from_str::<UsageQuery>(r#"{"kind":"script","source":"({})"}"#).is_err());

    let timed: UsageQuery =
        serde_json::from_str(r#"{"kind":"script","source":"({})","refreshIntervalMinutes":5}"#)
            .expect("timed usage query");
    assert_eq!(timed.refresh_interval_minutes(), 5);
    assert_eq!(
        serde_json::to_value(&timed).expect("serialized"),
        serde_json::json!({"kind":"script","source":"({})","refreshIntervalMinutes":5})
    );
}

#[test]
fn provider_files_carry_no_client_field_and_round_trip_through_profiles() {
    let profile = ProviderProfile {
        id: "0b91a2f4-6c85-4a12-9f0d-2f4a1b3c5d6e".into(),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "中继".into(),
        model: Some("claude-opus-4".into()),
        base_url: Some("https://relay.example".into()),
        api_key: "test-api-key".into(),
        upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    };
    let file = ProviderFile::from_profile(&profile, 300);
    let text = serde_json::to_string(&file).expect("provider file serializes");
    assert!(!text.contains("\"app\""));
    assert!(text.contains("\"websiteUrl\":null"));
    let parsed: ProviderFile = serde_json::from_str(&text).expect("provider file parses");
    assert_eq!(parsed.position, 300);
    assert_eq!(parsed.into_profile(AppKind::Claude), profile);
}

#[test]
fn provider_file_requires_the_explicit_output_limit_field() {
    let legacy = r#"{
            "id":"0b91a2f4-6c85-4a12-9f0d-2f4a1b3c5d6e",
            "name":"官方登录",
            "position":100,
            "routeMode":"official",
            "apiKey":"",
            "upstreamProtocol":null,
            "baseUrl":null,
            "model":null,
            "websiteUrl":null
        }"#;
    assert!(serde_json::from_str::<ProviderFile>(legacy).is_err());

    let current = legacy.replacen(
        "\"websiteUrl\":null",
        "\"maxOutputTokens\":null,\n            \"websiteUrl\":null",
        1,
    );
    assert!(serde_json::from_str::<ProviderFile>(&current).is_ok());

    let obsolete = current.replacen(
        "\"baseUrl\":null",
        "\"authScheme\":\"bearer\",\n            \"baseUrl\":null",
        1,
    );
    assert!(serde_json::from_str::<ProviderFile>(&obsolete).is_err());
}

#[test]
fn codex_model_settings_reject_unknown_persisted_members() {
    assert!(serde_json::from_str::<ProviderFile>(
        r#"{
                "id":"0b91a2f4-6c85-4a12-9f0d-2f4a1b3c5d6e",
                "name":"中继",
                "position":100,
                "apiKey":"test-api-key",
                "baseUrl":"https://relay.example",
                "model":null,
                "modelOptions":{"kind":"codex","contextWindow":128000,"legacyField":true}
            }"#
    )
    .is_err());
}

#[test]
fn common_settings_store_automatic_or_explicit_values_only() {
    let json = r#"{
            "settings": {
                "model_reasoning_effort": { "mode": "explicit", "value": "high" },
                "hide_agent_reasoning": { "mode": "automatic" }
            }
        }"#;
    let settings: CommonSettings = serde_json::from_str(json).expect("common settings");
    assert_eq!(
        settings.value("model_reasoning_effort"),
        Some(&CommonSettingValue::Explicit {
            value: ConfigValue::Str("high".into())
        })
    );
    assert_eq!(
        settings.value("hide_agent_reasoning"),
        Some(&CommonSettingValue::Automatic)
    );
    // The file shape is fixed: no extra members are accepted.
    assert!(serde_json::from_str::<CommonSettings>(r#"{ "settings": {}, "extra": 1 }"#).is_err());
}

fn classification_draft() -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: "中转".to_string(),
        base_url: Some("https://relay.example/v1".to_string()),
        api_key: "sk-test".to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        max_output_tokens: ExplicitMaxOutputTokens::none(),
        model: Some("gpt-test".to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

#[test]
fn profile_save_classification_follows_metadata_and_active_identity() {
    let profile = ProviderProfile::from_draft("p1".into(), classification_draft());

    // Unchanged draft: nothing to write, regardless of active identity.
    assert_eq!(
        classify_profile_save(Some(&profile), &classification_draft(), true),
        ProfileSaveKind::NoChange
    );

    // Metadata-only edits never require an apply, even when active.
    let mut metadata = classification_draft();
    metadata.name = "新名称".into();
    metadata.notes = Some("备注".into());
    metadata.website_url = Some("https://example.com".into());
    metadata.usage_query = Some(UsageQuery::Declarative {
        url: "https://relay.example/balance".into(),
        remaining_path: None,
        used_path: None,
        total_path: None,
        unit: None,
        refresh_interval_minutes: 0,
    });
    assert!(!profile.draft_touches_live_configuration(&metadata));
    assert_eq!(
        classify_profile_save(Some(&profile), &metadata, true),
        ProfileSaveKind::SaveOnly
    );

    // Effective edits require an apply only when the provider is active.
    let mut rekeyed = classification_draft();
    rekeyed.api_key = "sk-other".into();
    assert!(profile.draft_touches_live_configuration(&rekeyed));
    assert_eq!(
        classify_profile_save(Some(&profile), &rekeyed, false),
        ProfileSaveKind::SaveOnly
    );
    assert_eq!(
        classify_profile_save(Some(&profile), &rekeyed, true),
        ProfileSaveKind::SaveAndApply
    );

    // New drafts always classify as create.
    assert_eq!(
        classify_profile_save(None, &classification_draft(), false),
        ProfileSaveKind::Create
    );
}

#[test]
fn profile_save_kind_serializes_as_camel_case_tags() {
    assert_eq!(
        serde_json::to_value(ProfileSaveKind::SaveAndApply).expect("kind serializes"),
        serde_json::json!("saveAndApply")
    );
    assert_eq!(
        serde_json::to_value(ProfileSaveKind::NoChange).expect("kind serializes"),
        serde_json::json!("noChange")
    );
}

#[test]
fn official_quota_interval_is_metadata_only_and_survives_storage() {
    let mut draft = classification_draft();
    draft.route_mode = RouteMode::Official;
    draft.model = None;
    draft.base_url = None;
    draft.api_key = String::new();
    draft.upstream_protocol = None;
    let profile = ProviderProfile::from_draft("official-1".into(), draft.clone());
    profile.validate().expect("official codex draft validates");

    // Retiming the quota panel is application-side metadata: it never
    // re-applies the client configuration, even while active.
    let mut retimed = draft;
    retimed.official_quota_refresh_interval_minutes = Some(30);
    assert!(!profile.draft_touches_live_configuration(&retimed));
    assert_eq!(
        classify_profile_save(Some(&profile), &retimed, true),
        ProfileSaveKind::SaveOnly
    );

    let stored = ProviderFile::from_profile(
        &ProviderProfile::from_draft("official-1".into(), retimed),
        0,
    );
    let text = serde_json::to_string(&stored).expect("provider file serializes");
    assert!(text.contains("\"officialQuotaRefreshIntervalMinutes\":30"));
    let parsed: ProviderFile = serde_json::from_str(&text).expect("provider file parses");
    assert_eq!(
        parsed
            .into_profile(AppKind::Codex)
            .official_quota_refresh_interval_minutes,
        Some(30)
    );
}
