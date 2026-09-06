//! Export the actual software frame as PPM for visual QA without a desktop UI.
use std::{
    fs::File,
    io::{self, Write},
};
use terminalshooter::{
    game::{Enemy, Game, Kind, Phase},
    render::{self, ColorMode, Stats, TerminalRenderer},
    world::Vec2,
};

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args.first().expect(
        "Usage: preview OUTPUT.ppm|OUTPUT.ansi [play|title|pause|hit|reload] [WIDTH HEIGHT] [256]",
    );
    let mut game = Game::new(7);
    game.start();
    game.enemies = [(7.5, Kind::Brute), (9.5, Kind::Grunt), (10.9, Kind::Runner)]
        .into_iter()
        .map(|(x, kind)| {
            let mut e = Enemy::new(Vec2::new(x, 7.5), kind);
            e.wake = 0.0;
            e
        })
        .collect();
    game.say("SECTOR 07 // HOLD THE PERIMETER");
    match args.get(1).map(String::as_str).unwrap_or("play") {
        "play" => {}
        "title" => game.phase = Phase::Title,
        "pause" => game.phase = Phase::Paused,
        "hit" => {
            game.muzzle = 0.07;
            game.hit = 0.1;
            game.enemies[1].flash = 0.1;
        }
        "reload" => game.player.reload = 0.4,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown preview scene",
            ));
        }
    }
    let width = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(120);
    let height = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(36);
    if width < 40 || height < 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Preview must be at least 40x16",
        ));
    }
    let frame = render::draw(
        &game,
        width,
        height,
        Stats {
            fps: 60.0,
            release_events: false,
        },
    );
    let mut out = File::create(path)?;
    if path.ends_with(".ansi") {
        let mode = if args.get(4).is_some_and(|v| v == "256") {
            ColorMode::Palette
        } else {
            ColorMode::TrueColor
        };
        return out.write_all(TerminalRenderer::new(mode).encode(&frame).as_bytes());
    }
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
