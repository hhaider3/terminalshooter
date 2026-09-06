# TERMINAL // BREACH

A short arena FPS for your terminal. Clear five waves with a spread shotgun,
keep moving around cover, and dodge when enemies wind up an attack.
Python standard library only; macOS or Linux (WSL on Windows).

```bash
python3 fps_hd.py
```

For a first run: **WASD** moves, **Q/E** turns, **F** toggles continuous fire,
and **X** dodges. You can play entirely on the keyboard. Aim toward a group,
close the distance for more damage, and keep an escape route around a pillar.

## The run

- Five waves, then a victory screen. Enemy health stays fixed; later waves
  introduce more enemies and different combinations.
- The shotgun hits multiple enemies in its spread. A centered close shot
  kills a red grunt. Purple brutes need several shots.
- Red grunts pursue steadily. Gold runners close in quickly. Purple brutes
  are slow, tough, and hit hard. Cyan enemies are still spawning and harmless.
- Enemies turn yellow while winding up an attack. Move out of reach, dodge,
  or interrupt them with a shot. Damage only lands after the windup.
- Dodge follows your current movement direction, or forward if stationary.
  It has a 1.8-second cooldown and cannot pass through walls.
- Kills within three seconds build a score multiplier up to 5x. Getting hit
  breaks the streak. A white crosshair confirms a hit.
- A four-second break between waves restores 25 HP and refills your shells.
  Continuous fire switches off between waves. Green pickups heal; amber
  pickups supply ammo. Two medkits start in the side lanes.
- The HUD shows remaining enemies and points toward the last two when no
  other message is displayed. The green minimap dot is your cell; the white
  tick is your facing direction.

## Controls

| Key | Action |
|---|---|
| W / Up, S / Down | Forward, backward |
| A / D | Strafe |
| Q / E, Left / Right | Turn |
| Shift+W | Sprint |
| Mouse movement | Horizontal aim |
| Space / left click | Shotgun; hold left mouse to keep firing |
| F | Toggle continuous fire |
| R / right click | Reload |
| X | Dodge |
| L | Toggle mouse look |
| `[` / `]`, mouse wheel | Adjust sensitivity |
| M / Shift+M | Minimap / tactical map |
| P | Pause |
| H | Control reminder |
| B | Toggle terminal beeps |
| R after a run | Restart |
| Escape / Ctrl-C | Quit; Escape closes the tactical map first |

The tactical map pauses combat. Pause, death, victory, and wave completion
switch off continuous fire. Mouse aiming stays level so vertical drift does
not pull your aim off enemies.

## Terminal performance

The game targets 60 FPS with a default viewport of up to **120×36** terminal
cells. Larger terminal windows leave spare space instead of increasing the
rendering workload. Minimum size: **40×16**. Below that size, gameplay stops
until the window is large enough again.

```bash
python3 fps_hd.py --256        # smaller output, useful for slower terminals
python3 fps_hd.py --truecolor  # richer color
python3 fps_hd.py --large      # allow up to 220×70; higher rendering cost
```

Color mode is detected automatically. Actual frame rate depends on your
terminal, window size, and machine. Standard terminals lack key-release
reports, so movement uses a short timeout and depends on OS key repeat.
The F toggle avoids relying on repeat for firing. Mouse motion stops at the
window edges; Q/E can always turn you around. Terminal modes are restored
when you exit.

## Checks

```bash
python3 fps_hd.py --test
python3 -m unittest -v
```

Tests cover rendering and input, minimap coordinates, enemy navigation,
shotgun spread, attack interruption and dodging, cover collision, cooldowns,
wave resupply, and victory.
