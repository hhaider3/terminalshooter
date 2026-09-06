use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use std::ops::{Add, Mul, Sub};

pub const MAP: [&str; 15] = [
    "###################",
    "#.................#",
    "#.................#",
    "#....##....##.....#",
    "#....##....##.....#",
    "#.................#",
    "#.................#",
    "#.................#",
    "#.................#",
    "#....##....##.....#",
    "#....##....##.....#",
    "#.................#",
    "#.................#",
    "#.................#",
    "###################",
];
pub const WIDTH: usize = 19;
pub const HEIGHT: usize = MAP.len();
pub const CELLS: usize = WIDTH * HEIGHT;
pub const FAR: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }
    pub fn unit(self) -> Self {
        let length = self.length();
        if length > 0.0001 {
            self * (1.0 / length)
        } else {
            Self::default()
        }
    }
    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }
    pub fn cell(self) -> (i32, i32) {
        (self.x.floor() as i32, self.y.floor() as i32)
    }
    pub fn facing(angle: f32) -> Self {
        Self::new(angle.cos(), angle.sin())
    }
}
impl Add for Vec2 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, scale: f32) -> Self {
        Self::new(self.x * scale, self.y * scale)
    }
}

pub fn normalize_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}
pub fn wall(x: i32, y: i32) -> bool {
    x < 0
        || y < 0
        || x >= WIDTH as i32
        || y >= HEIGHT as i32
        || MAP[y as usize].as_bytes()[x as usize] == b'#'
}
pub fn solid(p: Vec2) -> bool {
    let (x, y) = p.cell();
    wall(x, y)
}
pub fn fits(p: Vec2, radius: f32) -> bool {
    for x in [(p.x - radius).floor() as i32, (p.x + radius).floor() as i32] {
        for y in [(p.y - radius).floor() as i32, (p.y + radius).floor() as i32] {
            if wall(x, y) {
                return false;
            }
        }
    }
    true
}

pub fn slide(p: &mut Vec2, delta: Vec2, radius: f32) {
    let steps = (delta.length() / 0.1).ceil().max(1.0) as usize;
    let step = delta * (1.0 / steps as f32);
    for _ in 0..steps {
        let x = Vec2::new(p.x + step.x, p.y);
        if fits(x, radius) {
            p.x = x.x;
        }
        let y = Vec2::new(p.x, p.y + step.y);
        if fits(y, radius) {
            p.y = y.y;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub distance: f32,
    pub side: bool,
    pub texture: f32,
}

/// Ray parameter t, not Euclidean length: camera-plane rays have no fisheye.
pub fn cast(origin: Vec2, direction: Vec2) -> RayHit {
    let (mut mx, mut my) = origin.cell();
    let dx = if direction.x.abs() < 1e-8 {
        f32::INFINITY
    } else {
        direction.x.recip().abs()
    };
    let dy = if direction.y.abs() < 1e-8 {
        f32::INFINITY
    } else {
        direction.y.recip().abs()
    };
    let sx = if direction.x < 0.0 { -1 } else { 1 };
    let sy = if direction.y < 0.0 { -1 } else { 1 };
    let mut tx = if sx < 0 {
        origin.x - mx as f32
    } else {
        mx as f32 + 1.0 - origin.x
    } * dx;
    let mut ty = if sy < 0 {
        origin.y - my as f32
    } else {
        my as f32 + 1.0 - origin.y
    } * dy;
    for _ in 0..128 {
        let (distance, side) = if tx < ty {
            let t = tx;
            tx += dx;
            mx += sx;
            (t, false)
        } else {
            let t = ty;
            ty += dy;
            my += sy;
            (t, true)
        };
        if wall(mx, my) || distance > FAR {
            let hit = origin + direction * distance;
            return RayHit {
                distance: distance.clamp(0.001, FAR),
                side,
                texture: if side {
                    hit.x.fract().abs()
                } else {
                    hit.y.fract().abs()
                },
            };
        }
    }
    RayHit {
        distance: FAR,
        side: false,
        texture: 0.0,
    }
}

pub fn visible(a: Vec2, b: Vec2) -> bool {
    let delta = b - a;
    let distance = delta.length();
    distance < 0.001 || cast(a, delta.unit()).distance + 0.001 >= distance
}

pub fn routes(target: Vec2) -> [u16; CELLS] {
    let mut result = [u16::MAX; CELLS];
    let (x, y) = target.cell();
    if wall(x, y) {
        return result;
    }
    let mut queue = VecDeque::from([(x, y)]);
    result[y as usize * WIDTH + x as usize] = 0;
    while let Some((x, y)) = queue.pop_front() {
        let next = result[y as usize * WIDTH + x as usize] + 1;
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if wall(nx, ny) {
                continue;
            }
            let index = ny as usize * WIDTH + nx as usize;
            if result[index] == u16::MAX {
                result[index] = next;
                queue.push_back((nx, ny));
            }
        }
    }
    result
}

pub fn waypoint(pos: Vec2, field: &[u16; CELLS]) -> Option<Vec2> {
    let (x, y) = pos.cell();
    if wall(x, y) {
        return None;
    }
    let current = field[y as usize * WIDTH + x as usize];
    let next = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
        .into_iter()
        .filter(|&(nx, ny)| !wall(nx, ny) && field[ny as usize * WIDTH + nx as usize] < current)
        .min_by_key(|&(nx, ny)| field[ny as usize * WIDTH + nx as usize])?;
    if (next.0 != x && (pos.y - y as f32 - 0.5).abs() > 0.06)
        || (next.1 != y && (pos.x - x as f32 - 0.5).abs() > 0.06)
    {
        Some(Vec2::new(x as f32 + 0.5, y as f32 + 0.5))
    } else {
        Some(Vec2::new(next.0 as f32 + 0.5, next.1 as f32 + 0.5))
    }
}

#[derive(Clone, Debug)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn index(&mut self, length: usize) -> usize {
        (self.next_u64() % length as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arena_is_connected() {
        let distances = routes(Vec2::new(9.5, 12.5));
        for (y, row) in MAP.iter().enumerate() {
            assert_eq!(row.len(), WIDTH);
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'.' {
                    assert_ne!(distances[y * WIDTH + x], u16::MAX);
                }
            }
        }
    }
    #[test]
    fn rays_and_cover() {
        assert!((cast(Vec2::new(1.5, 1.5), Vec2::new(-1.0, 0.0)).distance - 0.5).abs() < 0.001);
        assert!(!visible(Vec2::new(4.5, 3.5), Vec2::new(7.5, 3.5)));
        assert!(solid(Vec2::new(-0.01, 2.0)));
        let mut pos = Vec2::new(4.5, 3.5);
        slide(&mut pos, Vec2::new(5.0, 0.0), 0.22);
        assert!(pos.x < 4.8);
    }
}
