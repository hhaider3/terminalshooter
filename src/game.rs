use crate::world::{self, CELLS, Rng, Vec2};
use std::f32::consts::FRAC_PI_2;

pub const MAGAZINE: u16 = 6;
pub const FINAL_WAVE: u8 = 5;
pub const SHOT_DELAY: f32 = 0.38;
pub const RELOAD_TIME: f32 = 0.85;
/// Pellet spread either side of the crosshair, in radians.
pub const SPREAD: f32 = 0.10;
/// Side-lane medkit spots, restocked at the start of every wave.
pub const MEDKIT_SPOTS: [Vec2; 2] = [Vec2::new(2.5, 7.5), Vec2::new(16.5, 7.5)];
/// Reserve shells added at the start of each wave after the first.
pub const WAVE_RESUPPLY: u16 = 12;
/// Shells in an ammo pickup, dropped by every third kill.
pub const AMMO_PICKUP: u16 = 6;
/// Endless waves stop growing at this many enemies.
pub const MAX_ENEMIES: usize = 25;

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
    /// Half the sprite's width in map units, shared by rendering and hits.
    pub fn half_width(self) -> f32 {
        match self {
            Self::Grunt => 0.48,
            Self::Runner => 0.32,
            Self::Brute => 0.72,
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
    /// Continue past the final wave instead of winning.
    pub endless: bool,
    /// Best score across runs; updated when a run ends.
    pub best: u32,
    /// The run that just ended beat the previous best.
    pub new_best: bool,
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
                reserve: 24,
                reload: 0.0,
                cooldown: 0.0,
                dash_cooldown: 0.0,
                dash_time: 0.0,
                dash_direction: Vec2::default(),
            },
            enemies: Vec::new(),
            pickups: Vec::new(),
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
            endless: false,
            best: 0,
            new_best: false,
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
        self.wave = self.wave.saturating_add(1);
        self.enemies.clear();
        let count = (3 + self.wave as usize * 2).min(MAX_ENEMIES);
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
            // Arena has far more safe cells than MAX_ENEMIES.
            if !candidates.is_empty() {
                let pos = candidates[self.rng.index(candidates.len())];
                self.enemies.push(Enemy::new(pos, kind));
            }
        }
        self.player.shells = MAGAZINE;
        // A fixed resupply keeps ammo drops worth collecting; the floor keeps
        // every wave winnable however many shells the last one used.
        if self.wave > 1 {
            self.player.reserve = self.player.reserve.saturating_add(WAVE_RESUPPLY);
        }
        self.player.reserve = self.player.reserve.max(count as u16 * 2);
        self.player.reload = 0.0;
        for spot in MEDKIT_SPOTS {
            if !self
                .pickups
                .iter()
                .any(|p| p.kind == PickupKind::Medkit && p.pos == spot)
            {
                self.pickups.push(Pickup {
                    pos: spot,
                    kind: PickupKind::Medkit,
                });
            }
        }
        let wave = self.wave_label();
        self.say(format!(
            "{wave}  //  {} HOSTILES INCOMING",
            self.enemies.len()
        ));
    }

    /// "WAVE 2/5" in a normal run, "WAVE 7 ENDLESS" after continuing.
    pub fn wave_label(&self) -> String {
        if self.endless {
            format!("WAVE {} ENDLESS", self.wave)
        } else {
            format!("WAVE {}/{FINAL_WAVE}", self.wave)
        }
    }

    fn finish_wave(&mut self) {
        self.auto_fire = false;
        self.combo = 0;
        if self.wave >= FINAL_WAVE && !self.endless {
            self.end(Phase::Won);
        } else {
            self.start_break();
            self.say("WAVE CLEAR  //  +25 HP  //  RESUPPLY INCOMING");
        }
    }

    fn start_break(&mut self) {
        self.break_time = 4.0;
        self.player.hp = (self.player.hp + 25).min(100);
    }

    fn end(&mut self, phase: Phase) {
        self.phase = phase;
        self.auto_fire = false;
        self.new_best = self.score > self.best;
        self.best = self.best.max(self.score);
    }

    /// Keep playing after winning, with waves that keep growing.
    pub fn continue_endless(&mut self) {
        if self.phase != Phase::Won {
            return;
        }
        self.endless = true;
        self.phase = Phase::Playing;
        self.start_break();
        self.say("ENDLESS MODE  //  +25 HP  //  NEXT WAVE IN 4s");
    }

    /// Never leave a run unwinnable: with no shells anywhere, drop a crate in
    /// the side lane farther from the player.
    fn ensure_ammo(&mut self) {
        let p = &self.player;
        if p.shells > 0 || p.reserve > 0 || self.pickups.iter().any(|p| p.kind == PickupKind::Ammo)
        {
            return;
        }
        let spot = MEDKIT_SPOTS
            .into_iter()
            .max_by(|a, b| (*a - p.pos).length().total_cmp(&(*b - p.pos).length()))
            .unwrap_or(MEDKIT_SPOTS[0]);
        self.pickups.push(Pickup {
            pos: Vec2::new(spot.x, spot.y - 1.0),
            kind: PickupKind::Ammo,
        });
        self.say("OUT OF SHELLS  //  CRATE DROPPED IN THE SIDE LANE");
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
            // Sprites are billboards on the camera plane, so the crosshair is
            // over the body exactly when the lateral offset is within its half-width.
            let body = (enemy.kind.half_width() / distance.max(0.001))
                .min(1.0)
                .asin();
            if distance > 12.0
                || angle > body + SPREAD
                || !world::visible(self.player.pos, enemy.pos)
            {
                continue;
            }
            let damage = 72.0
                * (1.0 - (distance - 4.0).max(0.0) * 0.075).max(0.4)
                * if angle <= body { 1.0 } else { 0.65 };
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
                if self.kills.is_multiple_of(3) {
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
            // The break is for recovering: pickups stay collectable and the
            // final kill's corpse and hit flash fade as they do in combat.
            for enemy in &mut self.enemies {
                decay(&mut enemy.flash, dt);
                decay(&mut enemy.corpse, dt);
            }
            self.collect_pickups();
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
            let mut movement = match target {
                Some(target) => {
                    let offset = target - enemy.pos;
                    // Endless waves stop speeding up at 1.5x.
                    let pace = (1.0 + (self.wave - 1) as f32 * 0.035).min(1.5);
                    let speed = enemy.kind.speed() * pace;
                    offset.unit() * (speed * dt).min(offset.length())
                }
                None => Vec2::default(),
            };
            // Routes converge on the same cell centres, so enemies out of sight
            // need separating as much as those in pursuit.
            for (other, (pos, alive)) in positions.iter().enumerate() {
                let apart = enemy.pos - *pos;
                if index != other && *alive && apart.length() < 0.55 {
                    // Coincident enemies have no direction apart; split them by index.
                    let away = if apart.length() > 0.0001 {
                        apart.unit()
                    } else {
                        Vec2::facing(index as f32 * 2.4)
                    };
                    movement = movement + away * (dt * 0.8);
                }
            }
            world::slide(&mut enemy.pos, movement, 0.18);
        }
        if damage > 0 {
            self.player.hp = (self.player.hp - damage).max(0);
            self.hurt = 0.3;
            self.combo_time = 0.0;
            self.say(format!("HIT -{damage}  //  X TO DODGE"));
            if self.player.hp == 0 {
                self.end(Phase::Lost);
                return;
            }
        }
        self.collect_pickups();
        self.ensure_ammo();
    }

    fn collect_pickups(&mut self) {
        let player = &mut self.player;
        self.pickups.retain(|pickup| {
            if (pickup.pos - player.pos).length() > 0.65 {
                return true;
            }
            match pickup.kind {
                PickupKind::Medkit if player.hp < 100 => player.hp = (player.hp + 30).min(100),
                PickupKind::Ammo => player.reserve = player.reserve.saturating_add(AMMO_PICKUP),
                _ => return true,
            }
            false
        });
    }
}
