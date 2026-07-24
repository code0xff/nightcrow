use crate::config::{
    Config, WebMirrorConfig, ensure_web_mirror_password, generate_password, validate_config,
};
use crate::config::web::{
    GENERATED_PASSWORD_LEN, PASSWORD_ALPHABET, WEB_MIRROR_TABLE, WEB_VIEWER_TABLE, insert_password,
};

#[test]
fn web_config_defaults_are_off_and_loopback() {
    let cfg = WebMirrorConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.bind, "127.0.0.1");
    assert_eq!(cfg.port, 8090);
    assert!(cfg.password.is_none());
    assert!(cfg.hashed_password.is_none());
    assert!(!cfg.has_credential());
}

#[test]
fn web_mirror_config_parses_from_toml() {
    let toml = r#"
[web_mirror]
enabled = true
bind = "0.0.0.0"
port = 9000
password = "hunter2"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(cfg.web_mirror.enabled);
    assert_eq!(cfg.web_mirror.bind, "0.0.0.0");
    assert_eq!(cfg.web_mirror.port, 9000);
    assert_eq!(cfg.web_mirror.password.as_deref(), Some("hunter2"));
    assert!(cfg.web_mirror.has_credential());
    validate_config(&cfg).unwrap();
}

#[test]
fn config_without_web_table_defaults() {
    // A pre-existing config file with no [web_mirror] table must still parse and
    // validate, falling back to the disabled default.
    let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
    assert!(!cfg.web_mirror.enabled);
    assert_eq!(cfg.web_mirror.port, 8090);
    validate_config(&cfg).unwrap();
}

#[test]
fn web_has_credential_treats_empty_password_as_missing() {
    let empty = WebMirrorConfig {
        password: Some(String::new()),
        ..WebMirrorConfig::default()
    };
    assert!(
        !empty.has_credential(),
        "an empty password is not a credential"
    );
    let with_pw = WebMirrorConfig {
        password: Some("x".into()),
        ..WebMirrorConfig::default()
    };
    assert!(with_pw.has_credential());
    let with_hash = WebMirrorConfig {
        hashed_password: Some("$argon2id$...".into()),
        ..WebMirrorConfig::default()
    };
    assert!(with_hash.has_credential());
}

#[test]
fn web_validation_rejects_port_zero_when_enabled() {
    let mut cfg = Config::default();
    cfg.web_mirror.enabled = true;
    cfg.web_mirror.port = 0;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn web_validation_rejects_bad_bind_when_enabled() {
    let mut cfg = Config::default();
    cfg.web_mirror.enabled = true;
    cfg.web_mirror.bind = "not-an-ip".into();
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn web_validation_ignores_bind_and_port_when_disabled() {
    // A disabled web section is never acted on, so its fields are not
    // range-checked — a stale/garbage value must not block startup.
    let mut cfg = Config::default();
    cfg.web_mirror.enabled = false;
    cfg.web_mirror.port = 0;
    cfg.web_mirror.bind = "not-an-ip".into();
    assert!(validate_config(&cfg).is_ok());
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
fn insert_password_adds_line_under_existing_header() {
    let source = "[web_mirror]\nenabled = true\nport = 8090\n";
    let out = insert_password(source, WEB_MIRROR_TABLE, "secret");
    assert_eq!(
        out, "[web_mirror]\npassword = \"secret\"\nenabled = true\nport = 8090\n",
        "the password line must land right after the [web_mirror] header"
    );
    // The result round-trips and exposes the password.
    let cfg: Config = toml::from_str(&out).unwrap();
    assert_eq!(cfg.web_mirror.password.as_deref(), Some("secret"));
}

#[test]
fn insert_password_appends_table_when_absent() {
    let source = "[layout]\nupper_pct = 55\n";
    let out = insert_password(source, WEB_MIRROR_TABLE, "secret");
    assert!(out.starts_with(source));
    assert!(out.contains("\n[web_mirror]\npassword = \"secret\"\n"));
    let cfg: Config = toml::from_str(&out).unwrap();
    assert_eq!(cfg.web_mirror.password.as_deref(), Some("secret"));
}

#[test]
fn insert_password_appends_table_into_empty_source() {
    let out = insert_password("", WEB_MIRROR_TABLE, "secret");
    assert_eq!(out, "[web_mirror]\npassword = \"secret\"\n");
    let cfg: Config = toml::from_str(&out).unwrap();
    assert_eq!(cfg.web_mirror.password.as_deref(), Some("secret"));
}

#[test]
fn the_web_mirror_table_configures_the_mirror() {
    let cfg: Config = toml::from_str("[web_mirror]\nenabled = true\nport = 8100\n").unwrap();

    assert!(cfg.web_mirror.enabled);
    assert_eq!(cfg.web_mirror.port, 8100);
}

#[test]
fn insert_password_targets_the_named_table() {
    // The viewer's credential must land under [web_viewer], not [web] —
    // writing it to the wrong table would silently give the mirror a
    // second password and leave the viewer without one.
    let source = "[web_mirror]\nport = 8090\n";

    let out = insert_password(source, WEB_VIEWER_TABLE, "vsecret");

    assert!(
        out.contains("[web_viewer]\npassword = \"vsecret\""),
        "got: {out}"
    );
    let web_table = out.split("[web_viewer]").next().unwrap();
    assert!(
        !web_table.contains("vsecret"),
        "the viewer password leaked into [web_mirror]: {out}"
    );
}

#[test]
fn insert_password_finds_an_existing_viewer_table() {
    let source = "[web_mirror]\nport = 8090\n\n[web_viewer]\nport = 8091\n";

    let out = insert_password(source, WEB_VIEWER_TABLE, "vsecret");

    let viewer = out.split("[web_viewer]").nth(1).unwrap();
    assert!(viewer.contains("password = \"vsecret\""), "got: {out}");
    assert_eq!(out.matches("[web_viewer]").count(), 1, "no duplicate table");
}

#[test]
fn insert_password_ignores_commented_header() {
    // A `# [web]` comment is not a real table header, so the password must
    // be appended as a new table rather than inserted under the comment.
    let source = "# [web] example\nfoo = 1\n";
    let out = insert_password(source, WEB_MIRROR_TABLE, "secret");
    assert!(out.contains("\n[web_mirror]\npassword = \"secret\"\n"));
}

#[test]
fn ensure_web_password_is_noop_when_credential_present() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = Config::default();
    cfg.web_mirror.enabled = true;
    cfg.web_mirror.password = Some("preset".into());
    let generated = ensure_web_mirror_password(&mut cfg, &path).unwrap();
    assert!(
        generated.is_none(),
        "an existing credential must not be replaced"
    );
    assert!(
        !path.exists(),
        "no file should be written when a password exists"
    );
    assert_eq!(cfg.web_mirror.password.as_deref(), Some("preset"));
}

#[test]
fn ensure_web_password_generates_persists_and_sets() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "[web_mirror]\nenabled = true\n").unwrap();

    let mut cfg = Config::default();
    cfg.web_mirror.enabled = true;
    let generated = ensure_web_mirror_password(&mut cfg, &path).unwrap();

    let pw = generated.expect("a password must be generated when none is set");
    assert_eq!(cfg.web_mirror.password.as_deref(), Some(pw.as_str()));
    // The persisted file now parses back to the same password.
    let reparsed: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reparsed.web_mirror.password.as_deref(), Some(pw.as_str()));
}

#[test]
fn ensure_web_password_creates_file_when_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = Config::default();
    cfg.web_mirror.enabled = true;

    let pw = ensure_web_mirror_password(&mut cfg, &path)
        .unwrap()
        .unwrap();

    assert!(path.exists());
    let reparsed: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reparsed.web_mirror.password.as_deref(), Some(pw.as_str()));
}