use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use terminalshooter::{
    game::{Game, Phase},
    input::Input,
    render::{self, BG, ColorMode, Frame, GREEN, Stats, TerminalRenderer, WHITE},
    world::Vec2,
};

fn key(c: char, kind: KeyEventKind) -> Event {
    Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char(c),
        KeyModifiers::NONE,
        kind,
    ))
}
fn motion(x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

fn click(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 50,
        row: 15,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn quick_click_or_space_tap_between_ticks_fires_exactly_once() {
    for (down, up) in [
        (
            click(MouseEventKind::Down(MouseButton::Left)),
            click(MouseEventKind::Up(MouseButton::Left)),
        ),
        (
            key(' ', KeyEventKind::Press),
            key(' ', KeyEventKind::Release),
        ),
    ] {
        let mut game = Game::new(1);
        game.start();
        let mut input = Input::new(1, true);
        input.handle(down, 0.001, &mut game);
        input.handle(up, 0.002, &mut game);
        // Dodge reads the current controls too; it must not consume the click.
        input.handle(key('x', KeyEventKind::Press), 0.003, &mut game);
        for tick in 1..=120 {
            game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
        }
        assert_eq!(game.shots_fired, 1);
        assert_eq!(game.player.shells, 5);
    }
}

#[test]
fn clicks_respect_cooldown_and_are_cleared_on_focus_loss() {
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::new(1, false);
    assert!(game.shoot());
    input.handle(
        click(MouseEventKind::Down(MouseButton::Left)),
        0.001,
        &mut game,
    );
    input.handle(
        click(MouseEventKind::Up(MouseButton::Left)),
        0.002,
        &mut game,
    );
    for tick in 1..=120 {
        game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
    }
    assert_eq!(
        game.shots_fired, 1,
        "cooldown must not be bypassed or queue a later shot"
    );
    input.handle(
        click(MouseEventKind::Down(MouseButton::Left)),
        1.1,
        &mut game,
    );
    assert_eq!(game.shots_fired, 2, "the second press fires immediately");
    input.handle(Event::FocusLost, 1.2, &mut game);
    input.handle(key('p', KeyEventKind::Press), 1.3, &mut game);
    game.tick(1.0 / 120.0, input.take_controls(1.4));
    assert_eq!(game.shots_fired, 2, "resuming must discard old holds");
}

#[test]
fn mouse_down_fires_before_any_tick_and_mouse_up_never_fires() {
    for from_title in [false, true] {
        let mut game = Game::new(1);
        if !from_title {
            game.start();
        }
        let mut input = Input::new(1, false);
        input.handle(
            click(MouseEventKind::Down(MouseButton::Left)),
            0.0,
            &mut game,
        );
        assert_eq!(game.shots_fired, 1);
        assert_eq!(game.player.shells, 5);
        assert!(game.muzzle > 0.0);
        // Expire the weapon cooldown without sampling held input, so a
        // mistakenly release-triggered shot cannot hide behind the cooldown.
        for _ in 0..60 {
            game.tick(1.0 / 120.0, Default::default());
        }
        input.handle(click(MouseEventKind::Up(MouseButton::Left)), 0.5, &mut game);
        game.tick(1.0 / 120.0, input.take_controls(0.51));
        assert_eq!(game.shots_fired, 1);
    }
}

#[test]
fn holding_mouse_continues_firing_until_release() {
    let mut game = Game::new(1);
    // Begin by holding the click that deploys from the title screen.
    let mut input = Input::new(1, false);
    input.handle(
        click(MouseEventKind::Down(MouseButton::Left)),
        0.0,
        &mut game,
    );
    for tick in 1..=480 {
        if tick == 120 {
            input.handle(key('l', KeyEventKind::Press), 1.0, &mut game);
        }
        game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
    }
    assert!(
        game.shots_fired >= 8,
        "holding must continue through an automatic reload"
    );
    assert!(game.player.reserve < 48);
    let shots = game.shots_fired;
    input.handle(click(MouseEventKind::Up(MouseButton::Left)), 4.0, &mut game);
    for tick in 481..=600 {
        game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
    }
    assert_eq!(game.shots_fired, shots);
}

#[test]
fn left_drag_recovers_a_hold_and_firing_resumes_after_wave_break() {
    let mut game = Game::new(1);
    game.start();
    game.break_time = 0.02;
    let mut input = Input::new(1, false);
    input.handle(
        click(MouseEventKind::Drag(MouseButton::Left)),
        0.0,
        &mut game,
    );
    for tick in 1..=60 {
        game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
    }
    assert_eq!(game.wave, 2);
    assert_eq!(game.shots_fired, 2);
    input.handle(Event::FocusLost, 0.5, &mut game);
    input.handle(
        click(MouseEventKind::Drag(MouseButton::Left)),
        0.6,
        &mut game,
    );
    input.handle(key('p', KeyEventKind::Press), 0.7, &mut game);
    assert!(!input.take_controls(0.8).fire);
}

#[test]
fn enhanced_keys_stop_on_release_and_fallback_keys_expire() {
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::new(1, true);
    input.handle(key('w', KeyEventKind::Press), 0.0, &mut game);
    assert_eq!(input.controls(10.0).forward, 1.0);
    input.handle(key('w', KeyEventKind::Release), 10.0, &mut game);
    assert_eq!(input.controls(10.0).forward, 0.0);
    let mut input = Input::new(1, false);
    input.handle(key('w', KeyEventKind::Press), 0.0, &mut game);
    assert_eq!(input.controls(0.02).forward, 1.0);
    assert_eq!(input.controls(0.06).forward, 0.0);
}

thread_local! {
    static PHYSICAL_KEYS: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

fn physical_key(code: KeyCode) -> Option<bool> {
    let bit = match code {
        KeyCode::Char('w') => 1,
        KeyCode::Char('a') => 2,
        KeyCode::Char('s') => 4,
        KeyCode::Char('d') => 8,
        KeyCode::Char('q') => 16,
        KeyCode::Char('e') => 32,
        _ => return None,
    };
    Some(PHYSICAL_KEYS.with(|keys| keys.get() & bit != 0))
}

#[test]
fn physical_holds_work_without_repeats_and_stop_independently_on_release() {
    for (letter, bit) in [('w', 1), ('a', 2), ('s', 4), ('d', 8), ('q', 16), ('e', 32)] {
        let mut game = Game::new(1);
        game.start();
        let mut input = Input::with_key_state(1, false, Some(physical_key));
        PHYSICAL_KEYS.with(|keys| keys.set(bit));
        input.handle(key(letter, KeyEventKind::Press), 0.0, &mut game);
        let controls = input.controls(10.0);
        assert!(controls.direction(0.0).length() + controls.turn.abs() > 0.9);
        PHYSICAL_KEYS.with(|keys| keys.set(0));
        assert_eq!(input.controls(10.001).direction(0.0).length(), 0.0);
        assert_eq!(input.controls(10.001).turn, 0.0);
    }
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::with_key_state(1, false, Some(physical_key));
    PHYSICAL_KEYS.with(|keys| keys.set(9));
    input.handle(key('w', KeyEventKind::Press), 0.0, &mut game);
    input.handle(key('d', KeyEventKind::Press), 0.01, &mut game);
    assert_eq!(input.controls(10.0).forward, 1.0);
    assert_eq!(input.controls(10.0).strafe, 1.0);
    PHYSICAL_KEYS.with(|keys| keys.set(8));
    assert_eq!(input.controls(10.01).forward, 0.0);
    assert_eq!(input.controls(10.01).strafe, 1.0);
    input.handle(Event::FocusLost, 10.02, &mut game);
    input.handle(key('p', KeyEventKind::Press), 11.0, &mut game);
    assert_eq!(
        input.controls(11.1).strafe,
        0.0,
        "focus loss must require a fresh terminal press"
    );
    PHYSICAL_KEYS.with(|keys| keys.set(0));
}

#[test]
fn fallback_repeats_sustain_movement_but_never_assume_an_initial_hold() {
    let mut game = Game::new(1);
    game.start();
    // No physical state available: also covers denied native access and SSH.
    let mut input = Input::with_key_state(1, false, Some(|_| None));
    input.handle(key('w', KeyEventKind::Press), 0.0, &mut game);
    for i in 6..50 {
        assert_eq!(input.controls(i as f64 / 100.0).forward, 0.0);
    }
    for i in 10..40 {
        let now = i as f64 * 0.05;
        // Legacy terminal repeats arrive as ordinary Press events.
        input.handle(key('w', KeyEventKind::Press), now, &mut game);
        assert_eq!(input.controls(now + 0.04).forward, 1.0);
        if i > 10 {
            assert_eq!(input.controls(now + 0.08).forward, 1.0);
        }
    }
    assert_eq!(input.controls(2.15).forward, 0.0);
    input.handle(key(' ', KeyEventKind::Press), 3.0, &mut game);
    for i in 1..=120 {
        game.tick(1.0 / 120.0, input.take_controls(3.0 + i as f64 / 120.0));
    }
    assert_eq!(
        game.shots_fired, 1,
        "movement grace must not turn a Space tap into multiple shots"
    );
}

#[test]
fn single_movement_and_turn_taps_are_small_and_do_not_coast() {
    for native_unavailable in [false, true] {
        for letter in ['w', 'a', 's', 'd', 'q', 'e'] {
            let mut game = Game::new(1);
            game.start();
            let start_pos = game.player.pos;
            let start_angle = game.player.angle;
            PHYSICAL_KEYS.with(|keys| keys.set(0));
            let mut input = Input::with_key_state(
                1,
                false,
                if native_unavailable {
                    None
                } else {
                    Some(physical_key)
                },
            );
            input.handle(key(letter, KeyEventKind::Press), 0.0, &mut game);
            for tick in 1..=12 {
                game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
            }
            let distance = (game.player.pos - start_pos).length();
            let rotation = (game.player.angle - start_angle).abs();
            if matches!(letter, 'q' | 'e') {
                assert!(
                    rotation > 0.0 && rotation < 0.12,
                    "{letter}: {rotation} radians"
                );
            } else {
                assert!(
                    distance > 0.0 && distance < 0.2,
                    "{letter}: {distance} cells"
                );
            }
            let stopped = (game.player.pos, game.player.angle);
            for tick in 13..=120 {
                game.tick(1.0 / 120.0, input.take_controls(tick as f64 / 120.0));
            }
            assert_eq!((game.player.pos, game.player.angle), stopped);
        }
    }
}

#[test]
fn repeated_toggle_key_does_not_flicker_state() {
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::new(1, false);
    input.handle(key('f', KeyEventKind::Press), 0.0, &mut game);
    for i in 1..10 {
        input.handle(key('f', KeyEventKind::Repeat), i as f64 / 10.0, &mut game);
    }
    assert!(game.auto_fire);
    input.handle(key('f', KeyEventKind::Press), 2.0, &mut game);
    assert!(!game.auto_fire);
}

#[test]
fn focus_loss_pauses_and_clears_movement_and_firing() {
    let mut game = Game::new(1);
    game.start();
    game.auto_fire = true;
    let mut input = Input::new(1, true);
    input.handle(key('w', KeyEventKind::Press), 0.0, &mut game);
    input.handle(Event::FocusLost, 0.1, &mut game);
    assert_eq!(game.phase, Phase::Paused);
    assert!(!game.auto_fire);
    assert_eq!(input.controls(0.2).forward, 0.0);
    input.handle(key('p', KeyEventKind::Press), 1.0, &mut game);
    assert_eq!(game.phase, Phase::Playing);
}

#[test]
fn mouse_l_toggle_and_motion_work_at_windowed_and_fullscreen_coordinates() {
    for (x, y) in [(40, 12), (180, 55)] {
        let mut game = Game::new(1);
        game.start();
        let mut input = Input::new(1, false);
        let angle = game.player.angle;
        input.handle(motion(x, y), 0.0, &mut game);
        input.handle(motion(x + 5, y), 0.1, &mut game);
        assert!(game.player.angle > angle);
        input.handle(key('l', KeyEventKind::Press), 1.0, &mut game);
        let angle = game.player.angle;
        input.handle(motion(x + 10, y), 1.1, &mut game);
        assert_eq!(game.player.angle, angle);
        input.handle(key('l', KeyEventKind::Press), 2.0, &mut game);
        input.handle(motion(x + 20, y), 2.1, &mut game);
        assert_eq!(
            game.player.angle, angle,
            "re-enabling must reset the baseline"
        );
        input.handle(motion(x + 22, y), 2.2, &mut game);
        assert!(game.player.angle > angle);
    }
}

#[test]
fn title_motion_does_not_start_and_restart_resets_game() {
    let mut game = Game::new(1);
    let mut input = Input::new(1, false);
    input.handle(motion(40, 12), 0.0, &mut game);
    assert_eq!(game.phase, Phase::Title);
    input.handle(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        0.1,
        &mut game,
    );
    assert_eq!(game.phase, Phase::Playing);
    game.phase = Phase::Lost;
    game.player.hp = 0;
    game.mouse = false;
    input.handle(key('r', KeyEventKind::Press), 1.0, &mut game);
    assert_eq!(game.phase, Phase::Playing);
    assert_eq!(game.player.hp, 100);
    assert_eq!(game.wave, 1);
    assert!(!game.mouse);
}

#[test]
fn menus_have_no_world_pixels_or_crosshair_and_fit_all_sizes() {
    for (w, h) in [(40, 16), (120, 36), (220, 70)] {
        for phase in [
            Phase::Title,
            Phase::Paused,
            Phase::Map,
            Phase::Help,
            Phase::Lost,
            Phase::Won,
        ] {
            let mut game = Game::new(1);
            game.phase = phase;
            game.muzzle = 0.1;
            let frame = render::draw(&game, w, h, Stats::default());
            assert!(frame.cells.iter().all(|cell| cell.glyph != '▀'));
            assert_eq!(frame.cells.len(), w * h);
            assert!(frame.plain().lines().all(|line| line.chars().count() == w));
        }
    }
}

#[test]
fn minimap_uses_containing_cell_and_never_rounds_into_cover() {
    let mut game = Game::new(1);
    game.start();
    game.player.pos = Vec2::new(4.9, 3.5);
    game.player.angle = 0.0;
    let frame = render::draw(&game, 80, 30, Stats::default());
    // Origin (2,2), containing cell (4,3) => half-pixel (6,5).
    assert_eq!(frame.cells[2 * 80 + 6].bg, GREEN);
    assert_eq!(frame.cells[2 * 80 + 7].bg, render::MM_WALL);
}

#[test]
fn unchanged_frames_emit_nothing_and_changes_only_update_their_cells() {
    for mode in [ColorMode::Palette, ColorMode::TrueColor] {
        let mut renderer = TerminalRenderer::new(mode);
        let mut frame = Frame::new(40, 16);
        frame.text(5, 5, "MENU", WHITE);
        let full = renderer.encode(&frame);
        assert!(full.contains("\x1b[2J"));
        assert!(renderer.encode(&frame).is_empty());
        frame.text(6, 5, "X", GREEN);
        let delta = renderer.encode(&frame);
        assert!(delta.contains("\x1b[6;7H"));
        assert!(!delta.contains("MENU"));
        assert!(!delta.contains("\x1b[2J"));
        assert!(delta.len() < full.len() / 3);
        assert!(delta.starts_with("\x1b[?2026h") && delta.ends_with("\x1b[?2026l"));
    }
}

#[test]
fn text_is_composited_before_encoding_and_resize_forces_repaint() {
    let mut frame = Frame::new(40, 16);
    frame.pixel(20, 16, GREEN);
    frame.text(20, 8, "P", WHITE);
    assert_eq!(frame.cells[8 * 40 + 20].glyph, 'P');
    assert_eq!(frame.cells[8 * 40 + 20].bg, BG);
    let mut renderer = TerminalRenderer::new(ColorMode::Palette);
    renderer.encode(&frame);
    assert!(renderer.encode(&Frame::new(60, 20)).contains("\x1b[2J"));
}
