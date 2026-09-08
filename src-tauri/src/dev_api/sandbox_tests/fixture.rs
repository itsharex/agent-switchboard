use super::*;

pub(super) struct Sandbox {
    pub(super) root: PathBuf,
    pub(super) home: PathBuf,
    pub(super) codex: PathBuf,
    pub(super) claude: PathBuf,
    pub(super) config: PathBuf,
    pub(super) auth: PathBuf,
    pub(super) settings: PathBuf,
    pub(super) secret: String,
    pub(super) secret_b: String,
    pub(super) oauth: String,
    pub(super) initial_codex: &'static str,
    pub(super) initial_claude: &'static str,
    pub(super) initial_auth: String,
    pub(super) api: Api,
    pub(super) state: crate::local_state::LocalState,
    _app: tauri::App,
}

impl Sandbox {
    pub(super) fn new(root: &Path) -> Self {
        // Only the subprocess environment is redirected; parent/user variables stay intact.
        let home = root.join("home");
        let codex = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or(home.join(".codex"));
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or(home.join(".claude"));
        let config = codex.join("config.toml");
        let auth = codex.join("auth.json");
        let settings = claude.join("settings.json");
        let secret = uuid::Uuid::new_v4().to_string();
        let secret_b = uuid::Uuid::new_v4().to_string();
        let oauth = uuid::Uuid::new_v4().to_string();
        let initial_codex = "# host comment\nmodel_provider = \"openai\"\nmodel = \"host-model\"\n[mcp_servers.audit]\ncommand = \"host-only\" # retain bytes\n";
        let initial_claude =
            "{\"permissions\":{\"deny\":[\"Read(.env)\"]},\"env\":{\"HOST_SETTING\":\"keep\"}}";
        write(&config, initial_codex);
        write(
            &auth,
            &json!({"auth_mode":"chatgpt", "tokens":{"access_token":oauth, "refresh_token":"fixture-refresh", "id_token":"fixture-id"}, "host":"keep"})
                .to_string(),
        );
        write(&settings, initial_claude);
        let initial_auth = read(&auth);
        // Sentinel default files must stay untouched when explicit directories are used.
        if codex != home.join(".codex") {
            write(&home.join(".codex/config.toml"), "default sentinel");
        }
        if claude != home.join(".claude") {
            write(&home.join(".claude/settings.json"), "default sentinel");
        }

        let (app, state, api) = start_api(root);
        Self {
            root: root.to_path_buf(),
            home,
            codex,
            claude,
            config,
            auth,
            settings,
            secret,
            secret_b,
            oauth,
            initial_codex,
            initial_claude,
            initial_auth,
            api,
            state,
            _app: app,
        }
    }
}

fn start_api(root: &Path) -> (tauri::App, crate::local_state::LocalState, Api) {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    // Tauri joins this absolute identifier to app_data_dir: no real app state is used.
    context.config_mut().identifier = root.join("app-data").to_string_lossy().into_owned();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), root.join("app-data"));
    let state = crate::local_state::LocalState::from_app(app.handle()).unwrap();
    state.initialize_schemas().unwrap();
    app.manage(crate::gateway::GatewayController::start(&state));
    app.manage(crate::gateway::PortChangePreparations::default());
    app.manage(crate::commands::ConfigWriteGate::default());
    app.manage(crate::commands::switching::ProfileSavePreparations::default());
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/invoke", server.server_addr());
    let handle = app.handle().clone();
    thread::spawn(move || serve(server, handle, ORIGIN.to_string()));
    let api = Api {
        url,
        client: reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    };

    (app, state, api)
}
