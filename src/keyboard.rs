use crossterm::event::KeyCode;

pub type KeyState = fn(KeyCode) -> Option<bool>;

/// Run explicitly from the player's terminal, whose macOS permissions can
/// differ from the editor or process that built the game.
pub fn setup() {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: These permission APIs take no pointers and do not capture keys.
        let allowed = unsafe { CGPreflightListenEventAccess() || CGRequestListenEventAccess() };
        if allowed {
            println!("Input Monitoring is enabled. Restart the game with ./play.sh.");
        } else {
            println!(
                "Enable your terminal app (or terminalshooter if listed) in System Settings > Privacy & Security > Input Monitoring.\nThen fully quit and reopen the terminal app and run ./play.sh again.\nWithout permission or terminal key-release support, held keys pause until keyboard repeat begins."
            );
        }
    }
    #[cfg(not(target_os = "macos"))]
    println!(
        "Use a terminal with keyboard enhancement/key-release support for continuous movement without repeat delay."
    );
}

/// Only consult the local keyboard for a local terminal session. Input invokes
/// this for game controls that have already arrived through the terminal.
pub fn local_key_state() -> Option<KeyState> {
    #[cfg(target_os = "macos")]
    if ["SSH_CONNECTION", "SSH_TTY", "TMUX", "STY"]
        .iter()
        .all(|name| std::env::var_os(name).is_none())
    {
        // A denied query reports false, indistinguishable from a released key.
        // Make the missing capability explicit instead of pretending to poll it.
        if unsafe { CGPreflightListenEventAccess() } {
            return Some(mac_key_down);
        }
    }
    None
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
}

#[cfg(target_os = "macos")]
fn mac_key_down(code: KeyCode) -> Option<bool> {
    let key = match code {
        KeyCode::Char('a') => 0,
        KeyCode::Char('s') => 1,
        KeyCode::Char('d') => 2,
        KeyCode::Char('q') => 12,
        KeyCode::Char('w') => 13,
        KeyCode::Char('e') => 14,
        KeyCode::Char(' ') => 49,
        KeyCode::Left => 123,
        KeyCode::Right => 124,
        KeyCode::Down => 125,
        KeyCode::Up => 126,
        _ => return None,
    };
    // Check hardware state as well as the login session: terminal delivery and
    // the session state table need not become visible at the same instant.
    // SAFETY: Quartz takes scalar values; 1 is HIDSystemState and 0 is
    // combinedSessionState. Neither query installs an event tap.
    Some(unsafe { CGEventSourceKeyState(1, key) || CGEventSourceKeyState(0, key) })
}
