use super::{Shutdown, ShutdownWatch};
use signal_hook::consts::signal::{SIGINT, SIGTERM};

/// Raising a signal reaches every registered watch in the process, so the
/// cases must not overlap: one test's signal would otherwise be collected by
/// another test's watch. Each case drops its watch before releasing the lock,
/// which takes its handlers down with it.
static SIGNAL_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Raise `signal` at a registered watch and report what it saw.
///
/// Registration completes before the raise, which is the whole point of the
/// split: with the handlers already installed, the signal is held rather than
/// terminating the test process at its default disposition. Nothing here
/// sleeps or retries — the wait is answered by a signal that has already
/// arrived.
fn deliver(signal: i32) -> Shutdown {
    let _guard = SIGNAL_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let watch = ShutdownWatch::register().expect("handlers install");
    signal_hook::low_level::raise(signal).expect("raising a registered signal");
    watch.wait().expect("waiting for a stop signal succeeds")
}

#[test]
fn ctrl_c_is_reported_as_an_interrupt() {
    assert_eq!(deliver(SIGINT), Shutdown::Interrupt);
}

#[test]
fn a_service_manager_stop_is_reported_as_a_terminate() {
    assert_eq!(deliver(SIGTERM), Shutdown::Terminate);
}

#[test]
fn a_signal_that_arrives_before_the_wait_is_not_lost() {
    // The gap this closes: a stop signal during startup, before the server is
    // ready to wait on it. Registration is what makes it survive, so the raise
    // here is deliberately far from the wait.
    let _guard = SIGNAL_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let watch = ShutdownWatch::register().expect("handlers install");
    signal_hook::low_level::raise(SIGTERM).expect("raising a registered signal");

    // Stand in for the work a server does between registering and waiting.
    let mut startup = 0u64;
    for i in 0..10_000 {
        startup = startup.wrapping_add(i);
    }
    assert_ne!(startup, 0, "the stand-in work is not optimized away");

    assert_eq!(
        watch.wait().expect("a held signal is collected"),
        Shutdown::Terminate
    );
}

#[test]
fn each_stop_signal_names_itself() {
    assert_eq!(Shutdown::Interrupt.as_str(), "SIGINT");
    assert_eq!(Shutdown::Terminate.as_str(), "SIGTERM");
}
