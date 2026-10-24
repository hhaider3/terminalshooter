use crossterm::{cursor, event, execute, terminal};
mod pacing;
use pacing::FramePacer;
use std::error::Error;
use std::io::{self, IsTerminal, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terminalshooter::{
    game::{Controls, Game, Phase},
    input::Input,
    keyboard,
    profile::Profile,
    render::{self, ColorMode, Frame, Stats, TerminalRenderer},
};

const HELP: &str = "TERMINAL // BREACH — Rust arena FPS

Usage: terminalshooter [options]
  --256            Use 256 colors (less terminal output)
  --truecolor      Use 24-bit color
  --fps N          Target 15–120 FPS (default 60)
  --large          Viewport up to 220x70 (default 160x50)
  --compact        Viewport up to 120x36 for slower terminals
  --no-mouse       Start with mouse-look disabled
  --cell-mouse     Aim in whole cells even where pixel mouse reports work
  --seed N         Reproducible enemy spawns
  --bench          Benchmark rendering; no terminal needed
  --setup-keyboard  Enable macOS held-key access (run once in your terminal)
  --help           Show this help

WASD move, Q/E turn, Space/click shotgun, F toggle-fire, X dodge,
R reload, P/Esc pause, M minimap, Shift+M tactical map, H help,
L mouse-look on/off, Ctrl-C quit. Mouse-look uses terminal events;
it does not lock the desktop pointer. Rest the pointer at a window edge
to keep turning. Keyboard-only play is supported.";

#[derive(Debug)]
struct Options {
    color: ColorMode,
    fps: u32,
    large: bool,
    compact: bool,
    mouse: bool,
    pixel_mouse: bool,
    seed: u64,
    bench: bool,
}
impl Options {
    fn parse() -> Result<Option<Self>, String> {
        let env = |name: &str| std::env::var(name).unwrap_or_default();
        let color = if env("TERM_PROGRAM") != "Apple_Terminal"
            && (matches!(env("COLORTERM").as_str(), "truecolor" | "24bit")
                || matches!(
                    env("TERM_PROGRAM").as_str(),
                    "iTerm.app" | "WezTerm" | "ghostty" | "vscode"
                )
                || !env("WT_SESSION").is_empty())
        {
            ColorMode::TrueColor
        } else {
            ColorMode::Palette
        };
        let mut result = Self {
            color,
            fps: 60,
            large: false,
            compact: false,
            mouse: true,
            pixel_mouse: true,
            seed: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            bench: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{HELP}");
                    return Ok(None);
                }
                "--setup-keyboard" => {
                    keyboard::setup();
                    return Ok(None);
                }
                "--256" => result.color = ColorMode::Palette,
                "--truecolor" => result.color = ColorMode::TrueColor,
                "--large" => {
                    result.large = true;
                    result.compact = false;
                }
                "--compact" => {
                    result.compact = true;
                    result.large = false;
                }
                "--no-mouse" => result.mouse = false,
                "--cell-mouse" => result.pixel_mouse = false,
                "--bench" => result.bench = true,
                "--fps" => {
                    result.fps = args
                        .next()
                        .ok_or("--fps needs a number")?
                        .parse()
                        .map_err(|_| "Invalid FPS")?;
                    if !(15..=120).contains(&result.fps) {
                        return Err("--fps must be 15–120".into());
                    }
                }
                "--seed" => {
                    result.seed = args
                        .next()
                        .ok_or("--seed needs a number")?
                        .parse()
                        .map_err(|_| "Invalid seed")?
                }
                _ => return Err(format!("Unknown option: {arg}. Use --help.")),
            }
        }
        Ok(Some(result))
    }
}

/// Pixels per cell, when the terminal reports its size in pixels.
fn cell_size() -> Option<(f32, f32)> {
    let size = terminal::window_size().ok()?;
    (size.width > 0 && size.height > 0 && size.columns > 0 && size.rows > 0).then(|| {
        (
            f32::from(size.width) / f32::from(size.columns),
            f32::from(size.height) / f32::from(size.rows),
        )
    })
}

/// Terminals known to honour SGR pixel mouse reports (mode 1016). Others may
/// ignore the request and keep sending cells, which would be misread as
/// pixels, so they keep cell reports. Multiplexers do not forward the mode.
fn pixel_mouse_supported() -> bool {
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    let multiplexed = ["TMUX", "STY", "ZELLIJ"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
    let known = env("TERM") == "xterm-kitty"
        || env("TERM").starts_with("foot")
        || matches!(env("TERM_PROGRAM").as_str(), "WezTerm" | "ghostty");
    !multiplexed && known && cell_size().is_some()
}

struct Terminal {
    enhanced: bool,
    pixel_mouse: bool,
    restored: Arc<AtomicBool>,
}
impl Terminal {
    fn enter(pixel_mouse: bool) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut guard = Self {
            enhanced: false,
            pixel_mouse: false,
            restored: Arc::new(AtomicBool::new(false)),
        };
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::DisableLineWrap,
            event::EnableMouseCapture,
            event::EnableFocusChange
        )?;
        // Pixel reports reuse the SGR format that mouse capture just enabled.
        if pixel_mouse && pixel_mouse_supported() {
            io::stdout().write_all(b"\x1b[?1016h")?;
            io::stdout().flush()?;
            guard.pixel_mouse = true;
        }
        // Crossterm bounds this protocol query; unsupported terminals use repeat.
        if terminal::supports_keyboard_enhancement().unwrap_or(false) {
            execute!(
                io::stdout(),
                event::PushKeyboardEnhancementFlags(
                    event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
            guard.enhanced = true;
        }
        Ok(guard)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        restore(self.enhanced, self.pixel_mouse, &self.restored);
    }
}
fn restore(enhanced: bool, pixel_mouse: bool, restored: &AtomicBool) {
    // The panic hook and unwinding guard share this flag so protocol stacks
    // are popped once, even when both cleanup paths run.
    if restored.swap(true, Ordering::Relaxed) {
        return;
    }
    let mut out = io::stdout();
    // End a partially written synchronized frame before restoring the display.
    let _ = execute!(out, terminal::EndSynchronizedUpdate);
    if enhanced {
        let _ = execute!(out, event::PopKeyboardEnhancementFlags);
    }
    if pixel_mouse {
        let _ = out.write_all(b"\x1b[?1016l");
    }
    let _ = execute!(
        out,
        event::DisableMouseCapture,
        event::DisableFocusChange,
        crossterm::style::ResetColor,
        terminal::EnableLineWrap,
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

fn run(options: Options) -> Result<(), Box<dyn Error>> {
    if options.bench {
        benchmark(&options);
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "Run in an interactive terminal, or use --bench for headless rendering.".into(),
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    let handler_stop = Arc::clone(&stop);
    ctrlc::set_handler(move || {
        handler_stop.store(true, Ordering::Relaxed);
    })?;
    let terminal = Terminal::enter(options.pixel_mouse)?;
    // Restore before printing a panic, so it remains readable on the main screen.
    let previous_hook = std::panic::take_hook();
    let (enhanced, pixel_mouse) = (terminal.enhanced, terminal.pixel_mouse);
    let restored = Arc::clone(&terminal.restored);
    std::panic::set_hook(Box::new(move |info| {
        restore(enhanced, pixel_mouse, &restored);
        previous_hook(info);
    }));
    let mut game = Game::new(options.seed);
    let profile_path = Profile::path();
    let mut profile = profile_path
        .as_deref()
        .map(Profile::load)
        .unwrap_or_default();
    profile.apply(&mut game);
    let saved_mouse = profile.mouse;
    game.mouse &= options.mouse;
    let mut save = |game: &Game| -> io::Result<()> {
        profile.update(game);
        // --no-mouse applies to this launch only, unless L turned mouse-look back on.
        if !options.mouse && !game.mouse {
            profile.mouse = saved_mouse;
        }
        profile_path
            .as_deref()
            .map_or(Ok(()), |path| profile.save(path))
    };
    let native_keys = keyboard::local_key_state();
    let mut input = Input::with_key_state(options.seed, terminal.enhanced, native_keys);
    if !terminal.enhanced && native_keys.is_none() {
        game.say("KEY REPEAT MODE // Run ./play.sh --setup-keyboard for smooth holds");
    }
    let mut renderer = TerminalRenderer::new(options.color);
    let mut frame = Frame::new(0, 0);
    let mut encoded = String::new();
    let origin = Instant::now();
    let mut previous = origin;
    let mut accumulator = 0.0_f64;
    let mut fps = options.fps as f32;
    let mut fps_since = origin;
    let mut fps_frames = 0;
    let frame_duration = Duration::from_secs_f64(1.0 / options.fps as f64);
    let mut pacer = FramePacer::new(origin, frame_duration);
    let mut resize_pause = false;
    let mut out = io::stdout();
    let mut run_over = false;
    'frames: while !stop.load(Ordering::Relaxed) {
        let frame_start = Instant::now();
        let elapsed = frame_start.duration_since(previous).as_secs_f64();
        previous = frame_start;
        fps_frames += 1;
        let fps_elapsed = frame_start.duration_since(fps_since).as_secs_f32();
        if fps_elapsed >= 0.5 {
            fps = fps_frames as f32 / fps_elapsed;
            fps_since = frame_start;
            fps_frames = 0;
        }
        let (cols, rows) = terminal::size()?;
        let too_small = cols < 40 || rows < 16;
        // The pointer stops at the window edge, not the capped viewport's.
        input.screen_width = cols;
        if terminal.pixel_mouse {
            // Cell size changes with the font as well as the window.
            input.cell_size = cell_size();
        }
        if too_small {
            if game.phase == Phase::Playing {
                game.overlay(Phase::Paused);
                resize_pause = true;
            }
            input.clear();
            accumulator = 0.0;
        } else if resize_pause {
            game.phase = Phase::Playing;
            input.clear();
            resize_pause = false;
        }
        let shots_before = game.shots_fired;
        // Drain a bounded batch so a flood of mouse reports cannot starve frames.
        for _ in 0..256 {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            let ev = event::read()?;
            if ev == event::Event::FocusLost {
                // Growing the window must not resume an unfocused game.
                resize_pause = false;
            }
            if too_small
                && !matches!(ev, event::Event::FocusLost | event::Event::Resize(_, _))
                && !matches!(ev, event::Event::Key(k) if k.code == event::KeyCode::Char('c') && k.modifiers.contains(event::KeyModifiers::CONTROL))
            {
                continue;
            }
            let outcome = input.handle(ev, origin.elapsed().as_secs_f64(), &mut game);
            if outcome.quit {
                break 'frames;
            }
            if outcome.redraw {
                renderer.invalidate();
            }
        }
        let health_before = game.player.hp;
        let phase_before = game.phase;
        // Fixed 120-Hz simulation; output FPS cannot change movement/fire rates.
        if !too_small && game.phase == Phase::Playing {
            accumulator = (accumulator + elapsed).min(0.1);
            while accumulator >= 1.0 / 120.0 {
                game.tick(
                    1.0 / 120.0,
                    input.take_controls(origin.elapsed().as_secs_f64()),
                );
                accumulator -= 1.0 / 120.0;
            }
        } else {
            accumulator = 0.0;
        }
        if phase_before != game.phase {
            input.clear();
            game.auto_fire = false;
        }
        // Save the best score as soon as a run ends, in case the game is killed.
        // The ending shot may come from a click handled above, not a tick.
        let over = matches!(game.phase, Phase::Won | Phase::Lost);
        if over && !run_over {
            let _ = save(&game);
        }
        run_over = over;
        if game.sound && (game.shots_fired > shots_before || game.player.hp < health_before) {
            out.write_all(b"\x07")?;
        }
        let (max_cols, max_rows) = if options.compact {
            (120, 36)
        } else if options.large {
            (220, 70)
        } else {
            (160, 50)
        };
        render::draw_into(
            &mut frame,
            &game,
            cols.min(max_cols).max(1) as usize,
            rows.min(max_rows).max(1) as usize,
            Stats {
                fps,
                release_events: terminal.enhanced,
            },
        );
        renderer.encode_into(&frame, &mut encoded);
        if !encoded.is_empty() {
            out.write_all(encoded.as_bytes())?;
            out.flush()?;
        }
        pacer.wait();
    }
    // Restore the screen first so a save error stays readable.
    drop(terminal);
    if let Err(error) = save(&game) {
        eprintln!("terminalshooter: could not save settings: {error}");
    }
    Ok(())
}

fn benchmark(options: &Options) {
    println!("Headless render + ANSI encoding; excludes terminal drawing and output latency.");
    for (width, height) in [(100, 30), (120, 36), (160, 50), (220, 70)] {
        let mut game = Game::new(7);
        game.start();
        let mut renderer = TerminalRenderer::new(options.color);
        let mut frame = Frame::new(0, 0);
        let mut encoded = String::new();
        let start = Instant::now();
        let mut bytes = 0;
        for _ in 0..600 {
            // Keep every sample in gameplay, even when enemies reach the camera.
            game.player.hp = 100;
            game.tick(
                1.0 / 60.0,
                Controls {
                    turn: 0.2,
                    ..Controls::default()
                },
            );
            render::draw_into(&mut frame, &game, width, height, Stats::default());
            renderer.encode_into(std::hint::black_box(&frame), &mut encoded);
            bytes += encoded.len();
        }
        println!(
            "{width}x{height}: {:.3} ms/frame, {} bytes/frame",
            start.elapsed().as_secs_f64() * 1000.0 / 600.0,
            bytes / 600
        );
    }
}

fn main() {
    let result = match Options::parse() {
        Ok(Some(options)) => run(options),
        Ok(None) => Ok(()),
        Err(error) => Err(error.into()),
    };
    if let Err(error) = result {
        eprintln!("terminalshooter: {error}");
        std::process::exit(1);
    }
}
