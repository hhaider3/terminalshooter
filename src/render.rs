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
    let mut depths = vec![world::FAR; w];
    for (x, depth) in depths.iter_mut().enumerate() {
        let camera_x = 2.0 * (x as f32 + 0.5) / w as f32 - 1.0;
        let ray = forward + side * (camera_x * plane_scale);
        let hit = world::cast(game.player.pos, ray);
        *depth = hit.distance;
        let size = h as f32 / hit.distance;
        let top = horizon - size / 2.0;
        let bottom = horizon + size / 2.0;
        let wall_color = if hit.side {
            Rgb(79, 110, 122)
        } else {
            Rgb(118, 149, 156)
        };
        let wall_color = if game.muzzle > 0.0 {
            wall_color.mix(GOLD, (0.2 - hit.distance * 0.015).max(0.0))
        } else {
            wall_color
        };
        let wall_color = wall_color.fog(hit.distance);
        for y in 0..h {
            let yf = y as f32 + 0.5;
            let color = if yf < top {
                let sky = Rgb(5, 9, 25).mix(Rgb(90, 47, 42), (yf / horizon).powi(3));
                if y < h / 4 && (x * 31 + y * 17).is_multiple_of(157) {
                    Rgb(135, 160, 185)
                } else {
                    sky
                }
            } else if yf >= bottom {
                let distance = h as f32 * 0.5 / (yf - horizon).max(1.0);
                let pos = game.player.pos + ray * distance;
                let checker = ((pos.x.floor() as i32) + (pos.y.floor() as i32)) & 1 == 0;
                let base = if checker {
                    Rgb(51, 58, 62)
                } else {
                    Rgb(40, 47, 53)
                };
                base.fog(distance)
            } else {
                let v = ((yf - top) / size).clamp(0.0, 1.0);
                let brick = v * 7.0;
                let u = hit.texture * 5.0 + if brick as i32 % 2 == 0 { 0.5 } else { 0.0 };
                if v < 0.035 {
                    Rgb(195, 115, 55).fog(hit.distance)
                } else if brick.fract() < 0.07 || u.fract() < 0.06 {
                    wall_color.scale(0.62)
                } else {
                    wall_color
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
        let half = (size / 2).max(1);
        let bottom = (horizon + h as f32 / depth / 2.0) as i32;
        let top = bottom - size;
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
                    demon_pixel(u, v, game.time, body, enemy.hp <= 0)
                };
                if let Some(color) = color {
                    frame.pixel(x, y, color.fog(depth));
                }
            }
        }
    }
    // Weapon recoil is independent from the camera and aiming point.
    let cx = w as i32 / 2;
    let cy = h as i32 / 2;
    let recoil = (game.muzzle / 0.07 * 2.0) as i32;
    let reload_drop = if game.player.reload > 0.0 { 4 } else { 0 };
    let gun_top = h as i32 - 12 + recoil + reload_drop;
    for y in gun_top.max(cy + 6)..h as i32 {
        let dy = y - gun_top;
        let half = if dy < 5 {
            2
        } else if dy < 9 {
            7
        } else {
            4
        };
        for x in cx - half..=cx + half {
            let color = if dy < 5 {
                if x == cx { DIM } else { WHITE.scale(0.7) }
            } else if dy < 9 {
                Rgb(114, 81, 43)
            } else {
                Rgb(53, 43, 36)
            };
            frame.pixel(x, y, color);
        }
    }
    if game.muzzle > 0.0 {
        for y in gun_top - 5..gun_top {
            for x in cx - 3..=cx + 3 {
                if (x - cx).abs() + (y - gun_top + 2).abs() < 4 {
                    frame.pixel(x, y, GOLD);
                }
            }
        }
    }
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

fn demon_pixel(u: f32, v: f32, time: f32, body: Rgb, dead: bool) -> Option<Rgb> {
    if dead {
        return if v > 0.8 && u > 0.15 && u < 0.85 {
            Some(body.scale(0.4))
        } else {
            None
        };
    }
    if v < 0.28 {
        if v < 0.12 && ((0.23..0.33).contains(&u) || (0.67..0.77).contains(&u)) {
            return Some(body);
        }
        if !(0.31..0.69).contains(&u) || v < 0.08 {
            return None;
        }
        if (0.14..0.21).contains(&v) && ((0.35..0.43).contains(&u) || (0.57..0.65).contains(&u)) {
            return Some(GOLD);
        }
        return Some(body);
    }
    if v < 0.66 {
        return if (0.12..0.88).contains(&u) {
            Some(body.scale(if (0.3..0.7).contains(&u) { 1.0 } else { 0.7 }))
        } else {
            None
        };
    }
    let wobble = (time * 7.0).sin() * 0.035;
    if (0.24 + wobble..0.43 + wobble).contains(&u) || (0.57 - wobble..0.76 - wobble).contains(&u) {
        Some(body.scale(0.6))
    } else {
        None
    }
}

fn minimap(frame: &mut Frame, game: &Game) {
    let ox = 2_i32;
    let oy = 2_i32;
    for y in -1..=world::HEIGHT as i32 {
        for x in -1..=world::WIDTH as i32 {
            let color = if x < 0 || y < 0 || x == world::WIDTH as i32 || y == world::HEIGHT as i32 {
                DIM
            } else if world::wall(x, y) {
                MM_WALL
            } else {
                BG
            };
            frame.pixel(ox + x, oy + y, color);
        }
    }
    for pickup in &game.pickups {
        let (x, y) = pickup.pos.cell();
        frame.pixel(
            ox + x,
            oy + y,
            if pickup.kind == PickupKind::Medkit {
                GREEN.scale(0.5)
            } else {
                GOLD
            },
        );
    }
    for enemy in game.enemies.iter().filter(|e| e.alive()) {
        let (x, y) = enemy.pos.cell();
        frame.pixel(ox + x, oy + y, RED);
    }
    let (x, y) = game.player.pos.cell();
    let facing = Vec2::facing(game.player.angle);
    let (tx, ty) = (x + facing.x.round() as i32, y + facing.y.round() as i32);
    if !world::wall(tx, ty)
        && world::visible(game.player.pos, Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5))
    {
        frame.pixel(ox + tx, oy + ty, WHITE);
    }
    frame.pixel(ox + x, oy + y, GREEN);
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
}

fn menu(frame: &mut Frame, game: &Game, stats: Stats) {
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
            let lines = [
                "RUST EDITION",
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
                frame.center(4 + i, line, if i == 7 { GOLD } else { WHITE });
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
