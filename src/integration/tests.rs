use super::*;
use std::fs;
fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "framelet-integration-test-{}",
        crate::identity::new_id().unwrap()
    ));
    fs::create_dir(&root).unwrap();
    root
}
#[test]
fn toml_inline_and_tables_keep_other_entries_and_comments() {
    let root = root();
    let path = root.join("config.toml");
    let desired = json!({"command":"D:\\folder with spaces\\framelet.exe","args":["mcp"],"env":{"FRAMELET_MCP_ROOT":"D:\\录屏"},"tool_timeout_sec":150});
    for text in [
        "# retain this comment\nmodel = 'selected'\n[mcp_servers.other]\ncommand='other'\n",
        "# retain this comment\nmodel = 'selected'\nmcp_servers = { other = { command = 'other' } }\n",
    ] {
        fs::write(&path, text).unwrap();
        let cfg = config::Config::load(&path, Client::Codex).unwrap();
        let bytes = cfg.render(Some(&desired)).unwrap();
        fs::write(&path, &bytes).unwrap();
        let cfg = config::Config::load(&path, Client::Codex).unwrap();
        assert_eq!(cfg.entry().unwrap(), Some(desired.clone()));
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("# retain this comment"));
        let value: Value = toml_edit::de::from_str(&text).unwrap();
        assert_eq!(value["mcp_servers"]["other"]["command"], "other");
        assert_eq!(value["model"], "selected");
        let bytes = cfg.render(None).unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(
            config::Config::load(&path, Client::Codex)
                .unwrap()
                .entry()
                .unwrap()
                .is_none()
        );
    }
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}
#[test]
fn invalid_toml_does_not_disclose_config_line() {
    let root = root();
    let path = root.join("config.toml");
    let bytes = b"secret = 'do-not-log-this-secret\n";
    fs::write(&path, bytes).unwrap();
    let err = config::Config::load(&path, Client::Codex).err().unwrap();
    assert!(!format!("{err:#}").contains("do-not-log-this-secret"));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}
#[test]
fn json_preserves_project_settings_and_other_servers() {
    let root = root();
    let path = root.join("client.json");
    let original = json!({"projects":{"private-project":{"accepted":true}},"mcpServers":{"other":{"command":"other"}}});
    fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let entry = json!({"command":"framelet.exe","args":["mcp"]});
    let cfg = config::Config::load(&path, Client::ClaudeCode).unwrap();
    fs::write(&path, cfg.render(Some(&entry)).unwrap()).unwrap();
    let cfg = config::Config::load(&path, Client::ClaudeCode).unwrap();
    fs::write(&path, cfg.render(None).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
        original
    );
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}
