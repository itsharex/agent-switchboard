use super::*;
use crate::extensions::contracts::{
    McpDefinition, SecretValue, SkillDefinition, SkillDependency, SkillManifest,
};
use crate::extensions::portable::package::{base64_decode, base64_encode};
use crate::extensions::skill::ContentEntry;
use crate::extensions::validate::ContentEntryKind;
use std::collections::BTreeMap;

fn content() -> Vec<ContentEntry> {
    vec![ContentEntry {
        relative_path: "SKILL.md".to_string(),
        kind: ContentEntryKind::File,
        bytes: b"---\nname: docs\n---\nbody".to_vec(),
        mode: 0o644,
    }]
}

fn skill_definition() -> SkillDefinition {
    SkillDefinition {
        content_digest: "a".repeat(64),
        manifest: SkillManifest {
            name: "docs".to_string(),
            description: Some("desc".to_string()),
            license: None,
            allowed_tools: None,
            unparsed_keys: Vec::new(),
        },
        source: None,
        host_scoped: None,
        compatibility: Vec::new(),
        dependencies: vec![SkillDependency {
            name: "docs".to_string(),
            resource_id: None,
        }],
    }
}

#[test]
fn skill_packages_round_trip_through_text_and_base64() {
    let mut entries = content();
    entries.push(ContentEntry {
        relative_path: "assets/logo.png".to_string(),
        kind: ContentEntryKind::File,
        bytes: vec![0x89, 0x50, 0x4e, 0x47, 0x0d],
        mode: 0o644,
    });
    let package = export_skill(&skill_definition(), &entries).unwrap();
    let json = serde_json::to_string(&package).unwrap();
    // No credential material or host paths exist in a skill package.
    assert!(!json.contains("secret-"));
    let parsed: PortablePackage = serde_json::from_str(&json).unwrap();
    let ImportMaterial::Skill {
        manifest,
        dependencies,
        entries,
        ..
    } = prepare_import(&parsed).unwrap()
    else {
        panic!("expected skill material");
    };
    assert_eq!(manifest.name, "docs");
    assert_eq!(dependencies, vec!["docs".to_string()]);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].bytes, vec![0x89, 0x50, 0x4e, 0x47, 0x0d]);
}

#[test]
fn stdio_mcp_exports_slot_names_without_values() {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        SecretValue::SecretRef {
            reference: "secret-keep".to_string(),
        },
    );
    env.insert(
        "REGION".to_string(),
        SecretValue::Plain {
            value: "us-east".to_string(),
        },
    );
    let mcp = McpDefinition::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "srv".to_string()],
        env,
        codex_options: None,
    };
    let package = export_mcp("docs", &mcp).unwrap();
    let json = serde_json::to_string(&package).unwrap();
    assert!(json.contains("TOKEN"));
    assert!(!json.contains("secret-keep"));
    assert!(!json.contains("us-east"));
    let ImportMaterial::McpStdio {
        name,
        definition,
        missing_env_slots,
    } = prepare_import(&package).unwrap()
    else {
        panic!("expected mcp material");
    };
    assert_eq!(name, "docs");
    match definition {
        McpDefinition::Stdio { env, command, .. } => {
            assert!(env.is_empty());
            assert_eq!(command, "npx");
        }
        other => panic!("unexpected variant {other:?}"),
    }
    assert_eq!(
        missing_env_slots,
        vec!["REGION".to_string(), "TOKEN".to_string()]
    );
}

#[test]
fn remote_mcps_refuse_export() {
    let mcp = McpDefinition::Http {
        url: "https://mcp.example.test/v1".to_string(),
        headers: BTreeMap::new(),
        bearer: None,
    };
    let error = export_mcp("docs", &mcp).unwrap_err();
    assert!(error.message.contains("不属于可移植内容"));
}

#[test]
fn traversal_and_bad_shapes_are_rejected_on_import() {
    let mut package = export_skill(&skill_definition(), &content()).unwrap();
    let PortablePayload::Skill { files, .. } = &mut package.payload else {
        panic!();
    };
    files[0].path = "../escape/SKILL.md".to_string();
    let error = prepare_import(&package).unwrap_err();
    assert!(error.message.contains("非法路径段"));

    let mut absolute = export_skill(&skill_definition(), &content()).unwrap();
    let PortablePayload::Skill { files, .. } = &mut absolute.payload else {
        panic!();
    };
    files[0].path = "C:/tmp/SKILL.md".to_string();
    assert!(prepare_import(&absolute).is_err());

    let mut versioned = export_skill(&skill_definition(), &content()).unwrap();
    versioned.schema_version = 99;
    assert!(prepare_import(&versioned).is_err());

    let mut missing = export_skill(&skill_definition(), &content()).unwrap();
    let PortablePayload::Skill { files, .. } = &mut missing.payload else {
        panic!();
    };
    files.clear();
    assert!(prepare_import(&missing).is_err());

    // Unknown payload kinds never parse (a stray top-level key is
    // absorbed by the flattened union, so `kind` is the gate).
    assert!(serde_json::from_str::<PortablePackage>(
        r#"{"schemaVersion":1,"kind":"pluginBundle","name":"x"}"#
    )
    .is_err());
}

#[test]
fn base64_round_trip_is_lossless() {
    for length in 0..=5usize {
        let bytes: Vec<u8> = (0..length as u8).collect();
        let encoded = base64_encode(&bytes);
        assert_eq!(base64_decode(&encoded).unwrap(), bytes, "length {length}");
    }
    let payload = vec![0xff, 0x00, 0xab, 0xcd, 0xef, 0x12];
    assert_eq!(base64_decode(&base64_encode(&payload)).unwrap(), payload);
}
