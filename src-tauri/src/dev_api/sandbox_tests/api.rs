use super::*;

pub(super) struct Api {
    pub(super) url: String,
    pub(super) client: reqwest::blocking::Client,
}

impl Api {
    pub(super) fn request(&self, command: &str, args: Value) -> Value {
        let response = self
            .client
            .post(&self.url)
            .header("Origin", ORIGIN)
            .header("Content-Type", "application/json")
            .body(json!({ "command": command, "args": args }).to_string())
            .send()
            .expect("sandbox HTTP request");
        assert_eq!(response.status(), 200);
        serde_json::from_str(&response.text().unwrap()).unwrap()
    }

    pub(super) fn ok(&self, command: &str, args: Value) -> Value {
        let response = self.request(command, args);
        assert_eq!(
            response["kind"], "success",
            "{command}: {}",
            response["error"]
        );
        response["result"].clone()
    }

    pub(super) fn rejected(&self, command: &str, args: Value, code: &str) {
        let response = self.request(command, args);
        assert_eq!(response["kind"], "failure", "{command} must reject");
        assert_eq!(response["error"]["code"], code);
    }

    pub(super) fn save_client(&self, app: &str, key: &str, value: Value) -> Value {
        let editor = self.ok("get_client_settings_editor", json!({ "target": app }));
        let mut settings = editor["settings"].clone();
        settings["settings"][key] = json!({ "mode": "explicit", "value": value });
        self.ok(
            "save_client_settings",
            json!({
                "target": app, "settings": settings, "expectedSettingsHash": editor["settingsHash"]
            }),
        )
    }

    pub(super) fn create(
        &self,
        app: &str,
        name: &str,
        official: bool,
        secret: &str,
        upstream_protocol: &str,
    ) -> Value {
        let catalog = self.ok("get_provider_parameters_catalog", json!({"target": app}));
        let mut parameters = catalog["defaults"].clone();
        let values = parameters["settings"].as_object_mut().unwrap();
        if app == "codex" {
            values.insert(
                "model_reasoning_effort".into(),
                json!({"mode":"explicit","value":"high"}),
            );
            values.insert(
                "web_search".into(),
                json!({"mode":"explicit","value":"live"}),
            );
        } else {
            values.insert(
                "effortLevel".into(),
                json!({"mode":"explicit","value":"high"}),
            );
        }
        let prepared = self.ok("prepare_profile_save", json!({
            "profileId": null,
            "expectedFileHash": null,
            "draft": {
            "parameters": parameters,
            "app": app, "routeMode": if official { "official" } else { "custom" },
            "name": name, "apiKey": if official { "" } else { secret },
            "baseUrl": if official { Value::Null } else { json!(format!("https://{name}.example.com/v1")) },
            "upstreamProtocol": if official { Value::Null } else { json!(upstream_protocol) },
            "responsesOptions": if !official && upstream_protocol == "responses" {
                json!({"requestMode": "standard"})
            } else { Value::Null },
            "maxOutputTokens": if app == "codex" && upstream_protocol == "anthropicMessages" { json!(8192) } else { Value::Null },
            "model": if official { Value::Null } else { json!(format!("{app}-{name}")) }, "websiteUrl": null
        }}));
        assert_eq!(prepared["kind"], "create");
        self.ok(
            "commit_profile_save",
            json!({"preparationId": prepared["preparationId"], "confirmWrite": false}),
        )["profile"]
            .clone()
    }

    pub(super) fn preview(&self, profile: &Value) -> Value {
        self.ok("preview_switch", json!({ "profileId": profile["id"] }))
    }

    pub(super) fn provider_record(&self, profile: &Value) -> Value {
        self.ok("list_profiles", json!({}))
            .as_array()
            .expect("provider records")
            .iter()
            .find(|record| record["profile"]["id"] == profile["id"])
            .expect("created profile record")
            .clone()
    }

    pub(super) fn prepare_edit(&self, record: &Value, draft: Value) -> Value {
        self.ok(
            "prepare_profile_save",
            json!({
                "profileId": record["profile"]["id"],
                "draft": draft,
                "expectedFileHash": record["fileHash"],
            }),
        )
    }

    pub(super) fn commit_profile_save(&self, prepared: &Value, confirm_write: bool) -> Value {
        self.ok(
            "commit_profile_save",
            json!({
                "preparationId": prepared["preparationId"],
                "confirmWrite": confirm_write,
            }),
        )
    }

    pub(super) fn switch_args(profile: &Value, preview: &Value, confirm: bool) -> Value {
        json!({ "profileId": profile["id"], "expectedHash": preview["contentHash"],
            "expectedRenderedHash": preview["renderedHash"], "confirmWrite": confirm })
    }

    pub(super) fn switch(&self, profile: &Value, secret: &str) -> Value {
        let preview = self.preview(profile);
        assert!(
            !preview.to_string().contains(secret),
            "preview leaked a fixture secret"
        );
        let outcome = self.ok("execute_switch", Self::switch_args(profile, &preview, true));
        assert!(
            !outcome.to_string().contains(secret),
            "outcome leaked a fixture secret"
        );
        outcome
    }
}
