use super::PreAttachAdmission;
use std::sync::Arc;

#[test]
fn stalled_no_frame_peers_are_bounded_and_raii_permits_release() {
    let admission = Arc::new(PreAttachAdmission::new(2));
    let partial_frame_peer = admission.try_reserve().expect("first slot");
    let no_frame_peer = admission.try_reserve().expect("second slot");

    assert_eq!(admission.active(), 2);
    assert!(admission.try_reserve().is_none(), "the cap is atomic");

    drop(partial_frame_peer);
    assert_eq!(admission.active(), 1);
    let returned_slot = admission.try_reserve().expect("a released slot returns");
    drop(no_frame_peer);
    assert_eq!(admission.active(), 1);
    drop(returned_slot);
    assert_eq!(admission.active(), 0);
}
