use super::*;

fn request(generation: u64, operation: GitLoadOperation) -> GitLoadRequest {
    GitLoadRequest {
        repo: "repo".into(),
        generation,
        operation,
    }
}

#[test]
fn 같은_lane의_대기_요청은_최신_요청_하나로_합쳐진다() {
    let mut pending = Pending::default();
    for generation in 1..=100_000 {
        pending.replace(request(
            generation,
            GitLoadOperation::StatusDiff(format!("{generation}.rs")),
        ));
    }

    let latest = pending.take_next().unwrap();
    assert_eq!(latest.generation, 100_000);
    assert!(pending.take_next().is_none());
}

#[test]
fn 서로_다른_lane의_요청은_서로를_덮어쓰지_않는다() {
    let mut pending = Pending::default();
    pending.replace(request(1, GitLoadOperation::StatusDiff("a.rs".into())));
    pending.replace(request(2, GitLoadOperation::WorkdirFile("a.rs".into())));

    assert!(pending.take_next().is_some());
    assert!(pending.take_next().is_some());
}

#[test]
fn continuously_refilled_diff_lane_cannot_starve_other_lanes() {
    let mut pending = Pending::default();
    pending.replace(request(1, GitLoadOperation::StatusDiff("a.rs".into())));
    pending.replace(request(2, GitLoadOperation::WorkdirFile("a.rs".into())));
    pending.replace(request(3, GitLoadOperation::CommitFiles(Oid::ZERO_SHA1)));
    pending.replace(request(4, GitLoadOperation::Decorations));

    let mut lanes = Vec::new();
    for generation in 5..9 {
        let next = pending.take_next().unwrap();
        lanes.push(next.operation.lane());
        pending.replace(request(
            generation,
            GitLoadOperation::StatusDiff(format!("{generation}.rs")),
        ));
    }

    assert!(lanes.contains(&LoadLane::File));
    assert!(lanes.contains(&LoadLane::CommitFiles));
    assert!(lanes.contains(&LoadLane::Decorations));
}
