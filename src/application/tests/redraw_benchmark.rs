//! Release-only measurements for the dirty-frame event loop.
//!
//! Run explicitly because the PTY half starts a real shell:
//!
//! ```text
//! cargo test --release measure_dirty_redraw -- --ignored --nocapture
//! ```

use crate::application::redraw::{RedrawCause, RedrawState};
#[cfg(unix)]
use crate::backend::{PtyBackend, TerminalBackend};
#[cfg(unix)]
use crate::config::ShellConfig;
use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
use std::time::{Duration, Instant};

const TICKS: usize = 600;
const TICK: Duration = Duration::from_millis(16);
#[cfg(unix)]
const PTY_SAMPLES: usize = 20;
#[cfg(unix)]
const PTY_TIMEOUT: Duration = Duration::from_secs(5);

struct DrawRun {
    draws: usize,
    elapsed: Duration,
}

fn draw_frame(terminal: &mut Terminal<TestBackend>) {
    terminal
        .draw(|frame| frame.render_widget(Paragraph::new("nightcrow frame"), frame.area()))
        .expect("test backend draw must succeed");
}

fn run_unconditional(ticks: usize) -> DrawRun {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test backend");
    let started = Instant::now();
    for _ in 0..ticks {
        draw_frame(&mut terminal);
    }
    DrawRun {
        draws: ticks,
        elapsed: started.elapsed(),
    }
}

fn run_dirty(ticks: usize, event_every: Option<usize>) -> DrawRun {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test backend");
    let mut state = RedrawState::new();
    let started = Instant::now();
    let mut draws = 0;
    for tick in 0..ticks {
        if event_every.is_some_and(|period| tick % period == 0) {
            state.request(RedrawCause::Terminal);
        }
        if state.take() {
            draw_frame(&mut terminal);
            draws += 1;
        }
    }
    DrawRun {
        draws,
        elapsed: started.elapsed(),
    }
}

fn p95(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    let rank = (samples.len() * 95).div_ceil(100).saturating_sub(1);
    samples[rank]
}

/// Events arrive after the current frame's draw in `main_loop`, so the next
/// frame is bounded by one input-poll interval. Keep this virtual measurement
/// beside the real PTY sample to make the latency claim explicit and stable.
fn simulated_event_latency_p95(ticks: usize, event_every: usize) -> Duration {
    let samples = (0..ticks).step_by(event_every).map(|_| TICK).collect();
    p95(samples)
}

#[cfg(unix)]
fn pty_command(marker: &str) -> Vec<u8> {
    format!("printf '{marker}\\n'\n").into_bytes()
}

#[cfg(unix)]
fn measure_pty_echo() -> Vec<Duration> {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let pane = backend
        .open_pane(24, 80, None)
        .expect("benchmark PTY must open");

    // Let the shell's startup output settle before sampling command echoes.
    let warmup_deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < warmup_deadline {
        let _ = backend.drain_events();
        std::thread::sleep(TICK);
    }

    let mut samples = Vec::with_capacity(PTY_SAMPLES);
    for i in 0..PTY_SAMPLES {
        let marker = format!("nightcrow-echo-{i}");
        backend
            .send_input(pane, &pty_command(&marker))
            .expect("benchmark PTY input must succeed");
        let started = Instant::now();
        let deadline = started + PTY_TIMEOUT;
        let mut output = Vec::new();
        let mut seen = false;
        while Instant::now() < deadline {
            for event in backend.drain_events() {
                if let crate::backend::BackendEvent::Output { pane: id, data } = event
                    && id == pane
                {
                    output.extend(data);
                    if output
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes())
                    {
                        seen = true;
                        break;
                    }
                }
            }
            if seen {
                break;
            }
            std::thread::sleep(TICK);
        }
        assert!(
            seen,
            "PTY marker {marker} did not arrive before {PTY_TIMEOUT:?}"
        );
        samples.push(started.elapsed());
    }
    backend.destroy_pane(pane);
    samples
}

#[test]
#[ignore = "release benchmark; starts a real shell and reports machine-specific timings"]
fn measure_dirty_redraw() {
    let before = run_unconditional(TICKS);
    let idle = run_dirty(TICKS, None);
    let heartbeat = run_dirty(TICKS, Some(60));
    let active = run_dirty(TICKS, Some(1));
    let before_latency = simulated_event_latency_p95(TICKS, 1);
    let after_latency = simulated_event_latency_p95(TICKS, 1);

    assert_eq!(idle.draws, 1, "idle should retain only the initial frame");
    assert_eq!(heartbeat.draws, TICKS / 60, "one heartbeat per second");
    assert_eq!(
        active.draws, TICKS,
        "an event every tick remains responsive"
    );

    println!(
        "draws/10s@60fps: before={} idle={} heartbeat={} active={}",
        before.draws, idle.draws, heartbeat.draws, active.draws
    );
    println!(
        "cpu-ms/10s-simulation: before={:.3} idle={:.3} heartbeat={:.3} active={:.3}",
        before.elapsed.as_secs_f64() * 1000.0,
        idle.elapsed.as_secs_f64() * 1000.0,
        heartbeat.elapsed.as_secs_f64() * 1000.0,
        active.elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "event-to-next-frame-p95-ms: before={:.3} after={:.3} (poll cap {:?})",
        before_latency.as_secs_f64() * 1000.0,
        after_latency.as_secs_f64() * 1000.0,
        TICK
    );
    #[cfg(unix)]
    {
        let echo = measure_pty_echo();
        println!(
            "pty-echo-p95-ms: {:.3} ({} samples; poll cap {:?})",
            p95(echo).as_secs_f64() * 1000.0,
            PTY_SAMPLES,
            TICK
        );
    }
    #[cfg(not(unix))]
    println!("pty-echo-p95-ms: unavailable (headless Windows ConPTY)");
}
