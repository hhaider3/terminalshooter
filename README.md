# ASCII FPS HD — truecolor 3D shooter in your terminal

A Wolfenstein-style raycaster with truecolor + half-block pixels (2x vertical
resolution), textured walls, perspective floor, gradient sky + sun, pixel-art
demons, muzzle-flash lighting, particles, screen shake and head-bob.
One file, zero dependencies (Python 3 stdlib only).

```bash
cd ~/ascii-fps && python3 fps_hd.py              # auto color pick (safe default)
cd ~/ascii-fps && python3 fps_hd.py --truecolor  # force 24-bit color
cd ~/ascii-fps && python3 fps_hd.py --256        # force 256-color fallback
```

Requires a terminal at least **40x16**. Best at 100x30+. Truecolor is
auto-detected (iTerm2, kitty, WezTerm, VS Code, Windows Terminal…); Apple
Terminal.app automatically gets the 256-color mode since it has no 24-bit
support. Override with the flags above if colors look wrong.

Same game loop throughout: wave-based demon combat, medkit/ammo pickups,
sprint FOV kick, minimap + big map, pause, game-over/restart.

Mouse-look works in most terminals. If yours doesn't report mouse motion,
keyboard turning (`Q`/`E`/arrows) still works. Mouse tracking is turned off
again when you quit. Both modern (SGR) and legacy (X10) mouse protocols are
understood, so mouse bytes can never leak in as phantom keypresses.

## Controls

| Key | Action |
|---|---|
| Mouse move | Look / aim (left-right turns, up-down looks up/down) |
| Left click / hold | Shoot (hold = auto-trigger) |
| Right click | Reload |
| Mouse wheel / `[` / `]` | Sensitivity up / down |
| `W` / `Up` | Move forward (`Shift+W` = sprint + FOV kick) |
| `S` / `Down` | Move back |
| `A` / `D` | Strafe left / right |
| `Left` / `Q` | Turn left |
| `Right` / `E` | Turn right |
| `SPACE` | Shoot (lights up the room) |
| `R` | Reload |
| `M` / `Shift+M` | Toggle minimap / big map |
| `B` | Toggle beep sounds |
| `P` | Pause |
| `H` | Help hint |
| `ESC` / `Ctrl-C` | Quit |

Kill all enemies to advance waves (more + tougher each wave). Grab the glowing
pickups: green = medkit, amber = ammo. `R` restarts after game over.

Minimap (top-left, `M` to hide): green dot+tick = you + facing, red pulse =
demons, green/amber dots = pickups, faint wedge = view cone.

## Test

```bash
python3 fps_hd.py --test
```
