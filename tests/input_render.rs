use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use terminalshooter::{
    game::{Controls, FINAL_WAVE, Game, Phase, Pickup, PickupKind},
    input::Input,
    render::{self, BG, ColorMode, Frame, GREEN, Stats, TerminalRenderer, WHITE},
    world::{self, Vec2},
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
fn physical_state_catching_up_to_terminal_press_has_no_repeat_delay() {
    for (letter, bit) in [('w', 1), ('a', 2), ('s', 4), ('d', 8), ('q', 16), ('e', 32)] {
        let mut game = Game::new(1);
        game.start();
        let mut input = Input::with_key_state(1, false, Some(physical_key));
        PHYSICAL_KEYS.with(|keys| keys.set(0));
        input.handle(key(letter, KeyEventKind::Press), 0.0, &mut game);
        PHYSICAL_KEYS.with(|keys| keys.set(bit));
        // No repeated terminal events throughout the initial OS repeat delay.
        for tick in 1..=120 {
            let controls = input.controls(tick as f64 / 120.0);
            assert!(controls.direction(0.0).length() + controls.turn.abs() > 0.9);
        }
        PHYSICAL_KEYS.with(|keys| keys.set(0));
        let controls = input.controls(1.001);
        assert_eq!(controls.direction(0.0).length() + controls.turn.abs(), 0.0);
    }
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
fn pointer_resting_at_a_window_edge_keeps_turning_until_moved_inward() {
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::new(1, true);
    input.screen_width = 100;
    input.handle(motion(50, 15), 0.0, &mut game);
    assert_eq!(input.controls(0.0).turn, 0.0);
    // No further reports arrive while the pointer rests at the edge.
    input.handle(motion(99, 15), 0.1, &mut game);
    let angle = game.player.angle;
    for _ in 0..60 {
        game.tick(1.0 / 120.0, input.take_controls(0.1));
    }
    let turned = world::normalize_angle(game.player.angle - angle);
    assert!((turned - 1.1).abs() < 0.01, "{turned}");
    // The zone ramps up toward the edge on both sides.
    input.handle(motion(96, 15), 0.2, &mut game);
    assert!((input.controls(0.2).turn - 0.4).abs() < 1e-6);
    input.handle(motion(4, 15), 0.3, &mut game);
    assert!((input.controls(0.3).turn + 0.2).abs() < 1e-6);
    input.handle(motion(0, 15), 0.4, &mut game);
    assert_eq!(input.controls(0.4).turn, -1.0);
    input.handle(motion(50, 15), 0.5, &mut game);
    assert_eq!(input.controls(0.5).turn, 0.0);
}

#[test]
fn edge_turning_respects_mouse_look_focus_and_keyboard_turns() {
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::new(1, true);
    input.handle(motion(0, 15), 0.0, &mut game);
    assert_eq!(input.controls(0.0).turn, 0.0, "unknown width has no zone");
    input.screen_width = 100;
    input.handle(motion(99, 15), 0.1, &mut game);
    input.handle(key('e', KeyEventKind::Press), 0.1, &mut game);
    assert_eq!(input.controls(0.1).turn, 1.0);
    input.handle(key('e', KeyEventKind::Release), 0.2, &mut game);
    input.handle(key('q', KeyEventKind::Press), 0.2, &mut game);
    assert_eq!(input.controls(0.2).turn, 0.0);
    input.handle(key('q', KeyEventKind::Release), 0.3, &mut game);
    // L turns edge turning off with the rest of mouse-look.
    input.handle(key('l', KeyEventKind::Press), 0.3, &mut game);
    assert_eq!(input.controls(0.3).turn, 0.0);
    input.handle(motion(99, 15), 0.4, &mut game);
    assert_eq!(input.controls(0.4).turn, 0.0);
    input.handle(key('l', KeyEventKind::Press), 1.0, &mut game);
    input.handle(motion(99, 15), 1.1, &mut game);
    assert_eq!(input.controls(1.1).turn, 1.0);
    // Resuming after focus loss waits for a fresh pointer report.
    input.handle(Event::FocusLost, 1.2, &mut game);
    input.handle(key('p', KeyEventKind::Press), 1.3, &mut game);
    assert_eq!(game.phase, Phase::Playing);
    assert_eq!(input.controls(1.3).turn, 0.0);
}

#[test]
fn pixel_mouse_reports_aim_in_fractions_of_a_cell() {
    let mut game = Game::new(1);
    game.start();
    let mut input = Input::new(1, true);
    input.screen_width = 100;
    input.cell_size = Some((10.0, 20.0));
    input.handle(motion(500, 300), 0.0, &mut game);
    let angle = game.player.angle;
    // Three pixels is less than one cell, which cell reports could not show.
    input.handle(motion(503, 300), 0.1, &mut game);
    let turned = game.player.angle - angle;
    assert!((turned - 0.3 * game.sensitivity).abs() < 1e-6, "{turned}");
    assert_eq!(input.controls(0.1).turn, 0.0);
    // Edge turning uses the containing cell: pixel 995 is in column 99.
    input.handle(motion(995, 300), 0.2, &mut game);
    assert_eq!(input.controls(0.2).turn, 1.0);
    input.handle(motion(945, 300), 0.3, &mut game);
    assert_eq!(input.controls(0.3).turn, 0.0);
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
fn hud_and_end_screen_count_waves_up_to_final_wave() {
    let mut game = Game::new(1);
    game.start();
    let progress = format!("WAVE 1/{FINAL_WAVE}");
    assert!(
        render::draw(&game, 120, 36, Stats::default())
            .plain()
            .contains(&progress)
    );
    game.phase = Phase::Lost;
    assert!(
        render::draw(&game, 120, 36, Stats::default())
            .plain()
            .contains(&progress)
    );
}

#[test]
fn tactical_map_shows_pickups_and_menus_show_the_best_score() {
    let mut game = Game::new(1);
    game.start();
    // Enemies are drawn over pickups; keep them off these cells.
    game.enemies.clear();
    game.pickups.push(Pickup {
        pos: Vec2::new(16.5, 2.5),
        kind: PickupKind::Ammo,
    });
    game.phase = Phase::Map;
    let text = render::draw(&game, 80, 30, Stats::default()).plain();
    let rows: Vec<Vec<char>> = text.lines().map(|row| row.chars().collect()).collect();
    // World row 0 is drawn on screen row 4, centred horizontally.
    let x = (80 - world::WIDTH) / 2;
    assert_eq!(rows[4 + 7][x + 2], '+');
    assert_eq!(rows[4 + 7][x + 16], '+');
    assert_eq!(rows[4 + 2][x + 16], '*');
    assert!(text.contains("+ MEDKIT  * AMMO"));

    game.best = 4210;
    game.phase = Phase::Title;
    for (w, h) in [(40, 16), (80, 24), (120, 36)] {
        let text = render::draw(&game, w, h, Stats::default()).plain();
        assert!(text.contains("BEST SCORE 4210"), "{w}x{h}");
    }
    game.phase = Phase::Won;
    game.new_best = true;
    let text = render::draw(&game, 40, 16, Stats::default()).plain();
    assert!(text.contains("NEW BEST SCORE"));
    assert!(text.contains("C: continue in endless mode"));
}

#[test]
fn won_screen_c_continues_into_endless_mode() {
    let mut game = Game::new(1);
    game.phase = Phase::Won;
    game.wave = FINAL_WAVE;
    game.enemies.clear();
    let mut input = Input::new(1, true);
    input.handle(key('c', KeyEventKind::Press), 0.0, &mut game);
    assert_eq!(game.phase, Phase::Playing);
    assert!(game.endless);
    let text = render::draw(&game, 100, 30, Stats::default()).plain();
    assert!(text.contains("WAVE CLEAR // NEXT IN 4"));
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
fn detailed_weapon_keeps_crosshair_and_hud_clear_at_all_sizes() {
    for (w, h) in [(40, 16), (120, 36), (160, 50), (220, 70)] {
        for reloading in [false, true] {
            let mut game = Game::new(1);
            game.start();
            game.player.reload = if reloading { 0.4 } else { 0.0 };
            let frame = render::draw(&game, w, h, Stats::default());
            let cross_y = h - 4;
            let cross = frame.cells[cross_y / 2 * w + w / 2];
            assert_eq!(if cross_y % 2 == 0 { cross.fg } else { cross.bg }, GREEN);
            assert!(frame.plain().lines().nth(h - 4).unwrap().contains("HP 100"));
            assert!(
                frame.cells[(h - 4) * w..]
                    .iter()
                    .all(|cell| cell.glyph != '▀')
            );
        }
    }
}

#[test]
fn compact_minimap_keeps_the_aiming_point_clear() {
    let mut game = Game::new(1);
    game.start();
    let frame = render::draw(&game, 40, 16, Stats::default());
    assert_eq!(frame.cells[6 * 40 + 20].fg, GREEN);
    // The cropped map still marks the player's containing cell.
    assert_eq!(frame.cells[4 * 40 + 6].fg, GREEN);
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

/// Minimal terminal that applies renderer output to a grid of visible cells.
/// A space's foreground is invisible, so it is not recorded.
struct Screen {
    width: usize,
    cells: Vec<(char, Vec<u32>, Vec<u32>)>,
}
impl Screen {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            cells: vec![(' ', vec![], vec![]); width * height],
        }
    }
    fn apply(&mut self, output: &str) {
        let (mut x, mut y) = (0, 0);
        let (mut fg, mut bg) = (vec![], vec![]);
        let mut chars = output.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\x1b' => {
                    assert_eq!(chars.next(), Some('['));
                    let mut params = String::new();
                    let end = chars.by_ref().find(|&p| {
                        let done = ('@'..='~').contains(&p);
                        if !done {
                            params.push(p);
                        }
                        done
                    });
                    let numbers: Vec<u32> =
                        params.split(';').filter_map(|n| n.parse().ok()).collect();
                    match end {
                        Some('H') => (y, x) = (numbers[0] as usize - 1, numbers[1] as usize - 1),
                        Some('C') => x += numbers.first().copied().unwrap_or(1) as usize,
                        Some('J') => self.cells.fill((' ', vec![], vec![])),
                        Some('m') => {
                            let mut rest = &numbers[..];
                            while let [first, tail @ ..] = rest {
                                let length = if tail.first() == Some(&5) { 2 } else { 4 };
                                match first {
                                    0 => (fg, bg) = (vec![], vec![]),
                                    38 => fg = tail[..length].to_vec(),
                                    48 => bg = tail[..length].to_vec(),
                                    other => panic!("unexpected SGR {other}"),
                                }
                                rest = if *first == 0 { tail } else { &tail[length..] };
                            }
                        }
                        Some('h' | 'l') => {}
                        other => panic!("unexpected sequence {other:?}"),
                    }
                }
                '\r' => x = 0,
                '\n' => y += 1,
                glyph => {
                    let visible_fg = if glyph == ' ' { vec![] } else { fg.clone() };
                    self.cells[y * self.width + x] = (glyph, visible_fg, bg.clone());
                    // Line wrap is disabled: the last column keeps the cursor.
                    x = (x + 1).min(self.width - 1);
                }
            }
        }
    }
}

#[test]
fn incremental_updates_reproduce_the_frame_and_a_full_repaint() {
    for mode in [ColorMode::Palette, ColorMode::TrueColor] {
        for (w, h) in [(40, 16), (100, 30), (160, 50)] {
            let mut game = Game::new(3);
            game.start();
            let mut renderer = TerminalRenderer::new(mode);
            let mut screen = Screen::new(w, h);
            for step in 0..40 {
                game.player.hp = 100;
                game.phase = if (20..25).contains(&step) {
                    Phase::Paused
                } else {
                    Phase::Playing
                };
                game.tick(
                    1.0 / 60.0,
                    Controls {
                        turn: 0.6,
                        forward: 0.3,
                        ..Controls::default()
                    },
                );
                let frame = render::draw(&game, w, h, Stats::default());
                screen.apply(&renderer.encode(&frame));
                let mut fresh = Screen::new(w, h);
                fresh.apply(&TerminalRenderer::new(mode).encode(&frame));
                assert!(
                    screen.cells == fresh.cells,
                    "{mode:?} {w}x{h} step {step}: incremental output diverged"
                );
                for (cell, (glyph, _, bg)) in frame.cells.iter().zip(&screen.cells) {
                    // A half block with matching halves may be sent as a space.
                    assert!(*glyph == cell.glyph || (*glyph == ' ' && cell.glyph == '▀'));
                    if mode == ColorMode::TrueColor {
                        let expected = [cell.bg.0, cell.bg.1, cell.bg.2];
                        assert_eq!(bg[0], 2);
                        for (got, want) in bg[1..].iter().zip(expected) {
                            assert!(got.abs_diff(u32::from(want)) <= 2);
                        }
                    }
                }
            }
        }
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
