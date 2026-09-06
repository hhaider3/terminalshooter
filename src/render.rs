use crate::game::{Game, Kind, MAGAZINE, Phase, PickupKind, RELOAD_TIME};
use crate::world::{self, Vec2};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);
pub const BG: Rgb = Rgb(8, 12, 20);
pub const GREEN: Rgb = Rgb(70, 245, 155);
pub const WHITE: Rgb = Rgb(220, 236, 241);
pub const DIM: Rgb = Rgb(90, 120, 140);
pub const GOLD: Rgb = Rgb(250, 184, 74);
pub const RED: Rgb = Rgb(245, 60, 52);
pub const MM_WALL: Rgb = Rgb(90, 125, 145);

impl Rgb {
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self(
            (self.0 as f32 + (other.0 as f32 - self.0 as f32) * t) as u8,
            (self.1 as f32 + (other.1 as f32 - self.1 as f32) * t) as u8,
            (self.2 as f32 + (other.2 as f32 - self.2 as f32) * t) as u8,
        )
    }
    pub fn scale(self, scale: f32) -> Self {
        Self(
            (self.0 as f32 * scale).clamp(0.0, 255.0) as u8,
            (self.1 as f32 * scale).clamp(0.0, 255.0) as u8,
            (self.2 as f32 * scale).clamp(0.0, 255.0) as u8,
        )
    }
    fn fog(self, distance: f32) -> Self {
        self.mix(BG, (distance / world::FAR).powf(1.4).min(0.92))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    pub fg: Rgb,
    pub bg: Rgb,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            glyph: ' ',
            fg: WHITE,
            bg: BG,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
}
impl Frame {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
        }
    }
    pub fn pixel(&mut self, x: i32, y: i32, color: Rgb) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 * 2 {
            return;
        }
        let cell = &mut self.cells[y as usize / 2 * self.width + x as usize];
        if cell.glyph != '▀' {
            cell.fg = cell.bg;
            cell.glyph = '▀';
        }
        if y % 2 == 0 {
            cell.fg = color;
        } else {
            cell.bg = color;
        }
    }
    pub fn text(&mut self, x: usize, y: usize, text: &str, color: Rgb) {
        if y >= self.height || x >= self.width {
            return;
        }
        for (offset, glyph) in text.chars().take(self.width - x).enumerate() {
            self.cells[y * self.width + x + offset] = Cell {
                glyph,
                fg: color,
                bg: BG,
            };
        }
    }
    fn shade(&mut self, x: i32, y: i32, color: Rgb, amount: f32) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 * 2 {
            let cell = self.cells[y as usize / 2 * self.width + x as usize];
            let old = if y % 2 == 0 { cell.fg } else { cell.bg };
            self.pixel(x, y, old.mix(color, amount));
        }
    }
    fn center(&mut self, y: usize, text: &str, color: Rgb) {
        self.text(
            self.width.saturating_sub(text.chars().count()) / 2,
            y,
            text,
            color,
        );
    }
    pub fn plain(&self) -> String {
        self.cells
            .chunks(self.width)
            .map(|row| row.iter().map(|c| c.glyph).collect::<String>() + "\n")
            .collect()
    }
}

#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub fps: f32,
    pub release_events: bool,
}

pub fn draw(game: &Game, width: usize, height: usize, stats: Stats) -> Frame {
    let mut frame = Frame::new(width, height);
    if width < 40 || height < 16 {
        frame.center(height / 2, "Resize terminal to at least 40x16", GOLD);
        return frame;
    }
    if game.phase != Phase::Playing {
        menu(&mut frame, game, stats);
        return frame;
    }
    scene(&mut frame, game);
    if game.minimap {
        minimap(&mut frame, game);
    }
    hud(&mut frame, game, stats);
    frame
}

fn scene(frame: &mut Frame, game: &Game) {
    let w = frame.width;
    let h = (frame.height - 4) * 2;
    let horizon = h as f32 / 2.0;
    let fov = (78.0_f32 + if game.sprinting { 4.0 } else { 0.0 }).to_radians();
    let plane_scale = (fov / 2.0).tan();
    let forward = Vec2::facing(game.player.angle);
    let side = Vec2::new(-forward.y, forward.x);
    let moon = Vec2::facing(-1.25);
    let moon_depth = moon.dot(forward);
    let moon_x = w as f32 * 0.5 * (1.0 + moon.dot(side) / (moon_depth * plane_scale));
    let mut depths = vec![world::FAR; w];
    for (x, depth) in depths.iter_mut().enumerate() {
        let camera_x = 2.0 * (x as f32 + 0.5) / w as f32 - 1.0;
        let ray = forward + side * (camera_x * plane_scale);
        let hit = world::cast(game.player.pos, ray);
        *depth = hit.distance;
        let size = h as f32 / hit.distance;
        let top = horizon - size / 2.0;
        let bottom = horizon + size / 2.0;
        let wall_point = game.player.pos + ray * hit.distance;
        let exterior = wall_point.x < 1.01
            || wall_point.x > 17.99
            || wall_point.y < 1.01
            || wall_point.y > 13.99;
        let base = if hit.side {
            Rgb(31, 79, 95)
        } else {
            Rgb(64, 108, 129)
        };
        // Quantized bands give depth without changing every color on every step.
        let light = 1.0 / (1.0 + (hit.distance * 0.4).floor() * 0.15);
        for y in 0..h {
            let yf = y as f32 + 0.5;
            let color = if yf < top {
                let band = ((yf / horizon) * 8.0).floor() / 8.0;
                let sky = Rgb(7, 13, 27).mix(Rgb(42, 64, 77), band * band);
                // Skyline is anchored to world bearing, not the screen.
                let azimuth = game.player.angle + (camera_x * plane_scale).atan();
                let sector = ((azimuth + std::f32::consts::TAU) * 55.0) as i32;
                let tower = ((sector / 4).rem_euclid(7) * 3 + 5) as f32;
                let mx = x as f32 - moon_x;
                let my = yf - h as f32 * 0.16;
                let radius = h as f32 * 0.065;
                if moon_depth > 0.2 && mx * mx + my * my < radius * radius {
                    if (mx + radius * 0.5).powi(2) + (my - radius * 0.2).powi(2) < radius * radius {
                        Rgb(33, 41, 63)
                    } else {
                        Rgb(146, 161, 190)
                    }
                } else if yf > horizon - tower && yf > horizon * 0.48 {
                    if sector.rem_euclid(4) == 1 && y % 4 == 1 {
                        Rgb(68, 103, 112)
                    } else {
                        Rgb(14, 25, 36)
                    }
                } else {
                    sky
                }
            } else if yf >= bottom {
                let distance = h as f32 * 0.5 / (yf - horizon).max(1.0);
                let pos = game.player.pos + ray * distance;
                let fx = pos.x.rem_euclid(1.0);
                let fy = pos.y.rem_euclid(1.0);
                let seam = fx < 0.025 || fy < 0.025;
                let lane = (pos.x - 9.5).abs();
                let base = if (lane - 1.6).abs() < 0.045 {
                    Rgb(45, 148, 156)
                } else if lane < 0.065 && pos.y.rem_euclid(2.0) < 0.7 {
                    Rgb(172, 133, 65)
                } else if seam {
                    Rgb(15, 25, 32)
                } else if (pos.x.floor() as i32 + pos.y.floor() as i32) & 1 == 0 {
                    Rgb(35, 48, 55)
                } else {
                    Rgb(30, 42, 50)
                };
                base.scale(1.0 / (1.0 + (distance * 0.3).floor() * 0.18))
            } else {
                let v = ((yf - top) / size).clamp(0.0, 1.0);
                let u = hit.texture;
                let panel = if v < 0.045 || (0.22..0.25).contains(&v) {
                    Rgb(14, 25, 34)
                } else if (0.06..0.10).contains(&v) && (0.12..0.88).contains(&u) {
                    if exterior {
                        Rgb(62, 176, 180)
                    } else {
                        Rgb(235, 160, 66)
                    }
                } else if v > 0.86 && !exterior {
                    if ((u * 7.0 + v * 5.0).floor() as i32) & 1 == 0 {
                        Rgb(185, 129, 48)
                    } else {
                        Rgb(26, 32, 37)
                    }
                } else if !(0.035..=0.965).contains(&u) {
                    Rgb(20, 33, 44)
                } else if u < 0.075 {
                    base.scale(1.4)
                } else if (0.33..0.65).contains(&v) && (0.25..0.75).contains(&u) {
                    if (v * 30.0).fract() < 0.45 {
                        Rgb(13, 24, 32)
                    } else {
                        base.scale(0.75)
                    }
                } else {
                    base
                };
                let panel = panel.scale(light);
                if game.muzzle > 0.0 {
                    panel.mix(GOLD, (0.24 - hit.distance * 0.02).max(0.0))
                } else {
                    panel
                }
            };
            frame.pixel(x as i32, y as i32, color);
        }
    }
    let mut sprites = Vec::new();
    for (index, enemy) in game.enemies.iter().enumerate() {
        if enemy.alive() || enemy.corpse > 0.0 {
            sprites.push(((enemy.pos - game.player.pos).dot(forward), false, index));
        }
    }
    for (index, pickup) in game.pickups.iter().enumerate() {
        sprites.push(((pickup.pos - game.player.pos).dot(forward), true, index));
    }
    sprites.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (depth, pickup, index) in sprites {
        if depth < 0.15 || depth > world::FAR {
            continue;
        }
        let (pos, scale) = if pickup {
            (game.pickups[index].pos, 0.45)
        } else {
            (game.enemies[index].pos, game.enemies[index].kind.scale())
        };
        let lateral = (pos - game.player.pos).dot(side);
        let center = (w as f32 * 0.5 * (1.0 + lateral / (depth * plane_scale))) as i32;
        let size = (h as f32 / depth * scale).min(h as f32 * 2.0) as i32;
        if size < 2 {
            continue;
        }
        let aspect = if pickup {
            0.5
        } else {
            match game.enemies[index].kind {
                Kind::Brute => 0.48,
                Kind::Runner => 0.32,
                Kind::Grunt => 0.40,
            }
        };
        let half = (size as f32 * aspect).max(1.0) as i32;
        let bottom = (horizon + h as f32 / depth / 2.0) as i32;
        let top = bottom - size;
        // Contact shadows anchor sprites to the floor, with wall occlusion.
        let radius = (size / 10).max(1);
        for x in (center - half).max(0)..=(center + half).min(w as i32 - 1) {
            if depth >= depths[x as usize] {
                continue;
            }
            for y in (bottom - radius).max(0)..=(bottom + radius).min(h as i32 - 1) {
                let ellipse = ((x - center) as f32 / half as f32).powi(2)
                    + ((y - bottom) as f32 / radius as f32).powi(2);
                if ellipse < 1.0 {
                    frame.shade(x, y, BG, 0.65);
                }
            }
        }
        for x in (center - half).max(0)..=(center + half).min(w as i32 - 1) {
            if depth >= depths[x as usize] {
                continue;
            }
            let u = (x - center + half) as f32 / (half * 2) as f32;
            for y in top.max(0)..bottom.min(h as i32) {
                let v = (y - top) as f32 / size as f32;
                let color = if pickup {
                    let base = match game.pickups[index].kind {
                        PickupKind::Medkit => GREEN,
                        PickupKind::Ammo => GOLD,
                    };
                    if (u - 0.5).abs() + (v - 0.5).abs() > 0.5 {
                        None
                    } else if (u - 0.5).abs() < 0.1 || (v - 0.5).abs() < 0.1 {
                        Some(WHITE)
                    } else {
                        Some(base)
                    }
                } else {
                    let enemy = &game.enemies[index];
                    let mut body = match enemy.kind {
                        Kind::Grunt => RED,
                        Kind::Runner => GOLD,
                        Kind::Brute => Rgb(176, 98, 225),
                    };
                    if enemy.wake > 0.0 {
                        body = Rgb(65, 180, 215);
                    }
                    if enemy.windup > 0.0 {
                        body = Rgb(255, 244, 100);
                    }
                    if enemy.flash > 0.0 {
                        body = WHITE;
                    }
                    enemy_pixel(u, v, body, enemy.kind, enemy.hp <= 0)
                };
                if let Some(color) = color {
                    frame.pixel(x, y, color.fog(depth));
                }
            }
        }
    }
    let cx = w as i32 / 2;
    let cy = h as i32 / 2;
    weapon(frame, game, h);
    let cross = if game.hit > 0.0 { WHITE } else { GREEN };
    for (dx, dy) in [(-3, 0), (3, 0), (0, -3), (0, 3), (0, 0)] {
        frame.pixel(cx + dx, cy + dy, cross);
    }
    if game.hurt > 0.0 {
        let direction = world::normalize_angle(game.hurt_angle - game.player.angle);
        let x = if direction < -0.5 {
            1
        } else if direction > 0.5 {
            w as i32 - 2
        } else {
            cx
        };
        for offset in -3..=3 {
            frame.pixel(x, if x == cx { 1 } else { cy + offset }, RED);
        }
    }
}

fn ink(mark: u8, body: Rgb, weapon: bool) -> Option<Rgb> {
    match mark {
        b's' => Some(Rgb(8, 17, 25)),
        b'H' => Some(body.mix(WHITE, 0.42)),
        b'a' | b'A' => Some(body),
        b'D' => Some(body.scale(0.42)),
        b'E' => Some(if weapon { GREEN } else { Rgb(255, 233, 165) }),
        b'G' => Some(Rgb(127, 99, 78)),
        b'g' => Some(Rgb(66, 57, 52)),
        _ => None,
    }
}

fn enemy_pixel(u: f32, v: f32, body: Rgb, kind: Kind, dead: bool) -> Option<Rgb> {
    if dead {
        return if v > 0.87 && (0.1..0.9).contains(&u) {
            Some(body.scale(0.3))
        } else {
            None
        };
    }
    let sprite = match kind {
        Kind::Grunt => crate::art::GRUNT,
        Kind::Runner => crate::art::RUNNER,
        Kind::Brute => crate::art::BRUTE,
    };
    let row = sprite[(v * sprite.len() as f32) as usize % sprite.len()].as_bytes();
    let x = (u * 19.0) as usize;
    ink(row.get(x).copied().unwrap_or(b' '), body, false)
}

fn weapon(frame: &mut Frame, game: &Game, scene_height: usize) {
    let sprite = crate::art::SHOTGUN;
    let height = (scene_height as f32 * 0.35).clamp(8.0, 32.0) as i32;
    let width = height * 40 / sprite.len() as i32;
    let reload = if game.player.reload > 0.0 {
        (std::f32::consts::PI * game.player.reload / RELOAD_TIME).sin()
    } else {
        0.0
    };
    let recoil = (game.muzzle / 0.07 * 3.0) as i32;
    let ox = frame.width as i32 / 2 + frame.width as i32 / 9 - width / 2;
    let oy = scene_height as i32 - height + recoil + (reload * height as f32 * 0.65) as i32;
    for y in 0..height {
        let row = sprite[y as usize * sprite.len() / height as usize].as_bytes();
        for x in 0..width {
            let mark = row
                .get(x as usize * 40 / width as usize)
                .copied()
                .unwrap_or(b' ');
            if let Some(color) = ink(mark, Rgb(94, 123, 139), true)
                && oy + y < scene_height as i32
            {
                frame.pixel(ox + x, oy + y, color);
            }
        }
    }
    if game.muzzle > 0.0 {
        let mx = ox + width / 2;
        for y in -7_i32..2 {
            for x in -8_i32..=8 {
                let r = x.abs() + (y + 2).abs();
                if r < 6 || (y == -2 && x.abs() < 8) {
                    frame.pixel(mx + x, oy + y, if r < 3 { WHITE } else { GOLD });
                }
            }
        }
    }
}

fn minimap(frame: &mut Frame, game: &Game) {
    let (ox, oy) = (2_i32, 2_i32);
    let (px, py) = game.player.pos.cell();
    let compact = frame.width < 60 || frame.height < 22;
    let (mw, mh) = if compact {
        (9, 9)
    } else {
        (world::WIDTH as i32, world::HEIGHT as i32)
    };
    let sx = (px - mw / 2).clamp(0, world::WIDTH as i32 - mw);
    let sy = (py - mh / 2).clamp(0, world::HEIGHT as i32 - mh);
    for y in -1..=mh {
        for x in -1..=mw {
            let color = if x < 0 || y < 0 || x == mw || y == mh {
                DIM
            } else if world::wall(sx + x, sy + y) {
                MM_WALL
            } else {
                BG
            };
            frame.pixel(ox + x, oy + y, color);
        }
    }
    let mark = |frame: &mut Frame, x: i32, y: i32, color| {
        if x >= sx && y >= sy && x < sx + mw && y < sy + mh {
            frame.pixel(ox + x - sx, oy + y - sy, color);
        }
    };
    for pickup in &game.pickups {
        let (x, y) = pickup.pos.cell();
        mark(
            frame,
            x,
            y,
            if pickup.kind == PickupKind::Medkit {
                GREEN.scale(0.5)
            } else {
                GOLD
            },
        );
    }
    for enemy in game.enemies.iter().filter(|e| e.alive()) {
        let (x, y) = enemy.pos.cell();
        mark(frame, x, y, RED);
    }
    let facing = Vec2::facing(game.player.angle);
    let (tx, ty) = (px + facing.x.round() as i32, py + facing.y.round() as i32);
    if !world::wall(tx, ty)
        && world::visible(game.player.pos, Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5))
    {
        mark(frame, tx, ty, WHITE);
    }
    mark(frame, px, py, GREEN);
}

fn hud(frame: &mut Frame, game: &Game, stats: Stats) {
    let row = frame.height - 4;
    let p = &game.player;
    let ammo = if p.reload > 0.0 {
        format!(
            "RELOAD {:3}%",
            (100.0 * (1.0 - p.reload / RELOAD_TIME)) as u8
        )
    } else {
        format!("SHELLS {}/{} +{}", p.shells, MAGAZINE, p.reserve)
    };
    frame.text(
        1,
        row,
        &format!("HP {:3}   {ammo}", p.hp),
        if p.hp > 30 { GREEN } else { RED },
    );
    let mut objective = format!(
        "WAVE {}/5  LEFT {}  SCORE {}",
        game.wave,
        game.alive(),
        game.score
    );
    if frame.width >= 65 {
        let _ = write!(objective, "   {:.0} FPS", stats.fps);
    }
    if game.break_time > 0.0 {
        objective = format!(
            "WAVE CLEAR // NEXT IN {} // +25 HP",
            game.break_time.ceil() as u8
        );
    }
    frame.text(1, row + 1, &objective, WHITE);
    let dash = if p.dash_cooldown == 0.0 {
        "READY".to_owned()
    } else {
        format!("{:.1}s", p.dash_cooldown)
    };
    let mut status = format!(
        "X dodge {dash} | F fire {} | R reload",
        if game.auto_fire { "ON" } else { "OFF" }
    );
    if frame.width >= 90 {
        let _ = write!(
            status,
            " | L mouse {} | H help",
            if game.mouse { "ON" } else { "OFF" }
        );
    }
    frame.text(1, row + 2, &status, DIM);
    let mut message = if game.message_time > 0.0 {
        game.message.clone()
    } else {
        "Yellow enemy = attack incoming. Shoot or dodge.".to_owned()
    };
    if game.message_time == 0.0
        && game.alive() <= 2
        && let Some(enemy) = game.enemies.iter().filter(|e| e.alive()).min_by(|a, b| {
            (a.pos - p.pos)
                .length()
                .total_cmp(&(b.pos - p.pos).length())
        })
    {
        let delta = enemy.pos - p.pos;
        let angle = world::normalize_angle(delta.angle() - p.angle);
        let bearing = if angle.abs() < 0.4 {
            "AHEAD"
        } else if angle.abs() > 2.4 {
            "BEHIND"
        } else if angle < 0.0 {
            "LEFT"
        } else {
            "RIGHT"
        };
        message = format!("LAST HOSTILES: {bearing}  {:.0}m", delta.length());
    }
    frame.text(1, row + 3, &message, GOLD);
    if frame.width >= 90 {
        let x = frame.width - 31;
        let health = (p.hp.max(0) as usize).div_ceil(10).min(10);
        frame.text(
            x,
            row,
            &format!("VITAL [{}{}]", "━".repeat(health), "·".repeat(10 - health)),
            if p.hp > 30 { GREEN } else { RED },
        );
        let shells = p.shells.min(MAGAZINE) as usize;
        frame.text(
            x,
            row + 1,
            &format!(
                "LOAD  {}{}",
                "▮ ".repeat(shells),
                "· ".repeat(MAGAZINE as usize - shells)
            ),
            GOLD,
        );
    }
}

fn menu(frame: &mut Frame, game: &Game, stats: Stats) {
    let edge = Rgb(32, 59, 72);
    for y in 0..frame.height {
        frame.text(0, y, "│", edge);
        frame.text(frame.width - 1, y, "│", edge);
    }
    let rule = "─".repeat(frame.width - 2);
    frame.text(1, 0, &rule, edge);
    frame.text(1, frame.height - 1, &rule, edge);
    let title = match game.phase {
        Phase::Title => "TERMINAL // BREACH",
        Phase::Paused => "PAUSED",
        Phase::Map => "TACTICAL MAP",
        Phase::Help => "FIELD MANUAL",
        Phase::Won => "ARENA CLEARED",
        Phase::Lost => "RUN ENDED",
        _ => "",
    };
    frame.center(1, title, GREEN);
    frame.center(2, &"─".repeat((frame.width - 6).min(44)), DIM);
    match game.phase {
        Phase::Title => {
            let spacious = frame.width >= 78 && frame.height >= 28;
            if spacious {
                let logo = [
                    "████▄  ████▄  █████  ▄███▄  ▄████  █   █",
                    "█   █  █   █  █      █   █  █      █   █",
                    "████   ████   ████   █████  █      █████",
                    "█   █  █  █   █      █   █  █      █   █",
                    "████   █   █  █████  █   █   ████  █   █",
                ];
                for (i, line) in logo.iter().enumerate() {
                    frame.center(5 + i, line, if i < 2 { WHITE } else { GREEN });
                }
                frame.center(11, "N I G H T   S H I F T   / /   S E C T O R   0 7", DIM);
            }
            let lines = [
                "PERIMETER SEALED. HOSTILES INBOUND.",
                "",
                "CLEAR FIVE WAVES. GET OUT.",
                "",
                "WASD move   Q/E turn   X dodge",
                "SPACE shotgun   F continuous fire",
                "",
                "ENTER or CLICK to deploy",
                "H controls   CTRL-C quit",
            ];
            for (i, line) in lines.iter().enumerate() {
                frame.center(
                    (if spacious { 14 } else { 4 }) + i,
                    line,
                    if i == 7 { GOLD } else { WHITE },
                );
            }
        }
        Phase::Paused => {
            frame.center(frame.height / 2 - 1, "P / ESC to resume", WHITE);
            frame.center(frame.height / 2 + 1, "CTRL-C to quit", GOLD);
        }
        Phase::Help => {
            let lines = [
                "WASD / up-down: move  SHIFT+W: sprint",
                "Q/E / left-right: turn  X: dodge",
                "SPACE / click: shotgun  R: reload",
                "F: continuous fire  L: mouse look",
                "M: minimap  SHIFT+M: tactical map",
                "[ / ] / wheel: mouse sensitivity",
                "B: beep sound  P / ESC: pause",
                "Mouse reports stop at window edges.",
                "Q/E always work. CTRL-C quits.",
                "H / ESC: back",
            ];
            for (i, line) in lines.iter().enumerate() {
                frame.center(4 + i, line, WHITE);
            }
        }
        Phase::Map => {
            let available = frame.height - 7;
            let start = (game.player.pos.y as usize)
                .saturating_sub(available / 2)
                .min(world::HEIGHT.saturating_sub(available));
            let x = frame.width.saturating_sub(world::WIDTH) / 2;
            for (i, line) in world::MAP.iter().skip(start).take(available).enumerate() {
                frame.text(x, 4 + i, line, DIM);
            }
            for enemy in game.enemies.iter().filter(|e| e.alive()) {
                let (ex, ey) = enemy.pos.cell();
                if ey as usize >= start && (ey as usize) < start + available {
                    frame.text(x + ex as usize, 4 + ey as usize - start, "E", RED);
                }
            }
            let (px, py) = game.player.pos.cell();
            frame.text(x + px as usize, 4 + py as usize - start, "@", GREEN);
            frame.center(frame.height - 2, "M / P / ESC: back", GOLD);
        }
        Phase::Won | Phase::Lost => {
            frame.center(
                frame.height / 2 - 2,
                &format!("SCORE {}   KILLS {}", game.score, game.kills),
                WHITE,
            );
            frame.center(
                frame.height / 2,
                &format!("WAVE {}/5   TIME {:.0}s", game.wave, game.time),
                DIM,
            );
            frame.center(
                frame.height / 2 + 2,
                "R / ENTER: new run   CTRL-C: quit",
                GOLD,
            );
        }
        _ => {}
    }
    if game.phase == Phase::Title && frame.height >= 20 {
        frame.center(
            frame.height - 2,
            if stats.release_events {
                "KEY RELEASE INPUT ACTIVE"
            } else {
                "TERMINAL KEY REPEAT // F TOGGLE-FIRE"
            },
            DIM,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Palette,
}

fn palette(color: Rgb) -> u32 {
    let values = [0_i32, 95, 135, 175, 215, 255];
    let nearest = |c: u8| {
        (0..6)
            .min_by_key(|&i| (values[i] - c as i32).pow(2))
            .unwrap()
    };
    let (r, g, b) = (nearest(color.0), nearest(color.1), nearest(color.2));
    let error = (values[r] - color.0 as i32).pow(2)
        + (values[g] - color.1 as i32).pow(2)
        + (values[b] - color.2 as i32).pow(2);
    let average = (color.0 as i32 + color.1 as i32 + color.2 as i32) / 3;
    let gray = ((average - 8 + 5) / 10).clamp(0, 23);
    let value = 8 + gray * 10;
    let gray_error = (value - color.0 as i32).pow(2)
        + (value - color.1 as i32).pow(2)
        + (value - color.2 as i32).pow(2);
    if gray_error < error {
        232 + gray as u32
    } else {
        (16 + 36 * r + 6 * g + b) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Encoded {
    glyph: char,
    fg: u32,
    bg: u32,
}

pub struct TerminalRenderer {
    pub mode: ColorMode,
    previous: Vec<Encoded>,
    size: (usize, usize),
}
impl TerminalRenderer {
    pub fn new(mode: ColorMode) -> Self {
        Self {
            mode,
            previous: Vec::new(),
            size: (0, 0),
        }
    }
    pub fn invalidate(&mut self) {
        self.previous.clear();
    }
    pub fn encode(&mut self, frame: &Frame) -> String {
        let encode = |rgb: Rgb| match self.mode {
            ColorMode::Palette => palette(rgb),
            ColorMode::TrueColor => ((rgb.0 as u32) << 16) | ((rgb.1 as u32) << 8) | rgb.2 as u32,
        };
        let cells: Vec<_> = frame
            .cells
            .iter()
            .map(|c| Encoded {
                glyph: c.glyph,
                fg: encode(c.fg),
                bg: encode(c.bg),
            })
            .collect();
        let full = self.size != (frame.width, frame.height) || self.previous.len() != cells.len();
        if !full && cells == self.previous {
            return String::new();
        }
        let mut out = String::with_capacity(frame.cells.len() * 16);
        out.push_str("\x1b[?2026h");
        if full {
            out.push_str("\x1b[0m\x1b[2J");
        }
        let mut fg = None;
        let mut bg = None;
        let mut adjacent = false;
        for (i, cell) in cells.iter().enumerate() {
            if !full && *cell == self.previous[i] {
                adjacent = false;
                continue;
            }
            if !adjacent || i % frame.width == 0 {
                let _ = write!(out, "\x1b[{};{}H", i / frame.width + 1, i % frame.width + 1);
            }
            if fg != Some(cell.fg) {
                self.color(&mut out, cell.fg, true);
                fg = Some(cell.fg);
            }
            if bg != Some(cell.bg) {
                self.color(&mut out, cell.bg, false);
                bg = Some(cell.bg);
            }
            out.push(cell.glyph);
            adjacent = true;
        }
        out.push_str("\x1b[0m\x1b[?2026l");
        self.previous = cells;
        self.size = (frame.width, frame.height);
        out
    }
    fn color(&self, out: &mut String, value: u32, fg: bool) {
        let prefix = if fg { 38 } else { 48 };
        match self.mode {
            ColorMode::Palette => {
                let _ = write!(out, "\x1b[{prefix};5;{value}m");
            }
            ColorMode::TrueColor => {
                let _ = write!(
                    out,
                    "\x1b[{prefix};2;{};{};{}m",
                    (value >> 16) & 255,
                    (value >> 8) & 255,
                    value & 255
                );
            }
        }
    }
}
