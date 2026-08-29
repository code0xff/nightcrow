use super::*;

#[test]
fn traffic_that_arrives_before_a_repository_has_a_reader_is_kept() {
    // The daemon subscribes a client to every open repository the moment it
    // connects, so a pane and its scrollback can be on the wire before the
    // client has been told the repository exists. The replay happens once --
    // dropping it would lose those panes for good.
    let router = TerminalRouter::default();

    router.deliver("r1", created(1)).unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: b"prompt$ ".to_vec(),
            },
        )
        .unwrap();

    let inbox = router.drain("r1");
    assert_eq!(inbox.len(), 2);
    assert_eq!(pane_of(&inbox[0]), 1);
}

#[test]
fn each_repository_drains_only_its_own_traffic() {
    let router = TerminalRouter::default();
    router.deliver("r1", created(1)).unwrap();
    router.deliver("r2", created(2)).unwrap();

    let first = router.drain("r1");
    assert_eq!(first.len(), 1);
    assert_eq!(pane_of(&first[0]), 1);
    let second = router.drain("r2");
    assert_eq!(second.len(), 1);
    assert_eq!(pane_of(&second[0]), 2);
}

#[test]
fn a_drained_inbox_is_empty_until_more_arrives() {
    let router = TerminalRouter::default();
    router.deliver("r1", created(1)).unwrap();

    assert_eq!(router.drain("r1").len(), 1);
    assert!(router.drain("r1").is_empty());
    assert!(
        router.drain("never-heard-of-it").is_empty(),
        "and an unknown repository is empty rather than a panic"
    );
}

#[test]
fn closing_a_repository_drops_what_was_queued_for_it() {
    // Its backend went with its tab, so nothing will ever drain this.
    let router = TerminalRouter::default();
    router.deliver("r1", created(1)).unwrap();
    router.deliver("gone", created(9)).unwrap();

    router.retain(&["r1".to_string()]);

    assert_eq!(router.drain("r1").len(), 1);
    assert!(router.drain("gone").is_empty());
}

#[test]
fn a_busy_repository_does_not_spend_another_repositories_budget() {
    let router = TerminalRouter::default();
    for pane in 1..=TERMINAL_DRAIN_MESSAGES as PaneId + 1 {
        router.deliver("busy", created(pane)).unwrap();
    }
    router.deliver("quiet", created(999)).unwrap();

    assert_eq!(router.drain("busy").len(), TERMINAL_DRAIN_MESSAGES);
    let quiet = router.drain("quiet");
    assert_eq!(quiet.len(), 1);
    assert_eq!(pane_of(&quiet[0]), 999);
}
