use super::inherit_ctrl_c_as_an_event;

/// The flag has no getter and observing it needs a real ConPTY child, so the
/// seam being callable is all a test can claim. Idempotence is the contract that
/// matters: a second caller must not fail startup.
#[test]
fn clearing_the_inherited_disposition_succeeds_and_repeats() {
    inherit_ctrl_c_as_an_event().expect("the disposition is cleared");
    inherit_ctrl_c_as_an_event().expect("clearing it again is not an error");
}
