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
