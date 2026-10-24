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
    pub fn reset(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cells.clear();
        self.cells.resize(width * height, Cell::default());
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
    draw_into(&mut frame, game, width, height, stats);
    frame
}

/// Draw into an existing frame, reusing its cell buffer across frames.
pub fn draw_into(frame: &mut Frame, game: &Game, width: usize, height: usize, stats: Stats) {
    frame.reset(width, height);
    if width < 40 || height < 16 {
        frame.center(height / 2, "Resize terminal to at least 40x16", GOLD);
        return;
    }
    if game.phase != Phase::Playing {
        menu(frame, game, stats);
        return;
    }
    scene(frame, game);
    if game.minimap {
        minimap(frame, game);
    }
    hud(frame, game, stats);
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
        // Smooth attenuation preserves depth as output resolution increases.
        let light = 1.0 / (1.0 + hit.distance * 0.06);
        // Skyline is anchored to world bearing, not the screen.
        let azimuth = game.player.angle + (camera_x * plane_scale).atan();
        let sector = ((azimuth + std::f32::consts::TAU) * 55.0) as i32;
        let tower = ((sector / 4).rem_euclid(7) * 3 + 5) as f32;
        let mx = x as f32 - moon_x;
        let radius = h as f32 * 0.065;
        for y in 0..h {
            let yf = y as f32 + 0.5;
            let color = if yf < top {
                let band = yf / horizon;
                let sky = Rgb(7, 13, 27).mix(Rgb(42, 64, 77), band * band);
                let my = yf - h as f32 * 0.16;
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
                base.scale(1.0 / (1.0 + distance * 0.05))
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
                let bevel = 0.82 + (1.0 - u) * 0.18 + (1.0 - v) * 0.12;
                let detail = if size > 28.0 {
                    let bolt = ((u - 0.12).abs().min((u - 0.88).abs()) / 0.018).powi(2)
                        + ((v - 0.28).abs().min((v - 0.78).abs()) / 0.014).powi(2);
                    if bolt < 1.0 {
                        WHITE.scale(0.65)
                    } else if (0.72..0.76).contains(&v) && (0.3..0.7).contains(&u) {
                        if (u * 48.0).fract() < 0.35 {
                            Rgb(18, 28, 35)
                        } else {
                            panel
                        }
                    } else {
                        panel.scale(bevel)
                    }
                } else {
                    panel.scale(bevel)
                };
                let panel = detail.scale(light);
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
        // Project width like the walls so the sprite matches the shotgun's hit test.
        let half_width = if pickup {
            0.225
        } else {
            game.enemies[index].kind.half_width()
        };
        let half = (half_width * w as f32 * 0.5 / (depth * plane_scale)).max(1.0) as i32;
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
        let stride =
            if !pickup && game.enemies[index].windup == 0.0 && game.enemies[index].wake == 0.0 {
                (game.time * 7.0 + index as f32).sin() * 0.18
            } else {
                0.0
            };
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
                    enemy_pixel(u, v, body, enemy.kind, enemy.hp <= 0, stride, size >= 30)
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

fn enemy_pixel(
    u: f32,
    v: f32,
    body: Rgb,
    kind: Kind,
    dead: bool,
    stride: f32,
    detail: bool,
) -> Option<Rgb> {
    if dead {
        return if v > 0.87 && (0.1..0.9).contains(&u) {
            Some(body.scale(0.3))
        } else {
            None
        };
    }
    crate::art::enemy(u, v, body, kind, stride, detail)
}

fn weapon(frame: &mut Frame, game: &Game, scene_height: usize) {
    let height = (scene_height as f32 * 0.38).clamp(8.0, 72.0) as i32;
    let width = height * 18 / 10;
    let reload = if game.player.reload > 0.0 {
        (std::f32::consts::PI * game.player.reload / RELOAD_TIME).sin()
    } else {
        0.0
    };
    let recoil = (game.muzzle / 0.07 * 3.0) as i32;
    let ox = frame.width as i32 / 2 + frame.width as i32 / 9 - width / 2;
    let oy = scene_height as i32 - height + recoil + (reload * height as f32 * 0.65) as i32;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0_u32; 3];
            let mut coverage = 0;
            for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                if let Some(Rgb(r, g, b)) = crate::art::weapon(
                    (x as f32 + dx) / width as f32,
                    (y as f32 + dy) / height as f32,
                    reload,
                ) {
                    sum[0] += r as u32;
                    sum[1] += g as u32;
                    sum[2] += b as u32;
                    coverage += 1;
                }
            }
            if coverage > 0 && oy + y < scene_height as i32 {
                frame.shade(
                    ox + x,
                    oy + y,
                    Rgb(
                        (sum[0] / coverage) as u8,
                        (sum[1] / coverage) as u8,
                        (sum[2] / coverage) as u8,
                    ),
                    coverage as f32 / 4.0,
                );
            }
        }
    }

    if game.muzzle > 0.0 {
        let mx = ox + (width as f32 * crate::art::MUZZLE.0) as i32;
        let my = oy + (height as f32 * crate::art::MUZZLE.1) as i32;
        for y in -7_i32..2 {
            for x in -8_i32..=8 {
                let r = x.abs() + (y + 2).abs();
                if r < 6 || (y == -2 && x.abs() < 8) {
                    frame.pixel(mx + x, my + y, if r < 3 { WHITE } else { GOLD });
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
        "{}  LEFT {}  SCORE {}",
        game.wave_label(),
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
            let top = if spacious { 14 } else { 4 };
            for (i, line) in lines.iter().enumerate() {
                frame.center(top + i, line, if i == 7 { GOLD } else { WHITE });
            }
            // Clear of the footer, which starts two rows above the bottom.
            if game.best > 0 {
                frame.center(
                    top + lines.len() + 1,
                    &format!("BEST SCORE {}", game.best),
                    DIM,
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
                "Pointer at window edge keeps turning.",
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
            let mut mark = |pos: Vec2, glyph, color| {
                let (mx, my) = pos.cell();
                if my as usize >= start && (my as usize) < start + available {
                    frame.text(x + mx as usize, 4 + my as usize - start, glyph, color);
                }
            };
            for pickup in &game.pickups {
                match pickup.kind {
                    PickupKind::Medkit => mark(pickup.pos, "+", GREEN),
                    PickupKind::Ammo => mark(pickup.pos, "*", GOLD),
                }
            }
            for enemy in game.enemies.iter().filter(|e| e.alive()) {
                mark(enemy.pos, "E", RED);
            }
            mark(game.player.pos, "@", GREEN);
            // The map uses at most height - 7 rows from row 4, so this row is free.
            frame.center(frame.height - 3, "@ YOU  E HOSTILE  + MEDKIT  * AMMO", DIM);
            frame.center(frame.height - 2, "M / P / ESC: back", GOLD);
        }
        Phase::Won | Phase::Lost => {
            let middle = frame.height / 2;
            frame.center(
                middle - 3,
                &format!("SCORE {}   KILLS {}", game.score, game.kills),
                WHITE,
            );
            if game.new_best {
                frame.center(middle - 2, "NEW BEST SCORE", GOLD);
            } else if game.best > 0 {
                frame.center(middle - 2, &format!("BEST {}", game.best), DIM);
            }
            frame.center(
                middle,
                &format!("{}   TIME {:.0}s", game.wave_label(), game.time),
                DIM,
            );
            frame.center(middle + 2, "R / ENTER: new run   CTRL-C: quit", GOLD);
            if game.phase == Phase::Won {
                frame.center(middle + 3, "C: continue in endless mode", GOLD);
            }
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

/// Foreground of a space, which is never visible and so never needs setting.
const ANY: u32 = u32::MAX;

fn digits(n: usize) -> usize {
    n.checked_ilog10().unwrap_or(0) as usize + 1
}

/// Length of a cursor-forward sequence; one column needs no count.
fn forward_len(columns: usize) -> usize {
    if columns == 1 { 3 } else { 3 + digits(columns) }
}

fn forward(out: &mut String, columns: usize) {
    if columns == 1 {
        out.push_str("\x1b[C");
    } else {
        let _ = write!(out, "\x1b[{columns}C");
    }
}

/// Emit the shortest move from a known cursor position, or an absolute one.
fn move_cursor(out: &mut String, from: Option<(usize, usize)>, row: usize, col: usize) {
    let absolute = 4 + digits(row + 1) + digits(col + 1);
    match from {
        Some((r, c)) if r == row && col > c && forward_len(col - c) < absolute => {
            forward(out, col - c);
        }
        // Rows below always exist, so line feeds never scroll the screen.
        Some((r, _))
            if row > r && 1 + (row - r) + if col > 0 { forward_len(col) } else { 0 } < absolute =>
        {
            out.push('\r');
            (r..row).for_each(|_| out.push('\n'));
            if col > 0 {
                forward(out, col);
            }
        }
        _ => {
            let _ = write!(out, "\x1b[{};{}H", row + 1, col + 1);
        }
    }
}

pub struct TerminalRenderer {
    pub mode: ColorMode,
    previous: Vec<Encoded>,
    current: Vec<Encoded>,
    size: (usize, usize),
}
impl TerminalRenderer {
    pub fn new(mode: ColorMode) -> Self {
        Self {
            mode,
            previous: Vec::new(),
            current: Vec::new(),
            size: (0, 0),
        }
    }
    pub fn invalidate(&mut self) {
        self.previous.clear();
    }
    pub fn encode(&mut self, frame: &Frame) -> String {
        let mut out = String::new();
        self.encode_into(frame, &mut out);
        out
    }
    /// Replace `out` with the update for `frame`; empty when nothing changed.
    pub fn encode_into(&mut self, frame: &Frame, out: &mut String) {
        out.clear();
        let mode = self.mode;
        // 64 levels per channel look identical in these scenes, but let far more
        // cells match the previous frame or reuse the current colour.
        let level = |c: u8| u32::from(c) / 4 * 4 + 2;
        let encode = |rgb: Rgb| match mode {
            ColorMode::Palette => palette(rgb),
            ColorMode::TrueColor => (level(rgb.0) << 16) | (level(rgb.1) << 8) | level(rgb.2),
        };
        self.current.clear();
        self.current.extend(frame.cells.iter().map(|c| {
            let (fg, bg) = (encode(c.fg), encode(c.bg));
            // A half block whose halves match shows only its background, like a space.
            if c.glyph == ' ' || (c.glyph == '▀' && fg == bg) {
                Encoded {
                    glyph: ' ',
                    fg: ANY,
                    bg,
                }
            } else {
                Encoded {
                    glyph: c.glyph,
                    fg,
                    bg,
                }
            }
        }));
        let full =
            self.size != (frame.width, frame.height) || self.previous.len() != self.current.len();
        if !full && self.current == self.previous {
            return;
        }
        out.push_str("\x1b[?2026h");
        if full {
            out.push_str("\x1b[0m\x1b[2J");
        }
        let mut fg = None;
        let mut bg = None;
        let mut cursor = None;
        for (i, cell) in self.current.iter().enumerate() {
            if !full && *cell == self.previous[i] {
                continue;
            }
            let (row, col) = (i / frame.width, i % frame.width);
            if cursor != Some((row, col)) {
                move_cursor(out, cursor, row, col);
            }
            let set_fg = cell.fg != ANY && fg != Some(cell.fg);
            let set_bg = bg != Some(cell.bg);
            if set_fg || set_bg {
                out.push_str("\x1b[");
                if set_fg {
                    self.color(out, cell.fg, true);
                    fg = Some(cell.fg);
                }
                if set_fg && set_bg {
                    out.push(';');
                }
                if set_bg {
                    self.color(out, cell.bg, false);
                    bg = Some(cell.bg);
                }
                out.push('m');
            }
            out.push(cell.glyph);
            // Line wrap is off, so writing the last column leaves the cursor there.
            cursor = Some((row, (col + 1).min(frame.width - 1)));
        }
        out.push_str("\x1b[0m\x1b[?2026l");
        std::mem::swap(&mut self.previous, &mut self.current);
        self.size = (frame.width, frame.height);
    }
    /// Write SGR colour parameters, without the introducer or final `m`.
    fn color(&self, out: &mut String, value: u32, fg: bool) {
        let prefix = if fg { 38 } else { 48 };
        match self.mode {
            ColorMode::Palette => {
                let _ = write!(out, "{prefix};5;{value}");
            }
            ColorMode::TrueColor => {
                let _ = write!(
                    out,
                    "{prefix};2;{};{};{}",
                    (value >> 16) & 255,
                    (value >> 8) & 255,
                    value & 255
                );
            }
        }
    }
}
