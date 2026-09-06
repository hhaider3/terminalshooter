use crate::game::{Controls, Game, Phase};
use crate::keyboard::KeyState;
use crate::world;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use std::collections::HashMap;

#[derive(Default)]
pub struct Outcome {
    pub quit: bool,
    pub redraw: bool,
}

struct HeldKey {
    expires: f64,
    shift: bool,
    native: bool,
}

pub struct Input {
    held: HashMap<KeyCode, HeldKey>,
    movement_events: HashMap<KeyCode, f64>,
    actions: HashMap<KeyCode, f64>,
    pub release_events: bool,
    mouse_position: Option<(u16, u16)>,
    left_mouse: bool,
    fire_pressed: bool,
    seed: u64,
    key_state: Option<KeyState>,
}

impl Input {
    pub fn new(seed: u64, release_events: bool) -> Self {
        Self::with_key_state(seed, release_events, None)
    }
    pub fn with_key_state(seed: u64, release_events: bool, key_state: Option<KeyState>) -> Self {
        Self {
            held: HashMap::new(),
            movement_events: HashMap::new(),
            actions: HashMap::new(),
            release_events,
            mouse_position: None,
            left_mouse: false,
            fire_pressed: false,
            seed,
            key_state: if release_events { None } else { key_state },
        }
    }
    pub fn clear(&mut self) {
        self.held.clear();
        self.movement_events.clear();
        self.mouse_position = None;
        self.left_mouse = false;
        self.fire_pressed = false;
    }
    pub fn controls(&mut self, now: f64) -> Controls {
        self.held.retain(|code, held| {
            if held.native {
                return self.key_state.and_then(|read| read(*code)) == Some(true);
            }
            held.expires >= now
        });
        let has = |codes: &[KeyCode]| -> f32 {
            if codes.iter().any(|c| self.held.contains_key(c)) {
                1.0
            } else {
                0.0
            }
        };
        Controls {
            forward: has(&[KeyCode::Char('w'), KeyCode::Up])
                - has(&[KeyCode::Char('s'), KeyCode::Down]),
            strafe: has(&[KeyCode::Char('d')]) - has(&[KeyCode::Char('a')]),
            turn: has(&[KeyCode::Char('e'), KeyCode::Right])
                - has(&[KeyCode::Char('q'), KeyCode::Left]),
            sprint: self
                .held
                .get(&KeyCode::Char('w'))
                .is_some_and(|held| held.shift),
            fire: self.fire_pressed || self.left_mouse || has(&[KeyCode::Char(' ')]) > 0.0,
        }
    }
    /// Consume press edges only when the simulation actually takes a step.
    /// A down/up pair in one event batch must still produce one fire attempt.
    pub fn take_controls(&mut self, now: f64) -> Controls {
        let controls = self.controls(now);
        self.fire_pressed = false;
        controls
    }
    pub fn handle(&mut self, event: Event, now: f64, game: &mut Game) -> Outcome {
        let old_phase = game.phase;
        let deploying_click = old_phase == Phase::Title
            && matches!(&event, Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left));
        let mut outcome = Outcome::default();
        match event {
            Event::FocusLost => {
                if game.phase == Phase::Playing {
                    game.overlay(Phase::Paused);
                }
                game.auto_fire = false;
                self.clear();
            }
            Event::Resize(_, _) => {
                self.clear();
                game.auto_fire = false;
                outcome.redraw = true;
            }
            Event::Mouse(mouse) => {
                let pos = (mouse.column, mouse.row);
                match mouse.kind {
                    MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                        if mouse.kind == MouseEventKind::Drag(MouseButton::Left)
                            && game.phase == Phase::Playing
                        {
                            self.left_mouse = true;
                        }
                        if game.phase == Phase::Playing
                            && game.mouse
                            && let Some(old) = self.mouse_position
                        {
                            let dx = pos.0 as f32 - old.0 as f32;
                            game.player.angle =
                                world::normalize_angle(game.player.angle + dx * game.sensitivity);
                        }
                        self.mouse_position = Some(pos);
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        self.mouse_position = Some(pos);
                        if game.phase == Phase::Title {
                            game.start();
                        }
                        if game.phase == Phase::Playing {
                            self.left_mouse = true;
                            // Trigger the first shot on the press itself. The
                            // held state drives subsequent shots and reloads.
                            game.shoot();
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => self.left_mouse = false,
                    MouseEventKind::Down(MouseButton::Right) => game.reload(),
                    MouseEventKind::ScrollUp => {
                        game.sensitivity = (game.sensitivity * 1.2).min(0.1)
                    }
                    MouseEventKind::ScrollDown => {
                        game.sensitivity = (game.sensitivity / 1.2).max(0.005)
                    }
                    _ => {}
                }
            }
            Event::Key(key) => {
                let code = match key.code {
                    KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                    code => code,
                };
                if code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    outcome.quit = true;
                    return outcome;
                }
                if key.kind == KeyEventKind::Release {
                    self.held.remove(&code);
                    self.movement_events.remove(&code);
                    self.actions.remove(&code);
                    return outcome;
                }
                if matches!(
                    code,
                    KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Char('w' | 'a' | 's' | 'd' | 'q' | 'e' | ' ')
                ) {
                    if game.phase == Phase::Playing {
                        if code == KeyCode::Char(' ') {
                            self.fire_pressed = true;
                        }
                        let previous = self.held.get(&code);
                        // Retain repeat timing separately from active movement:
                        // an expired tap must not make every repeat a new tap.
                        let repeating = self
                            .movement_events
                            .insert(code, now)
                            .is_some_and(|last| now - last <= 0.15);
                        // Require a matching physical press before relying on
                        // Quartz; denied access or synthetic input stays usable.
                        let native = previous.is_some_and(|held| held.native)
                            || self.key_state.and_then(|read| read(code)) == Some(true);
                        let expiry = if self.release_events {
                            f64::INFINITY
                        } else {
                            // A lone terminal event is a small nudge, never an
                            // assumed hold spanning the OS's initial delay.
                            now + if repeating { 0.10 } else { 0.045 }
                        };
                        self.held.insert(
                            code,
                            HeldKey {
                                expires: expiry,
                                shift: key.modifiers.contains(KeyModifiers::SHIFT),
                                native,
                            },
                        );
                    } else if game.phase == Phase::Title && code == KeyCode::Char(' ') {
                        game.start();
                    }
                } else {
                    let previous = self.actions.insert(code, now);
                    let fresh = key.kind != KeyEventKind::Repeat
                        && previous.is_none_or(|last| now - last > 0.25);
                    if !fresh {
                        return outcome;
                    }
                    match code {
                        KeyCode::Esc => match game.phase {
                            Phase::Playing => game.overlay(Phase::Paused),
                            Phase::Paused | Phase::Map | Phase::Help => game.phase = game.resume,
                            _ => outcome.quit = true,
                        },
                        KeyCode::Enter if game.phase == Phase::Title => game.start(),
                        KeyCode::Enter | KeyCode::Char('r')
                            if matches!(game.phase, Phase::Lost | Phase::Won) =>
                        {
                            let (mouse, sensitivity, sound, minimap) =
                                (game.mouse, game.sensitivity, game.sound, game.minimap);
                            self.seed = self.seed.wrapping_add(1);
                            *game = Game::new(self.seed);
                            game.mouse = mouse;
                            game.sensitivity = sensitivity;
                            game.sound = sound;
                            game.minimap = minimap;
                            game.start();
                        }
                        KeyCode::Char('r') => game.reload(),
                        KeyCode::Char('x') => game.dash(self.controls(now)),
                        KeyCode::Char('f')
                            if game.phase == Phase::Playing && game.break_time == 0.0 =>
                        {
                            game.auto_fire = !game.auto_fire
                        }
                        KeyCode::Char('p') => match game.phase {
                            Phase::Playing | Phase::Paused => game.overlay(Phase::Paused),
                            Phase::Map | Phase::Help => game.phase = game.resume,
                            _ => {}
                        },
                        KeyCode::Char('m') if game.phase == Phase::Map => game.phase = game.resume,
                        KeyCode::Char('m')
                            if key.modifiers.contains(KeyModifiers::SHIFT)
                                && game.phase == Phase::Playing =>
                        {
                            game.overlay(Phase::Map)
                        }
                        KeyCode::Char('m') => game.minimap = !game.minimap,
                        KeyCode::Char('h')
                            if matches!(
                                game.phase,
                                Phase::Playing | Phase::Title | Phase::Help
                            ) =>
                        {
                            game.overlay(Phase::Help)
                        }
                        KeyCode::Char('l') => {
                            game.mouse = !game.mouse;
                            self.mouse_position = None;
                            game.say(if game.mouse {
                                "TERMINAL MOUSE ON  //  Q/E ALSO TURN"
                            } else {
                                "MOUSE LOOK OFF  //  Q/E TO TURN"
                            });
                        }
                        KeyCode::Char('b') => game.sound = !game.sound,
                        KeyCode::Char('[') => {
                            game.sensitivity = (game.sensitivity / 1.2).max(0.005)
                        }
                        KeyCode::Char(']') => game.sensitivity = (game.sensitivity * 1.2).min(0.1),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if game.phase != old_phase {
            self.clear();
            // Preserve the deploying press for continuous fire after its first
            // shot. Mouse-up only ends the hold; it never triggers a shot.
            self.left_mouse = deploying_click && game.phase == Phase::Playing;
            game.auto_fire = false;
            outcome.redraw = true;
        }
        outcome
    }
}
