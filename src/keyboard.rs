use crossterm::event::KeyCode;

pub type KeyState = fn(KeyCode) -> Option<bool>;

/// Only consult the local keyboard for a local terminal session. Input invokes
/// this for game controls that have already arrived through the terminal.
pub fn local_key_state() -> Option<KeyState> {
    #[cfg(target_os = "macos")]
    if ["SSH_CONNECTION", "SSH_TTY", "TMUX", "STY"]
        .iter()
        .all(|name| std::env::var_os(name).is_none())
    {
        return Some(mac_key_down);
    }
    None
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
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
    // SAFETY: Quartz takes scalar values; 0 is combinedSessionState. This
    // queries a control's current state without installing an event tap.
    Some(unsafe { CGEventSourceKeyState(0, key) })
}
