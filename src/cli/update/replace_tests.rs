use super::*;

#[test]
fn a_failed_install_restores_the_previous_binary() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"old").unwrap();

    let error = replace_target(&target, |target| {
        std::fs::write(target, b"partial")?;
        anyhow::bail!("installer stopped")
    })
    .unwrap_err();

    assert!(error.to_string().contains("installer stopped"));
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
}

#[test]
fn a_successful_install_replaces_the_previous_binary() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"old").unwrap();

    replace_target(&target, |target| {
        std::fs::write(target, b"new")?;
        Ok(())
    })
    .unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"new");
}

#[test]
fn successful_update_message_explains_pending_cleanup_without_a_path() {
    assert_eq!(
        success_message(true),
        "nightcrow: update installed successfully.\nnightcrow: the parked old binary is still in use by the running session or updater; cleanup is pending.\nnightcrow: no second update is needed. If a session is running, run `nightcrow stop`, then start nightcrow again to use the new version."
    );
}

#[test]
fn successful_update_message_explains_restart_after_cleanup() {
    assert_eq!(
        success_message(false),
        "nightcrow: update installed successfully.\nnightcrow: no second update is needed. If a session is running, run `nightcrow stop`, then start nightcrow again to use the new version."
    );
}
