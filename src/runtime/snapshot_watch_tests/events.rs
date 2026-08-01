use super::*;

#[test]
fn an_open_caused_by_the_read_itself_does_not_wake_the_reader() {
    let (dir, path) = make_repo();

    for opened in [".git/HEAD", ".git/refs/heads/main", "src/main.rs", ""] {
        let event = at(
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            &path,
            opened,
        );
        assert!(!wakes_the_reader(&path, event), "opened {opened}");
    }
    drop(dir);
}

#[test]
fn a_finished_write_wakes_the_reader() {
    let (dir, path) = make_repo();
    let closed = EventKind::Access(AccessKind::Close(AccessMode::Write));

    assert!(wakes_the_reader(&path, at(closed, &path, ".git/HEAD")));
    assert!(wakes_the_reader(&path, at(closed, &path, "src/main.rs")));
    drop(dir);
}

#[test]
fn an_ordinary_write_or_creation_wakes_the_reader() {
    let (dir, path) = make_repo();

    for kind in [
        EventKind::Modify(ModifyKind::Any),
        EventKind::Create(CreateKind::File),
    ] {
        assert!(
            wakes_the_reader(&path, at(kind, &path, "src/main.rs")),
            "{kind:?}"
        );
    }
    drop(dir);
}

#[test]
fn a_dropped_events_signal_wakes_the_reader() {
    let (dir, path) = make_repo();

    let overflowed = Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan));
    assert!(wakes_the_reader(&path, overflowed));
    assert!(wakes_the_reader(&path, Err(notify::Error::generic("boom"))));
    drop(dir);
}
