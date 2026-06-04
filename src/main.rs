mod app;
mod backend;
mod config;
mod git;
mod input;
mod logging;
mod runtime;
mod session;
#[cfg(test)]
mod test_util;
mod ui;
mod util;

use anyhow::{Context, Result};
use app::{App, Focus, ViewMode};
use clap::Parser;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use input::{Action, encode_key, map_key, prefix_action, vim_navigation_action};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use std::{io, time::Duration};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// nightcrow — TUI for Agentic Coding
///
/// Opens a git diff viewer (top) and multi-terminal panes (bottom)
/// in the current directory.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to the git repository (defaults to current directory)
    #[arg(short, long)]
    repo: Option<std::path::PathBuf>,

    /// Open a terminal pane running this command at startup. Repeatable;
    /// each --exec adds one pane after any config [[startup_command]] panes.
    #[arg(long = "exec", value_name = "COMMAND")]
    exec: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::load_config()?;
    // Resolve before entering the alternate screen so a too-many-panes error
    // surfaces as plain stderr text rather than a flash behind the TUI.
    let startup_commands = config::resolve_startup_commands(&cfg, &cli.exec)?;
    // Parse the leader before the alternate screen too, so a malformed
    // `[input] leader` is reported as plain stderr. `load_config` already
    // validated it; re-parsing keeps the KeyEvent local to the app setup.
    let leader = config::parse_leader(&cfg.input.leader)?;

    let input_path = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir().context("cannot determine current directory")?,
    };
    let repo_path = git::resolve_repo_path(input_path)
        .to_string_lossy()
        .to_string();

    let _log_guard = logging::init_logging(&cfg.log, &repo_path);

    tracing::info!(
        level = cfg.log.level.as_str(),
        rotation = ?cfg.log.rotation,
        prompt_log = cfg.log.prompt_log,
        "logging initialized"
    );

    let _guard = TerminalGuard::enter()?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    run(&mut terminal, repo_path, cfg, startup_commands, leader)
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        // EnableBracketedPaste makes crossterm surface paste as
        // `Event::Paste(String)` instead of a flood of `Event::Key` chars —
        // the latter would each be filtered as control chars by the search
        // handler and silently drop newlines.
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

enum KeyOutcome {
    Continue,
    Quit,
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo_path: String,
    cfg: config::Config,
    startup_commands: Vec<config::StartupCommand>,
    leader: KeyEvent,
) -> Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let mut app = init_app(&repo_path, &cfg, &startup_commands, leader);

    if matches!(splash_loop(terminal, &app)?, SplashOutcome::Quit) {
        tracing::info!(repo = %app.repo_path, "nightcrow stopped during splash");
        return Ok(());
    }
    main_loop(terminal, &mut app, &ss, &ts, &cfg)?;

    session::save_session(&app.repo_path, &app.save_session());
    tracing::info!(repo = %app.repo_path, "nightcrow stopped");
    Ok(())
}

fn init_app(
    repo_path: &str,
    cfg: &config::Config,
    startup_commands: &[config::StartupCommand],
    leader: KeyEvent,
) -> App {
    let saved_session = session::load_session(repo_path);
    let mut app = App::new(
        repo_path.to_string(),
        cfg.log.prompt_log,
        startup_commands,
        leader,
    );
    app.set_accent_index(cfg.theme.preset_index());
    app.cfg_agent_indicator = cfg.agent_indicator.clone();
    app.pagination.page_size = cfg.log.commit_log_page_size;
    app.pagination.prefetch_threshold = cfg.log.commit_log_prefetch_threshold;
    if let Some(state) = saved_session {
        app.set_accent_index(state.accent_idx);
        app.set_pending_session(state);
    }
    app
}

enum SplashOutcome {
    Enter,
    Quit,
}

fn splash_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
) -> Result<SplashOutcome> {
    let splash = ui::splash::SplashState::new();
    loop {
        let accent = app.current_accent();
        terminal.draw(|frame| {
            ui::splash::draw(frame, &splash, accent);
        })?;
        if splash.is_done() {
            break;
        }
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Honour Esc so the user can abort during the splash instead
                // of being forced to wait for it to clear and quit from the
                // main view. (Leader-based quit needs a two-key sequence, so
                // it isn't recognised on the one-shot splash screen.) Any
                // other key dismisses the splash.
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if k.code == KeyCode::Esc {
                        return Ok(SplashOutcome::Quit);
                    }
                    break;
                }
                Event::Resize(_, _) => terminal.clear()?,
                _ => {}
            }
        }
    }
    terminal.clear()?;
    Ok(SplashOutcome::Enter)
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    cfg: &config::Config,
) -> Result<()> {
    loop {
        app.poll_snapshot();
        app.poll_terminal();
        app.poll_commit_log_page_fetch();

        let size = terminal.size()?;
        if let Some(area) =
            ui::terminal_content_area(app, Rect::new(0, 0, size.width, size.height), &cfg.layout)
        {
            app.terminal.resize_panes(area.height, area.width);
            app.terminal.sync_scroll();
        }

        let accent = app.current_accent();
        terminal.draw(|frame| {
            ui::draw(frame, app, ss, ts, &cfg.layout, accent);
        })?;

        // 16 ms ≈ 60 fps. The previous 50 ms tick noticeably lagged PTY echo
        // on every keystroke (typing felt sticky). `event::poll` performs an
        // OS-level wait when nothing is happening, so the higher cap doesn't
        // burn CPU at idle.
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Ratatui's next draw will pick up the new size from
                // `Frame::area()`. An explicit clear() here only adds a
                // visible flash on resize without improving correctness.
                Event::Resize(_, _) => {}
                Event::Key(key) => {
                    if matches!(handle_key(app, key), KeyOutcome::Quit) {
                        return Ok(());
                    }
                }
                Event::Paste(text) => handle_paste(app, &text),
                _ => {}
            }
        }
    }
}

/// Route a bracketed-paste payload to the appropriate sink.
///
/// Modal overlays (repo input, file/diff search) accept the text after
/// stripping control characters — the same rule the typed-key handlers
/// enforce. The terminal pane receives the paste re-wrapped in
/// `ESC [200~ ... ESC [201~` so the inner shell can distinguish multi-line
/// paste from interactive input (crossterm consumes the outer markers when
/// surfacing `Event::Paste`).
fn handle_paste(app: &mut App, text: &str) {
    // A paste arriving while the prefix is armed would otherwise leave the
    // PREFIX indicator stuck and make the next key resolve as a follow-up.
    // Resolve the prefix first (tmux treats a non-command event as a cancel),
    // then route the paste normally.
    app.cancel_prefix();
    if app.repo_input.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.repo_input_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList && app.status_view.search_active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList
        && (app.log_view.commit_search_active || app.log_view.file_search_active)
    {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.log_search_push(ch);
        }
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.diff.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::Terminal {
        // Strip ESC (0x1b) and NUL (0x00) before forwarding: an embedded
        // 0x1b can re-arm or cancel the bracketed-paste boundary the shell
        // is parsing, and NUL is malformed for most line-buffered shells.
        // Newlines, tabs, and other printable controls stay in — they are
        // exactly what bracketed paste is meant to deliver atomically.
        let sanitized: Vec<u8> = text
            .as_bytes()
            .iter()
            .copied()
            .filter(|&b| b != 0x1b && b != 0x00)
            .collect();
        // Only wrap in bracketed-paste markers when the running program asked
        // for them (DECSET 2004). A raw program that never enabled the mode
        // would otherwise receive the literal `[200~`/`[201~` markers as input.
        let bracketed = app
            .active_screen()
            .map(|screen| screen.bracketed_paste())
            .unwrap_or(false);
        if bracketed {
            let mut bytes = Vec::with_capacity(sanitized.len() + 12);
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(&sanitized);
            bytes.extend_from_slice(b"\x1b[201~");
            app.terminal.send_input(&bytes);
        } else {
            app.terminal.send_input(&sanitized);
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    // Crossterm emits Press/Repeat/Release for every keystroke on Windows
    // and on terminals that negotiate the kitty keyboard protocol.
    // Without this guard every keypress would be processed twice or more
    // — visible as doubled search chars, Ctrl+Q firing repeatedly, and
    // Backspace popping past the buffer.
    if key.kind != KeyEventKind::Press {
        return KeyOutcome::Continue;
    }

    // Modal overlays (repo-input dialog, both search bars) own every
    // keystroke until dismissed. They are checked before any leader handling
    // so a leader keypress while a search/repo dialog is open is typed/edited
    // by the overlay rather than arming the prefix.
    let overlay_active = app.repo_input.active
        || app.status_view.search_active
        || app.diff.search.active
        || app.log_view.commit_search_active
        || app.log_view.file_search_active;
    if overlay_active {
        // A prefix could only be armed if an overlay opened out from under it;
        // disarm so the indicator never lingers behind a modal.
        app.cancel_prefix();
        if app.repo_input.active {
            handle_repo_input_key(app, key);
        } else {
            // Search overlays are handled inside the focus-local upper handler.
            handle_upper_key(app, key, Action::None);
        }
        return KeyOutcome::Continue;
    }

    // Prefix is armed: this key is the single follow-up. Resolve it three
    // ways — Esc/Ctrl+C cancels, the leader again sends a literal leader to
    // the PTY, a mapped key runs its action; any other key is consumed.
    if app.prefix_armed() {
        return handle_prefix_followup(app, key);
    }

    // The leader chord arms the prefix; nothing else happens this tick.
    if app.is_leader_key(key) {
        app.arm_prefix();
        return KeyOutcome::Continue;
    }

    let action = map_key(key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }

    match app.focus {
        Focus::Terminal => handle_terminal_key(app, key, action),
        Focus::FileList | Focus::DiffViewer => handle_upper_key(app, key, action),
    }
    KeyOutcome::Continue
}

/// Resolve the single key pressed while the prefix is armed. The prefix is
/// always disarmed before returning (tmux-style: one follow-up per leader).
fn handle_prefix_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
    app.cancel_prefix();

    // `<L> <L>`: send the leader chord literally to the focused PTY so the
    // running program still sees the prefix key when the user means it. This
    // is resolved before the Esc/Ctrl+C cancel below so that a `ctrl+c` leader
    // can still deliver a literal Ctrl+C via `<leader><leader>` (Esc remains a
    // universal cancel regardless of the configured leader).
    if app.is_leader_key(key) {
        if app.focus == Focus::Terminal
            && let Some(data) = encode_key(app.leader)
        {
            app.terminal.send_input(&data);
        }
        return KeyOutcome::Continue;
    }

    // Esc / Ctrl+C cancel the prefix without acting. The follow-up key is
    // consumed (not forwarded) so the cancel never leaks into the PTY.
    let is_ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if key.code == KeyCode::Esc || is_ctrl_c {
        return KeyOutcome::Continue;
    }

    // A mapped follow-up runs its app command everywhere (terminal + upper).
    let action = prefix_action(key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }
    // Unmapped follow-up: consume and drop it, then return to pass-through.
    KeyOutcome::Continue
}

fn handle_global_action(app: &mut App, action: Action) -> Option<KeyOutcome> {
    match action {
        Action::Quit => Some(KeyOutcome::Quit),
        Action::NewPane => {
            app.open_new_pane();
            Some(KeyOutcome::Continue)
        }
        Action::ClosePane => {
            app.close_active_pane();
            Some(KeyOutcome::Continue)
        }
        Action::ChangeRepo => {
            app.start_repo_input();
            Some(KeyOutcome::Continue)
        }
        Action::ToggleFullscreen => {
            match app.focus {
                Focus::DiffViewer => app.toggle_diff_fullscreen(),
                Focus::FileList => app.toggle_list_fullscreen(),
                Focus::Terminal => app.toggle_terminal_fullscreen(),
            }
            Some(KeyOutcome::Continue)
        }
        Action::ToggleLogView => {
            app.toggle_mode();
            Some(KeyOutcome::Continue)
        }
        Action::CycleTheme => {
            app.cycle_accent();
            Some(KeyOutcome::Continue)
        }
        Action::SwitchPane(n) => {
            app.switch_pane(n);
            Some(KeyOutcome::Continue)
        }
        Action::FocusList => {
            app.focus_list();
            Some(KeyOutcome::Continue)
        }
        Action::FocusDiff => {
            app.focus_diff();
            Some(KeyOutcome::Continue)
        }
        Action::CycleForward => {
            app.cycle_focus_forward();
            Some(KeyOutcome::Continue)
        }
        Action::CycleBackward => {
            app.cycle_focus_backward();
            Some(KeyOutcome::Continue)
        }
        _ => None,
    }
}

fn has_command_modifier(key: KeyEvent) -> bool {
    key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    )
}

fn text_input_char(key: KeyEvent) -> Option<char> {
    if has_command_modifier(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) if !c.is_control() => Some(c),
        _ => None,
    }
}

fn matches_text_command(key: KeyEvent, expected: char) -> bool {
    !has_command_modifier(key) && matches!(key.code, KeyCode::Char(c) if c == expected)
}

fn handle_repo_input_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_repo_input(),
        KeyCode::Enter => app.confirm_repo_input(),
        KeyCode::Backspace => {
            if app.repo_input.buf.is_empty() {
                app.cancel_repo_input();
            } else {
                app.repo_input_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.repo_input_push(c);
            }
        }
    }
}

fn handle_terminal_key(app: &mut App, key: KeyEvent, action: Action) {
    match action {
        Action::TermScrollUp => {
            let lines = app.terminal.size.0 as usize;
            app.terminal.scroll_up(lines);
        }
        Action::TermScrollDown => {
            let lines = app.terminal.size.0 as usize;
            app.terminal.scroll_down(lines);
        }
        Action::TermScrollLineUp => app.terminal.scroll_up(3),
        Action::TermScrollLineDown => app.terminal.scroll_down(3),
        _ => {
            if let Some(data) = encode_key(key) {
                app.terminal.send_input(&data);
            }
        }
    }
}

fn handle_upper_key(app: &mut App, key: KeyEvent, action: Action) {
    if app.focus == Focus::FileList && app.status_view.search_active {
        handle_file_search_key(app, key);
        return;
    }
    if app.focus == Focus::FileList
        && (app.log_view.commit_search_active || app.log_view.file_search_active)
    {
        handle_log_search_key(app, key);
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        handle_diff_search_key(app, key);
        return;
    }

    // Apply vim-style j/k navigation only in upper panes; terminal focus is
    // routed through handle_terminal_key so j/k reach the PTY untouched.
    let action = vim_navigation_action(key).unwrap_or(action);

    match action {
        Action::Up => app.select_up(),
        Action::Down => app.select_down(),
        Action::PageUp => app.page_up(),
        Action::PageDown => app.page_down(),
        Action::TermScrollUp
        | Action::TermScrollDown
        | Action::TermScrollLineUp
        | Action::TermScrollLineDown => {}
        Action::None => handle_unmapped_upper_key(app, key),
        _ => {}
    }
}

fn handle_file_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.confirm_search(),
        KeyCode::Backspace => {
            if app.status_view.search_query.is_empty() {
                app.cancel_search();
            } else {
                app.search_pop();
            }
        }
        _ => {
            // Reject command chords: Ctrl+letter reaches crossterm as the
            // literal letter, not as a control char, so modifier state is the
            // reliable guard against polluting the query.
            if let Some(c) = text_input_char(key) {
                app.search_push(c);
            }
        }
    }
}

fn handle_log_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_log_search(),
        KeyCode::Enter => app.confirm_log_search(),
        KeyCode::Backspace => {
            // Which query is active depends on whether the drill-down file
            // list is showing; mirror the dispatch used by `log_search_push`.
            let query_empty = if app.log_view.drill_down {
                app.log_view.file_search_query.is_empty()
            } else {
                app.log_view.commit_search_query.is_empty()
            };
            if query_empty {
                app.cancel_log_search();
            } else {
                app.log_search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.log_search_push(c);
            }
        }
    }
}

fn handle_diff_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.diff.cancel_search(),
        KeyCode::Enter => app.diff.confirm_search(),
        KeyCode::Backspace => {
            if app.diff.search.query.is_empty() {
                app.diff.cancel_search();
            } else {
                app.diff.search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.diff.search_push(c);
            }
        }
    }
}

fn handle_unmapped_upper_key(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::FileList => match key.code {
            KeyCode::Enter if app.mode == ViewMode::Log && !app.log_view.drill_down => {
                app.log_drill_in()
            }
            // Log search Esc precedence sits ahead of `log_drill_out` so the
            // first Esc clears a confirmed filter before a second Esc exits
            // drill-down — mirrors the status-search Esc rule below.
            KeyCode::Esc
                if app.mode == ViewMode::Log
                    && app.log_view.drill_down
                    && !app.log_view.file_search_query.is_empty() =>
            {
                app.cancel_log_search()
            }
            KeyCode::Esc
                if app.mode == ViewMode::Log
                    && !app.log_view.drill_down
                    && !app.log_view.commit_search_query.is_empty() =>
            {
                app.cancel_log_search()
            }
            KeyCode::Esc if app.log_view.drill_down => app.log_drill_out(),
            _ if app.mode == ViewMode::Status && matches_text_command(key, '/') => {
                app.start_search()
            }
            _ if app.mode == ViewMode::Log && matches_text_command(key, '/') => {
                app.start_log_search()
            }
            KeyCode::Esc if !app.status_view.search_query.is_empty() => app.cancel_search(),
            KeyCode::Left => app.file_scroll_left(),
            KeyCode::Right => app.file_scroll_right(),
            _ => {}
        },
        Focus::DiffViewer => match key.code {
            _ if matches_text_command(key, 'v') => app.toggle_diff_file_view(),
            _ if matches_text_command(key, '/') => app.diff.start_search(),
            _ if matches_text_command(key, 'n') => app.diff.next_match(),
            _ if matches_text_command(key, 'N') => app.diff.prev_match(),
            KeyCode::Esc if !app.diff.search.query.is_empty() => app.diff.cancel_search(),
            KeyCode::Left => app.diff.scroll_left(),
            KeyCode::Right => app.diff.scroll_right(),
            _ => {}
        },
        Focus::Terminal => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DiffPaneView;
    use crate::app::tests::{app_with_fake_backend, app_with_files};
    use crossterm::event::KeyModifiers;

    fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    /// The default leader chord (Ctrl+G). Test apps all use the default, so a
    /// standalone constructor avoids borrowing `app` inside a `handle_key`
    /// call (which would conflict with the `&mut app` argument).
    fn leader() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
    }

    /// Snapshot the byte payloads the app's `FakeBackend` recorded so terminal
    /// pass-through and literal-leader tests can assert exact PTY bytes.
    fn backend_payloads(app: &App) -> Vec<Vec<u8>> {
        app.terminal
            .fake_backend_sent()
            .expect("test app must use a FakeBackend")
    }

    /// A FakeBackend-backed app with one open terminal pane and terminal
    /// focus, ready for PTY pass-through assertions.
    fn app_with_terminal_pane() -> App {
        let mut app = app_with_fake_backend();
        app.terminal.create_pane().unwrap();
        app.focus = Focus::Terminal;
        app
    }

    #[test]
    fn handle_key_ignores_release_events() {
        // Regression for 4faacce: Windows / kitty keyboard protocol emits
        // Press+Release pairs for every keystroke. Only Press must trigger
        // app mutations; a Release must never act.
        let mut app = app_with_files(vec!["a.rs"]);
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
            crossterm::event::KeyEventKind::Release,
        );

        let outcome = handle_key(&mut app, release);

        assert!(matches!(outcome, KeyOutcome::Continue));
    }

    #[test]
    fn handle_key_leader_then_q_quits() {
        let mut app = app_with_files(vec!["a.rs"]);

        let first = handle_key(&mut app, leader());
        assert!(matches!(first, KeyOutcome::Continue));
        assert!(app.prefix_armed(), "leader must arm the prefix");

        let second = handle_key(&mut app, press(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(second, KeyOutcome::Quit));
        assert!(!app.prefix_armed(), "prefix must disarm after follow-up");
    }

    #[test]
    fn handle_key_bare_ctrl_q_does_not_quit() {
        // Ctrl+Q is no longer an app command; in terminal focus it passes
        // through to the PTY, never quitting nightcrow.
        let mut app = app_with_terminal_pane();

        let outcome = handle_key(&mut app, press(KeyCode::Char('q'), KeyModifiers::CONTROL));

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(!app.prefix_armed());
    }

    #[test]
    fn handle_key_leader_esc_cancels() {
        let mut app = app_with_files(vec!["a.rs"]);
        let _ = handle_key(&mut app, leader());
        assert!(app.prefix_armed());

        let outcome = handle_key(&mut app, press(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(!app.prefix_armed(), "Esc must cancel the armed prefix");
    }

    #[test]
    fn handle_key_leader_ctrl_c_cancels() {
        let mut app = app_with_terminal_pane();
        let _ = handle_key(&mut app, leader());
        assert!(app.prefix_armed());

        let outcome = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(!app.prefix_armed(), "Ctrl+C must cancel the armed prefix");
        // The cancel is consumed, never leaked to the PTY.
        assert!(
            backend_payloads(&app).is_empty(),
            "Ctrl+C cancel must not send bytes to the PTY"
        );
    }

    #[test]
    fn handle_key_ctrl_alt_leader_passes_through() {
        // Ctrl+Alt+<leader> carries an extra modifier, so it is NOT the leader
        // chord — it must reach the PTY rather than arm the prefix.
        let mut app = app_with_terminal_pane();

        let outcome = handle_key(
            &mut app,
            press(KeyCode::Char('g'), KeyModifiers::CONTROL | KeyModifiers::ALT),
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(
            !app.prefix_armed(),
            "Ctrl+Alt+leader must not arm the prefix"
        );
        assert!(
            !backend_payloads(&app).is_empty(),
            "Ctrl+Alt+leader must pass through to the PTY"
        );
    }

    #[test]
    fn paste_while_prefix_armed_cancels_prefix() {
        let mut app = app_with_terminal_pane();
        let _ = handle_key(&mut app, leader());
        assert!(app.prefix_armed());

        handle_paste(&mut app, "hello");

        assert!(
            !app.prefix_armed(),
            "a paste must resolve (cancel) the armed prefix"
        );
    }

    #[test]
    fn leader_leader_sends_literal_leader_even_when_leader_is_ctrl_c() {
        // With a `ctrl+c` leader, `<leader><leader>` must still reach the PTY
        // as a literal Ctrl+C (0x03); the leader-again path takes precedence
        // over the Ctrl+C cancel path.
        let mut app = app_with_terminal_pane();
        app.leader = press(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let _ = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.prefix_armed());

        let outcome = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(!app.prefix_armed());
        assert_eq!(
            backend_payloads(&app).concat(),
            vec![0x03],
            "<leader><leader> must deliver a literal Ctrl+C to the PTY"
        );
    }

    #[test]
    fn terminal_paste_wraps_only_when_bracketed_mode_enabled() {
        let mut app = app_with_terminal_pane();
        // The running program enables bracketed paste (DECSET 2004).
        for parser in app.terminal.parsers.values_mut() {
            parser.process(b"\x1b[?2004h");
        }

        handle_paste(&mut app, "hi");

        assert_eq!(
            backend_payloads(&app).concat(),
            b"\x1b[200~hi\x1b[201~".to_vec(),
            "paste must be bracketed when the program enabled DECSET 2004"
        );
    }

    #[test]
    fn terminal_paste_sends_raw_when_bracketed_mode_disabled() {
        let mut app = app_with_terminal_pane();

        handle_paste(&mut app, "hi");

        assert_eq!(
            backend_payloads(&app).concat(),
            b"hi".to_vec(),
            "without DECSET 2004 the markers must not be sent as literal input"
        );
    }

    #[test]
    fn handle_key_leader_unmapped_followup_cancels() {
        let mut app = app_with_terminal_pane();
        let _ = handle_key(&mut app, leader());
        assert!(app.prefix_armed());

        let outcome = handle_key(&mut app, press(KeyCode::Char('z'), KeyModifiers::NONE));
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(!app.prefix_armed());
        // The unmapped follow-up is consumed, NOT forwarded to the PTY.
        assert!(
            backend_payloads(&app).is_empty(),
            "unmapped follow-up must be dropped, not sent to the PTY"
        );
    }

    #[test]
    fn handle_key_double_leader_sends_literal_to_pty() {
        let mut app = app_with_terminal_pane();
        let _ = handle_key(&mut app, leader());
        assert!(app.prefix_armed());

        let outcome = handle_key(&mut app, leader());
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(!app.prefix_armed());
        // Ctrl+G encodes to 0x07 (BEL) — the literal leader byte.
        assert_eq!(backend_payloads(&app), vec![vec![0x07]]);
    }

    #[test]
    fn handle_key_leader_t_opens_pane() {
        let mut app = app_with_terminal_pane();
        let before = app.terminal.panes.len();
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(app.terminal.panes.len(), before + 1);
    }

    #[test]
    fn handle_key_leader_l_toggles_log_view_from_upper_focus() {
        // Leader commands work in upper (file list) focus too, not just
        // terminal focus.
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;
        let before = app.mode;
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_ne!(
            app.mode, before,
            "leader+l must toggle the view in upper focus"
        );
    }

    #[test]
    fn handle_key_leader_digit_is_unmapped() {
        // Pane jumps moved entirely to the no-prefix F3-F9 keys. A digit after
        // the leader is unmapped, so it must not switch panes; the dispatcher
        // consumes it (disarming the prefix) instead of forwarding it to the PTY.
        let mut app = app_with_terminal_pane();
        app.terminal
            .create_pane_with(Some("echo two"), Some("two"))
            .unwrap();
        assert_eq!(app.terminal.panes.len(), 2);
        app.switch_pane(0);
        assert_eq!(app.terminal.active, 0);

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('2'), KeyModifiers::NONE));

        assert_eq!(app.terminal.active, 0, "leader+digit must not switch panes");
        assert!(
            !app.prefix_armed(),
            "unmapped follow-up must disarm the prefix"
        );
        assert!(
            backend_payloads(&app).is_empty(),
            "a consumed leader digit must not reach the PTY"
        );
    }

    #[test]
    fn handle_key_terminal_ctrl_w_passes_through_to_pty() {
        // Ctrl+W (and friends) are prompt-editing keys that must now reach
        // the running program as control bytes instead of closing the pane.
        let mut app = app_with_terminal_pane();

        let _ = handle_key(&mut app, press(KeyCode::Char('w'), KeyModifiers::CONTROL));

        // Ctrl+W encodes to 0x17 (ETB).
        assert_eq!(backend_payloads(&app), vec![vec![0x17]]);
    }

    #[test]
    fn handle_key_terminal_ctrl_app_keys_all_pass_through() {
        // Every former bare-Ctrl app shortcut now reaches the PTY untouched.
        for (c, byte) in [
            ('q', 0x11u8),
            ('t', 0x14),
            ('w', 0x17),
            ('f', 0x06),
            ('l', 0x0c),
            ('p', 0x10),
            ('o', 0x0f),
        ] {
            let mut app = app_with_terminal_pane();
            let _ = handle_key(&mut app, press(KeyCode::Char(c), KeyModifiers::CONTROL));
            assert_eq!(
                backend_payloads(&app),
                vec![vec![byte]],
                "ctrl+{c} must pass through to the PTY"
            );
        }
    }

    #[test]
    fn handle_key_overlay_blocks_leader_when_diff_search_active() {
        // While a search overlay is open the leader is typed/consumed by the
        // overlay, never arming the prefix or firing an app command.
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();
        assert!(app.diff.search.active);
        let before = app.mode;

        let _ = handle_key(&mut app, leader());
        assert!(!app.prefix_armed(), "leader must not arm behind an overlay");
        let _ = handle_key(&mut app, press(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(
            app.mode, before,
            "no app command may fire behind an overlay"
        );
        assert!(app.diff.search.active, "diff search must remain open");
    }

    #[test]
    fn handle_key_overlay_blocks_leader_when_repo_input_active() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.start_repo_input();
        app.repo_input.buf.clear();
        assert!(app.repo_input.active);

        let _ = handle_key(&mut app, leader());
        assert!(!app.prefix_armed());
        assert!(app.repo_input.active);
    }

    #[test]
    fn handle_key_repo_input_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.start_repo_input();
        app.repo_input.buf.clear();

        let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_x);

        assert!(app.repo_input.buf.is_empty());
    }

    #[test]
    fn handle_key_file_search_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;
        app.start_search();

        let ctrl_x = press(KeyCode::Char('x'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_x);

        assert!(app.status_view.search_query.is_empty());
    }

    #[test]
    fn handle_key_diff_search_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();

        let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_x);

        assert!(app.diff.search.query.is_empty());
    }

    #[test]
    fn handle_key_status_search_shortcut_requires_no_command_modifier() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;

        let ctrl_slash = press(KeyCode::Char('/'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_slash);

        assert!(!app.status_view.search_active);
    }

    #[test]
    fn handle_key_diff_file_toggle_requires_no_command_modifier() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;

        let alt_v = press(KeyCode::Char('v'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_v);

        assert_eq!(app.diff.view, DiffPaneView::Diff);
    }

    #[test]
    fn handle_paste_into_file_search_strips_control_chars() {
        // Regression for e21c449 + 4084760: paste into the file-search
        // overlay drops control characters (newlines, tabs, bells) before
        // appending to the query.
        let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
        app.focus = Focus::FileList;
        app.start_search();

        handle_paste(&mut app, "al\nph\ta\x07");

        assert_eq!(app.status_view.search_query.as_str(), "alpha");
    }

    #[test]
    fn handle_paste_into_diff_search_strips_control_chars() {
        let mut app = app_with_files(vec!["alpha.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();

        handle_paste(&mut app, "fn\rname\x08");

        assert_eq!(app.diff.search.query.as_str(), "fnname");
    }

    #[test]
    fn handle_paste_into_repo_input_strips_control_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.start_repo_input();
        // Pre-existing buffer content is preserved by repo_input_push;
        // start_repo_input copies the current repo_path in, so reset.
        app.repo_input.buf.clear();

        handle_paste(&mut app, "/tmp\n/repo\x07");

        assert_eq!(app.repo_input.buf, "/tmp/repo");
    }
}
