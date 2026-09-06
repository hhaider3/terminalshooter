use crate::world::{self, CELLS, Rng, Vec2};
use std::f32::consts::FRAC_PI_2;

pub const MAGAZINE: u16 = 6;
pub const FINAL_WAVE: u8 = 5;
pub const SHOT_DELAY: f32 = 0.38;
pub const RELOAD_TIME: f32 = 0.85;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Title,
    Playing,
    Paused,
    Map,
    Help,
    Won,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Grunt,
    Runner,
    Brute,
}
impl Kind {
    pub fn health(self) -> i32 {
        match self {
            Self::Grunt => 58,
            Self::Runner => 36,
            Self::Brute => 150,
        }
    }
    pub fn speed(self) -> f32 {
        match self {
            Self::Grunt => 1.15,
            Self::Runner => 2.05,
            Self::Brute => 0.8,
        }
    }
    pub fn damage(self) -> i32 {
        match self {
            Self::Grunt => 12,
            Self::Runner => 8,
            Self::Brute => 22,
        }
    }
    pub fn windup(self) -> f32 {
        match self {
            Self::Grunt => 0.65,
            Self::Runner => 0.45,
            Self::Brute => 0.9,
        }
    }
    pub fn scale(self) -> f32 {
        match self {
            Self::Grunt => 1.2,
            Self::Runner => 1.0,
            Self::Brute => 1.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub pos: Vec2,
    pub kind: Kind,
    pub hp: i32,
    pub wake: f32,
    pub windup: f32,
    pub cooldown: f32,
    pub flash: f32,
    pub corpse: f32,
}
impl Enemy {
    pub fn new(pos: Vec2, kind: Kind) -> Self {
        Self {
            pos,
            kind,
            hp: kind.health(),
            wake: 1.0,
            windup: 0.0,
            cooldown: 0.0,
            flash: 0.0,
            corpse: 0.0,
        }
    }
    pub fn alive(&self) -> bool {
        self.hp > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub pos: Vec2,
    pub angle: f32,
    pub hp: i32,
    pub shells: u16,
    pub reserve: u16,
    pub reload: f32,
    pub cooldown: f32,
    pub dash_cooldown: f32,
    pub dash_time: f32,
    pub dash_direction: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    Medkit,
    Ammo,
}
#[derive(Clone, Debug)]
pub struct Pickup {
    pub pos: Vec2,
    pub kind: PickupKind,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub forward: f32,
    pub strafe: f32,
    pub turn: f32,
    pub sprint: bool,
    pub fire: bool,
}
impl Controls {
    pub fn direction(self, angle: f32) -> Vec2 {
        let facing = Vec2::facing(angle);
        let side = Vec2::new(-facing.y, facing.x);
        let direction = facing * self.forward + side * self.strafe;
        if direction.length() > 1.0 {
            direction.unit()
        } else {
            direction
        }
    }
}

#[derive(Debug)]
pub struct Game {
    pub phase: Phase,
    pub resume: Phase,
    pub player: Player,
    pub enemies: Vec<Enemy>,
    pub pickups: Vec<Pickup>,
    pub wave: u8,
    pub score: u32,
    pub kills: u32,
    pub combo: u32,
    pub combo_time: f32,
    pub time: f32,
    pub break_time: f32,
    pub muzzle: f32,
    pub hit: f32,
    pub hurt: f32,
    pub hurt_angle: f32,
    pub message: String,
    pub message_time: f32,
    pub minimap: bool,
    pub auto_fire: bool,
    pub mouse: bool,
    pub sensitivity: f32,
    pub sound: bool,
    pub shots_fired: u32,
    pub sprinting: bool,
    rng: Rng,
    route_cell: (i32, i32),
    routes: [u16; CELLS],
}

fn decay(value: &mut f32, dt: f32) {
    *value = (*value - dt).max(0.0);
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let pos = Vec2::new(9.5, 12.5);
        let mut game = Self {
            phase: Phase::Title,
            resume: Phase::Playing,
            player: Player {
                pos,
                angle: -FRAC_PI_2,
                hp: 100,
                shells: MAGAZINE,
                reserve: 48,
                reload: 0.0,
                cooldown: 0.0,
                dash_cooldown: 0.0,
                dash_time: 0.0,
                dash_direction: Vec2::default(),
            },
            enemies: Vec::new(),
            pickups: vec![
                Pickup {
                    pos: Vec2::new(2.5, 7.5),
                    kind: PickupKind::Medkit,
                },
                Pickup {
                    pos: Vec2::new(16.5, 7.5),
                    kind: PickupKind::Medkit,
                },
            ],
            wave: 0,
            score: 0,
            kills: 0,
            combo: 0,
            combo_time: 0.0,
            time: 0.0,
            break_time: 0.0,
            muzzle: 0.0,
            hit: 0.0,
            hurt: 0.0,
            hurt_angle: 0.0,
            message: String::new(),
            message_time: 0.0,
            minimap: true,
            auto_fire: false,
            mouse: true,
            sensitivity: 0.025,
            sound: false,
            shots_fired: 0,
            sprinting: false,
            rng: Rng::new(seed),
            route_cell: pos.cell(),
            routes: world::routes(pos),
        };
        game.next_wave();
        game
    }

    pub fn start(&mut self) {
        self.phase = Phase::Playing;
    }
    pub fn say(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.message_time = 3.0;
    }
    pub fn alive(&self) -> usize {
        self.enemies.iter().filter(|e| e.alive()).count()
    }

    pub fn overlay(&mut self, phase: Phase) {
        if self.phase == phase {
            self.phase = self.resume;
        } else {
            self.resume = self.phase;
            self.phase = phase;
        }
        self.auto_fire = false;
        self.player.dash_time = 0.0;
    }

    pub fn reload(&mut self) {
        let p = &mut self.player;
        if self.phase == Phase::Playing && p.reload == 0.0 && p.shells < MAGAZINE && p.reserve > 0 {
            p.reload = RELOAD_TIME;
        }
    }

    pub fn dash(&mut self, controls: Controls) {
        if self.phase != Phase::Playing || self.player.dash_cooldown > 0.0 {
            return;
        }
        let mut direction = controls.direction(self.player.angle);
        if direction.length() < 0.01 {
            direction = Vec2::facing(self.player.angle);
        }
        self.player.dash_direction = direction.unit();
        self.player.dash_time = 0.18;
        self.player.dash_cooldown = 1.8;
    }

    fn next_wave(&mut self) {
        self.wave += 1;
        self.enemies.clear();
        let count = 3 + self.wave as usize * 2;
        let field = world::routes(self.player.pos);
        for i in 0..count {
            let kind = if self.wave >= 3 && i % 5 == 0 {
                Kind::Brute
            } else if self.wave >= 2 && i % 3 == 0 {
                Kind::Runner
            } else {
                Kind::Grunt
            };
            let mut candidates = Vec::new();
            for (index, distance) in field.iter().enumerate() {
                if *distance == u16::MAX {
                    continue;
                }
                let pos = Vec2::new(
                    (index % world::WIDTH) as f32 + 0.5,
                    (index / world::WIDTH) as f32 + 0.5,
                );
                if (pos - self.player.pos).length() >= 5.0
                    && self.enemies.iter().all(|e| (e.pos - pos).length() > 1.2)
                {
                    candidates.push(pos);
                }
            }
            // Arena has far more safe cells than the maximum 13 enemies.
            if !candidates.is_empty() {
                let pos = candidates[self.rng.index(candidates.len())];
                self.enemies.push(Enemy::new(pos, kind));
            }
        }
        self.player.shells = MAGAZINE;
        self.player.reserve = self.player.reserve.max(count as u16 * 5);
        self.player.reload = 0.0;
        self.say(format!(
            "WAVE {}/{}  //  {} HOSTILES INCOMING",
            self.wave,
            FINAL_WAVE,
            self.enemies.len()
        ));
    }

    fn finish_wave(&mut self) {
        self.auto_fire = false;
        self.combo = 0;
        if self.wave == FINAL_WAVE {
            self.phase = Phase::Won;
        } else {
            self.break_time = 4.0;
            self.player.hp = (self.player.hp + 25).min(100);
            self.say("WAVE CLEAR  //  +25 HP  //  RESUPPLY INCOMING");
        }
    }

    pub fn shoot(&mut self) -> bool {
        if self.phase != Phase::Playing
            || self.break_time > 0.0
            || self.player.cooldown > 0.0
            || self.player.reload > 0.0
        {
            return false;
        }
        if self.player.shells == 0 {
            self.reload();
            return false;
        }
        self.player.shells -= 1;
        self.player.cooldown = SHOT_DELAY;
        self.muzzle = 0.07;
        self.shots_fired += 1;
        let mut killed = 0;
        for enemy in &mut self.enemies {
            if !enemy.alive() || enemy.wake > 0.0 {
                continue;
            }
            let delta = enemy.pos - self.player.pos;
            let distance = delta.length();
            let angle = world::normalize_angle(delta.angle() - self.player.angle).abs();
            if distance > 12.0 || angle > 0.23 || !world::visible(self.player.pos, enemy.pos) {
                continue;
            }
            let damage = 72.0
                * (1.0 - (distance - 4.0).max(0.0) * 0.075).max(0.4)
                * if angle < 0.12 { 1.0 } else { 0.65 };
            enemy.hp -= damage as i32;
            enemy.flash = 0.16;
            enemy.windup = 0.0;
            enemy.cooldown = enemy.cooldown.max(0.3);
            self.hit = 0.18;
            if enemy.hp <= 0 {
                enemy.corpse = 0.4;
                self.kills += 1;
                self.combo = if self.combo_time > 0.0 {
                    (self.combo + 1).min(5)
                } else {
                    1
                };
                self.combo_time = 3.0;
                self.score += 100 * self.combo;
                killed += 1;
                if self.kills.is_multiple_of(4) {
                    self.pickups.push(Pickup {
                        pos: enemy.pos,
                        kind: PickupKind::Ammo,
                    });
                }
            } else {
                self.score += 10;
            }
        }
        if killed > 0 {
            self.say(format!("{} DOWN  //  {}x STREAK", killed, self.combo));
        }
        if !self.enemies.is_empty() && self.alive() == 0 {
            self.finish_wave();
        }
        true
    }

    pub fn tick(&mut self, dt: f32, controls: Controls) {
        if self.phase != Phase::Playing {
            return;
        }
        let dt = dt.clamp(0.0, 0.05);
        self.time += dt;
        for timer in [
            &mut self.muzzle,
            &mut self.hit,
            &mut self.hurt,
            &mut self.combo_time,
            &mut self.message_time,
            &mut self.player.cooldown,
            &mut self.player.dash_cooldown,
        ] {
            decay(timer, dt);
        }
        self.player.angle = world::normalize_angle(self.player.angle + controls.turn * 2.2 * dt);
        self.sprinting = controls.sprint && controls.forward > 0.0;
        if self.player.dash_time > 0.0 {
            let delta = self.player.dash_direction * (12.0 * self.player.dash_time.min(dt));
            world::slide(&mut self.player.pos, delta, 0.22);
            decay(&mut self.player.dash_time, dt);
        } else {
            let delta = controls.direction(self.player.angle)
                * (3.8 * if self.sprinting { 1.5 } else { 1.0 } * dt);
            world::slide(&mut self.player.pos, delta, 0.22);
        }
        if self.player.reload > 0.0 {
            decay(&mut self.player.reload, dt);
            if self.player.reload == 0.0 {
                let take = (MAGAZINE - self.player.shells).min(self.player.reserve);
                self.player.shells += take;
                self.player.reserve -= take;
            }
        }
        if self.break_time > 0.0 {
            decay(&mut self.break_time, dt);
            if self.break_time == 0.0 {
                self.next_wave();
            }
            return;
        }
        if controls.fire || self.auto_fire {
            self.shoot();
        }
        if self.phase != Phase::Playing || self.break_time > 0.0 {
            return;
        }
        if self.player.pos.cell() != self.route_cell {
            self.routes = world::routes(self.player.pos);
            self.route_cell = self.player.pos.cell();
        }
        let positions: Vec<_> = self.enemies.iter().map(|e| (e.pos, e.alive())).collect();
        let mut damage = 0;
        for (index, enemy) in self.enemies.iter_mut().enumerate() {
            decay(&mut enemy.flash, dt);
            decay(&mut enemy.cooldown, dt);
            if !enemy.alive() {
                decay(&mut enemy.corpse, dt);
                continue;
            }
            if enemy.wake > 0.0 {
                decay(&mut enemy.wake, dt);
                continue;
            }
            let delta = self.player.pos - enemy.pos;
            let distance = delta.length();
            let sees = world::visible(enemy.pos, self.player.pos);
            if enemy.windup > 0.0 {
                decay(&mut enemy.windup, dt);
                if enemy.windup == 0.0 {
                    enemy.cooldown = 0.7;
                    if distance < 1.2 && sees && self.player.dash_time == 0.0 {
                        damage += enemy.kind.damage();
                        self.hurt_angle = (enemy.pos - self.player.pos).angle();
                    }
                }
                continue;
            }
            if distance < 0.95 && sees {
                if enemy.cooldown == 0.0 {
                    enemy.windup = enemy.kind.windup();
                }
                continue;
            }
            let target = if sees {
                Some(self.player.pos)
            } else {
                world::waypoint(enemy.pos, &self.routes)
            };
            if let Some(target) = target {
                let offset = target - enemy.pos;
                let speed = enemy.kind.speed() * (1.0 + (self.wave - 1) as f32 * 0.035);
                let mut movement = offset.unit() * (speed * dt).min(offset.length());
                if sees {
                    for (other, (pos, alive)) in positions.iter().enumerate() {
                        let apart = enemy.pos - *pos;
                        if index != other && *alive && apart.length() < 0.55 {
                            movement = movement + apart.unit() * (dt * 0.8);
                        }
                    }
                }
                world::slide(&mut enemy.pos, movement, 0.18);
            }
        }
        if damage > 0 {
            self.player.hp = (self.player.hp - damage).max(0);
            self.hurt = 0.3;
            self.combo_time = 0.0;
            self.say(format!("HIT -{damage}  //  X TO DODGE"));
            if self.player.hp == 0 {
                self.phase = Phase::Lost;
                self.auto_fire = false;
                return;
            }
        }
        let player = &mut self.player;
        self.pickups.retain(|pickup| {
            if (pickup.pos - player.pos).length() > 0.65 {
                return true;
            }
            match pickup.kind {
                PickupKind::Medkit if player.hp < 100 => player.hp = (player.hp + 30).min(100),
                PickupKind::Ammo => player.reserve = player.reserve.saturating_add(16),
                _ => return true,
            }
            false
        });
    }
}
