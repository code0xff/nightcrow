use super::ProcessTree;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem as _};

#[test]
fn attaches_to_an_exited_but_unreaped_pty_child() {
    let pair = NativePtySystem::default()
        .openpty(PtySize::default())
        .expect("open a real PTY");
    let mut command = CommandBuilder::new("sh");
    command.arg("-c");
    command.arg("exit 0");
    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("spawn an exited-child fixture");
    drop(pair.slave);

    let pid = child.process_id().expect("PTY child exposes a PID") as libc::pid_t;
    let mut status = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    // WNOWAIT leaves the exited child waitable for the cleanup wait below.
    let wait_result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            status.as_mut_ptr(),
            libc::WEXITED | libc::WNOWAIT,
        )
    };
    assert_eq!(wait_result, 0, "waitid(WEXITED | WNOWAIT)");

    let tree = ProcessTree::attach(&*child, &*pair.master).expect("attach exited child");
    assert_eq!(tree.process_group, pid);
    assert_eq!(tree.session, pid);
    assert_eq!(tree.tty, pair.master.as_raw_fd());

    let exit = child.wait().expect("reap the exited child");
    assert!(exit.success());
}
