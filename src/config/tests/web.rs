use crate::config::web::{GENERATED_PASSWORD_LEN, PASSWORD_ALPHABET, upsert_password};
use crate::config::{
    Config, WebViewerConfig, ensure_web_viewer_password, generate_password, validate_config,
};

#[test]
fn web_config_defaults_to_loopback() {
    let cfg = WebViewerConfig::default();
    assert_eq!(cfg.bind, "127.0.0.1");
    assert_eq!(cfg.port, 8091);
    assert!(cfg.password.is_none());
    assert!(cfg.hashed_password.is_none());
    assert!(!cfg.has_credential());
}

#[test]
fn web_viewer_config_parses_from_toml() {
    let toml = r#"
[web_viewer]
bind = "0.0.0.0"
port = 9000
password = "hunter2"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.web_viewer.bind, "0.0.0.0");
    assert_eq!(cfg.web_viewer.port, 9000);
    assert_eq!(cfg.web_viewer.password.as_deref(), Some("hunter2"));
    assert!(cfg.web_viewer.has_credential());
    validate_config(&cfg).unwrap();
}

#[test]
fn config_without_web_table_defaults() {
    // A config file with no [web_viewer] table must still parse and validate,
    // falling back to the defaults.
    let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
    assert_eq!(cfg.web_viewer.port, 8091);
    validate_config(&cfg).unwrap();
}

#[test]
fn a_removed_sections_settings_are_ignored_not_rejected() {
    // Configs written before the web mirror was removed still carry its table.
    // Nothing deserializes with `deny_unknown_fields`, so the stale section is
    // dropped rather than failing the load of an otherwise valid file.
    let cfg: Config =
        toml::from_str("[web_mirror]\nenabled = true\nport = 8090\n\n[layout]\nupper_pct = 50\n")
            .unwrap();
    assert_eq!(cfg.layout.upper_pct, 50);
    validate_config(&cfg).unwrap();
}

#[test]
fn web_has_credential_treats_empty_password_as_missing() {
    let empty = WebViewerConfig {
        password: Some(String::new()),
        ..WebViewerConfig::default()
    };
    assert!(
        !empty.has_credential(),
        "an empty password is not a credential"
    );
    let with_pw = WebViewerConfig {
        password: Some("x".into()),
        ..WebViewerConfig::default()
    };
    assert!(with_pw.has_credential());
    let with_hash = WebViewerConfig {
        hashed_password: Some("$argon2id$...".into()),
        ..WebViewerConfig::default()
    };
    assert!(with_hash.has_credential());
}

#[test]
fn web_validation_rejects_port_zero() {
    let mut cfg = Config::default();
    cfg.web_viewer.port = 0;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn web_validation_rejects_a_bad_bind_address() {
    // Always checked now that the browser surface is part of the session:
    // there is no switched-off state in which a garbage address goes unread.
    let mut cfg = Config::default();
    cfg.web_viewer.bind = "not-an-ip".into();
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn generate_password_has_expected_length_and_alphabet() {
    let pw = generate_password().unwrap();
    assert_eq!(pw.chars().count(), GENERATED_PASSWORD_LEN);
    assert!(
        pw.bytes().all(|b| PASSWORD_ALPHABET.contains(&b)),
        "generated password must only use the unambiguous TOML-safe alphabet"
    );
    // Two draws should differ with overwhelming probability.
    assert_ne!(pw, generate_password().unwrap());
}

#[test]
fn upsert_password_adds_missing_key_to_existing_table() {
    let source = "[web_viewer]\nbind = \"127.0.0.1\"\nport = 8091\n";
    let out = upsert_password(source, "secret").unwrap();

    assert!(out.starts_with(source));
    assert_eq!(out.matches("password =").count(), 1);
    let cfg: Config = toml::from_str(&out).unwrap();
    assert_eq!(cfg.web_viewer.password.as_deref(), Some("secret"));
}

#[test]
fn upsert_password_appends_table_when_absent() {
    let source = "[layout]\nupper_pct = 55\n";
    let out = upsert_password(source, "secret").unwrap();

    assert!(out.starts_with(source));
    assert!(out.contains("\n[web_viewer]\npassword = \"secret\"\n"));
    let cfg: Config = toml::from_str(&out).unwrap();
    assert_eq!(cfg.web_viewer.password.as_deref(), Some("secret"));
}

#[test]
fn upsert_password_appends_table_into_empty_source() {
    let out = upsert_password("", "secret").unwrap();

    assert_eq!(out, "[web_viewer]\npassword = \"secret\"\n");
    let cfg: Config = toml::from_str(&out).unwrap();
    assert_eq!(cfg.web_viewer.password.as_deref(), Some("secret"));
}

#[test]
fn upsert_password_ignores_commented_header() {
    // A `# [web_viewer]` comment is not a real table header, so the password
    // must be appended as a new table rather than inserted under the comment.
    let source = "# [web_viewer] example\nfoo = 1\n";
    let out = upsert_password(source, "secret").unwrap();

    assert!(out.contains("\n[web_viewer]\npassword = \"secret\"\n"));
}

#[test]
fn upsert_password_replaces_value_without_losing_decor() {
    let source = "[web_viewer]\npassword   =   ''  # generated on first use\nport = 8091\n";

    let out = upsert_password(source, "secret").unwrap();

    assert_eq!(out.matches("password").count(), 1);
    assert!(out.contains("password   =   \"secret\"  # generated on first use"));
    assert_eq!(out.matches("[web_viewer]").count(), 1);
}

#[test]
fn upsert_password_preserves_crlf_when_appending_table() {
    let source = "[layout]\r\nupper_pct = 55\r\n";

    let out = upsert_password(source, "secret").unwrap();

    assert!(
        out.contains("\r\n[web_viewer]\r\npassword = \"secret\"\r\n"),
        "unexpected CRLF output: {out:?}"
    );
    assert!(
        !out.replace("\r\n", "").contains('\n'),
        "output contains a bare LF: {out:?}"
    );
}

#[test]
fn ensure_web_password_is_noop_when_credential_present() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = Config::default();
    cfg.web_viewer.password = Some("preset".into());
    let generated = ensure_web_viewer_password(&mut cfg, &path).unwrap();
    assert!(
        generated.is_none(),
        "an existing credential must not be replaced"
    );
    assert!(
        !path.exists(),
        "no file should be written when a password exists"
    );
    assert_eq!(cfg.web_viewer.password.as_deref(), Some("preset"));
}

#[test]
fn ensure_web_password_generates_persists_and_sets() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "[web_viewer]\nport = 8091\n").unwrap();

    let mut cfg = Config::default();
    let generated = ensure_web_viewer_password(&mut cfg, &path).unwrap();

    let pw = generated.expect("a password must be generated when none is set");
    assert_eq!(cfg.web_viewer.password.as_deref(), Some(pw.as_str()));
    // The persisted file now parses back to the same password.
    let reparsed: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reparsed.web_viewer.password.as_deref(), Some(pw.as_str()));
}

#[test]
fn ensure_web_password_replaces_an_empty_password_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    let source = "[web_viewer]\npassword = \"\" # generated on first use\nport = 8091\n";
    std::fs::write(&path, source).unwrap();
    let mut cfg: Config = toml::from_str(source).unwrap();

    let password = ensure_web_viewer_password(&mut cfg, &path)
        .unwrap()
        .expect("an empty password must be replaced");

    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(saved.matches("password =").count(), 1);
    assert!(saved.contains("# generated on first use"));
    let reparsed: Config = toml::from_str(&saved).unwrap();
    assert_eq!(
        reparsed.web_viewer.password.as_deref(),
        Some(password.as_str())
    );
}

#[test]
fn ensure_web_password_creates_file_when_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = Config::default();

    let pw = ensure_web_viewer_password(&mut cfg, &path)
        .unwrap()
        .unwrap();

    assert!(path.exists());
    let reparsed: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reparsed.web_viewer.password.as_deref(), Some(pw.as_str()));
}
