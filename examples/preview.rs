//! Export the actual software frame as PPM for visual QA without a desktop UI.
use std::{
    fs::File,
    io::{self, Write},
};
use terminalshooter::{
    game::{Enemy, Game, Kind},
    render::{self, Stats},
    world::Vec2,
};

fn main() -> io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: cargo run --release --example preview -- /tmp/breach.ppm");
    let mut game = Game::new(7);
    game.start();
    game.enemies = [(7.5, Kind::Brute), (9.5, Kind::Grunt), (11.5, Kind::Runner)]
        .into_iter()
        .map(|(x, kind)| {
            let mut e = Enemy::new(Vec2::new(x, 7.5), kind);
            e.wake = 0.0;
            e
        })
        .collect();
    let frame = render::draw(&game, 120, 36, Stats::default());
    let mut out = File::create(path)?;
    let height = (frame.height - 4) * 12;
    write!(out, "P6\n{} {}\n255\n", frame.width * 6, height)?;
    for y in 0..height {
        for x in 0..frame.width * 6 {
            let cell = frame.cells[y / 12 * frame.width + x / 6];
            let color = if cell.glyph == '▀' && (y / 6) % 2 == 0 {
                cell.fg
            } else {
                cell.bg
            };
            out.write_all(&[color.0, color.1, color.2])?;
        }
    }
    Ok(())
}
