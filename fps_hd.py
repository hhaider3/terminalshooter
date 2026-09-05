#!/usr/bin/env python3
"""
ASCII FPS HD - truecolor 3D shooter in the terminal. No ASCII-only restriction:
uses 24-bit color + half-block pixels (2x vertical resolution), textured walls,
perspective floor, gradient sky + sun, pixel-art demons, muzzle-lighting,
particles, screen shake, head-bob - all in your terminal, zero dependencies.

Run:
    cd ~/ascii-fps && python3 fps_hd.py [--256 | --truecolor]

Controls:
    MOUSE move .... look / aim (left-right turn, up-down pitch)
    MOUSE LEFT .... hold to fire (auto-trigger), click for single shots
    MOUSE RIGHT ... reload        MOUSE WHEEL ... sensitivity
    [ / ] ......... sensitivity down / up
    W / Up ........ forward (SHIFT+W = sprint)   S / Down ... back
    A / D ......... strafe                        Q / E or arrows ... turn
    SPACE ......... shoot   R ... reload   M / Shift+M ... map
    B ... beep sounds   P ... pause   H ... help   ESC / Ctrl-C ... quit
"""

import math
import os
import random
import re
import select
import shutil
import sys
import termios
import time
import tty

# -------------------------------------------------- game logic (standalone) ---
MAP = [
    "########################",
    "#......#.......#.......#",
    "#..##..#..###..#..###..#",
    "#..##......#......#....#",
    "#....####..#..#####..###",
    "#.#.....#.....#......#.#",
    "#.#.###.#####.#.####.#.#",
    "#...#.....#...#....#...#",
    "###.#.###.#.#####.####.#",
    "#...#.#...#.....#......#",
    "#.###.#.#######.#####..#",
    "#.....#.......#.....#..#",
    "#.#########.#.#####.#..#",
    "#.#.......#.#.....#.#..#",
    "#.#.#####.#.#####.#.#..#",
    "#...#...#...#.....#....#",
    "#.###.#.#####.##########",
    "#.....#......#.........#",
    "#.##########.#.#######.#",
    "#............#.........#",
    "########################",
]
MAP_W = len(MAP[0])
MAP_H = len(MAP)

FOV = math.radians(66)
MAX_DEPTH = 16.0
MOVE_SPEED = 3.4
SPRINT_MULT = 1.65
TURN_SPEED = 2.7
ENEMY_SPEED = 1.4
ATTACK_RANGE = 0.95
ATTACK_DMG = (7, 16)
SHOOT_RANGE = 12.0
SHOOT_DMG = 34
MAG_SIZE = 8

# Mouse-look tuning (radians of turn per terminal cell moved)
MOUSE_SENS_DEFAULT = 0.035
MOUSE_SENS_MIN = 0.006
MOUSE_SENS_MAX = 0.12
PITCH_SENS = 1.6       # horizon pixels per terminal row moved
PITCH_MAX = 14.0       # max look up/down offset in rows


def mouse_look(g, dx_cells, dy_cells):
    """Pure helper: apply mouse deltas to camera."""
    sens = g.get('mouse_sens', MOUSE_SENS_DEFAULT) if isinstance(g, dict) else g.mouse_sens
    ang = g.get('angle', 0.0) if isinstance(g, dict) else g.player['angle']
    pitch = g.get('pitch', 0.0) if isinstance(g, dict) else g.pitch
    ang = norm_angle(ang + dx_cells * sens)
    pitch = max(-PITCH_MAX, min(PITCH_MAX, pitch - dy_cells * PITCH_SENS))
    if isinstance(g, dict):
        g['angle'] = ang
        g['pitch'] = pitch
    else:
        g.player['angle'] = ang
        g.pitch = pitch
    return ang, pitch


def is_wall(x, y):
    ix, iy = int(x), int(y)
    if ix < 0 or iy < 0 or ix >= MAP_W or iy >= MAP_H:
        return True
    return MAP[iy][ix] == '#'


def cast_ray(px, py, angle):
    """DDA raycast. Returns (dist, side) where side 0=x-side, 1=y-side."""
    d, s, _, _, _ = cast_ray_ex(px, py, angle)
    return d, s


def cast_ray_ex(px, py, angle):
    """DDA raycast. Returns (dist, side, wall_x, map_x, map_y).
    wall_x = fractional hit position on wall (0..1) for texturing."""
    dx, dy = math.cos(angle), math.sin(angle)
    map_x, map_y = int(px), int(py)
    delta_x = abs(1.0 / dx) if dx != 0 else 1e30
    delta_y = abs(1.0 / dy) if dy != 0 else 1e30
    if dx < 0:
        step_x, side_x = -1, (px - map_x) * delta_x
    else:
        step_x, side_x = 1, (map_x + 1.0 - px) * delta_x
    if dy < 0:
        step_y, side_y = -1, (py - map_y) * delta_y
    else:
        step_y, side_y = 1, (map_y + 1.0 - py) * delta_y
    side = 0
    for _ in range(128):
        if side_x < side_y:
            side_x += delta_x
            map_x += step_x
            side = 0
        else:
            side_y += delta_y
            map_y += step_y
            side = 1
        if map_x < 0 or map_y < 0 or map_x >= MAP_W or map_y >= MAP_H:
            return MAX_DEPTH, side, 0.0, map_x, map_y
        if MAP[map_y][map_x] == '#':
            if side == 0:
                dist = side_x - delta_x
                wall_x = py + dist * dy
            else:
                dist = side_y - delta_y
                wall_x = px + dist * dx
            wall_x -= math.floor(wall_x)
            return min(dist, MAX_DEPTH), side, wall_x, map_x, map_y
    return MAX_DEPTH, side, 0.0, map_x, map_y


def norm_angle(a):
    while a > math.pi:
        a -= 2 * math.pi
    while a < -math.pi:
        a += 2 * math.pi
    return a


def los_clear(x0, y0, x1, y1):
    ang = math.atan2(y1 - y0, x1 - x0)
    dist = math.hypot(x1 - x0, y1 - y0)
    wall_d, _ = cast_ray(x0, y0, ang)
    return wall_d > dist - 0.25


def random_empty(min_player_dist=5.0, player=None):
    for _ in range(400):
        x = random.uniform(1.5, MAP_W - 1.5)
        y = random.uniform(1.5, MAP_H - 1.5)
        if is_wall(x, y):
            continue
        if player and math.hypot(x - player['x'], y - player['y']) < min_player_dist:
            continue
        return x, y
    return 2.5, 2.5


class Game:
    def __init__(self):
        self.reset()

    def reset(self):
        # spawn at the west end of the top corridor, facing east down it
        # (open view; the old spawn faced a wall 0.5 cells away)
        self.player = {'x': 1.5, 'y': 1.5, 'angle': 0.0, 'hp': 100,
                       'mag': MAG_SIZE, 'reserve': 48, 'reloading': 0.0}
        self.enemies = []
        self.pickups = []
        self.particles = []  # screen-space: sx,sy,vx,vy,life,maxlife,char,color
        self.score = 0
        self.kills = 0
        self.wave = 0
        self.msg = "Mouse to look, CLICK to shoot | WASD move | ESC quit"
        self.msg_t = 0.0
        self.hurt_flash = 0.0
        self.muzzle = 0.0
        self.show_map = True
        self.big_map = False
        self.paused = False
        self.game_over = False
        self.beep = False
        self.pitch = 0.0
        self.mouse_sens = MOUSE_SENS_DEFAULT
        self.bob_phase = 0.0
        self.bob_y = 0
        self.bob_x = 0
        self.shake = 0.0
        self.fov_kick = 0.0
        self.sprinting = False
        self.moving = False
        self.held = {}  # action -> expiry timestamp
        self.fps_ema = 30.0
        self.t = 0.0
        for _ in range(3):
            x, y = random_empty(4.0, self.player)
            self.pickups.append({'x': x, 'y': y, 'kind': 'hp' if _ < 2 else 'ammo',
                                 'bob': random.uniform(0, 6)})
        self.next_wave()

    def next_wave(self):
        self.wave += 1
        n = 3 + self.wave * 2
        for _ in range(n):
            x, y = random_empty(6.0, self.player)
            hp = 50 + self.wave * 12
            self.enemies.append({
                'x': x, 'y': y, 'hp': hp, 'maxhp': hp,
                'flash': 0.0, 'atk_cd': 0.0,
                'wx': x, 'wy': y, 'wait': 0.0,
                'phase': random.uniform(0, 6),
                'dead_t': 0.0,
                'speed': ENEMY_SPEED * random.uniform(0.85, 1.2 + self.wave * 0.05),
            })
        p = self.player
        p['hp'] = min(100, p['hp'] + 20)
        p['reserve'] += 16
        self.say(f"WAVE {self.wave}: {n} enemies! (+20 HP, +16 ammo)")

    def say(self, s):
        self.msg = s
        self.msg_t = 4.0

    def try_move(self, nx, ny, radius=0.22):
        if not is_wall(nx + (radius if nx > self.player['x'] else -radius), self.player['y']):
            if not is_wall(nx, self.player['y'] + radius) and not is_wall(nx, self.player['y'] - radius):
                self.player['x'] = nx
        if not is_wall(self.player['x'], ny + (radius if ny > self.player['y'] else -radius)):
            if not is_wall(self.player['x'] + radius, ny) and not is_wall(self.player['x'] - radius, ny):
                self.player['y'] = ny

    def boom(self, amount):
        self.shake = min(1.0, self.shake + amount)

    def spawn_hit_particles(self, big=False):
        n = 16 if big else 9
        for _ in range(n):
            self.particles.append({
                'sx': random.uniform(-6, 6), 'sy': random.uniform(-3, 3),
                'vx': random.uniform(-28, 28), 'vy': random.uniform(-20, 8),
                'life': random.uniform(0.2, 0.45), 'maxlife': 0.45,
                'char': random.choice(['*', 'x', '.', '\'']),
                'color': 'blood',
            })

    def spawn_spark(self):
        for _ in range(5):
            self.particles.append({
                'sx': random.uniform(-4, 4), 'sy': random.uniform(-2, 2),
                'vx': random.uniform(-20, 20), 'vy': random.uniform(-14, 6),
                'life': random.uniform(0.15, 0.3), 'maxlife': 0.3,
                'char': random.choice(['.', '\'', ':']),
                'color': 'spark',
            })

    def shoot(self):
        p = self.player
        if self.game_over or self.paused:
            return False
        if p['reloading'] > 0:
            return False
        if p['mag'] <= 0:
            self.say("RELOADING needed! Press R")
            self.start_reload()
            self.spawn_spark()
            return False
        p['mag'] -= 1
        self.muzzle = 0.13
        self.boom(0.16)
        best = None
        best_dist = SHOOT_RANGE
        for e in self.enemies:
            if e['hp'] <= 0:
                continue
            dx, dy = e['x'] - p['x'], e['y'] - p['y']
            dist = math.hypot(dx, dy)
            if dist > SHOOT_RANGE or dist < 0.15:
                continue
            ang = norm_angle(math.atan2(dy, dx) - p['angle'])
            tol = 0.09 + min(0.12, 0.55 / max(dist, 0.6))
            if abs(ang) > tol:
                continue
            if not los_clear(p['x'], p['y'], e['x'], e['y']):
                continue
            if dist < best_dist:
                best_dist = dist
                best = e
        if best is not None:
            dmg = int(SHOOT_DMG * (1.0 if best_dist < 4 else max(0.45, 1.0 - (best_dist - 4) * 0.07)))
            dmg += random.randint(-5, 5)
            best['hp'] -= dmg
            best['flash'] = 0.15
            if best['hp'] <= 0:
                best['dead_t'] = 0.55
                self.kills += 1
                gained = 100 + self.wave * 15
                self.score += gained
                self.say(f"Kill! +{gained}  (kills: {self.kills})")
                self.spawn_hit_particles(big=True)
                self.boom(0.25)
                if random.random() < 0.35:
                    kind = 'hp' if random.random() < 0.5 else 'ammo'
                    self.pickups.append({'x': best['x'], 'y': best['y'], 'kind': kind,
                                         'bob': 0.0})
            else:
                self.score += 10
                self.spawn_hit_particles(big=False)
        else:
            self.spawn_spark()
        if all(e['hp'] <= 0 for e in self.enemies):
            self.next_wave()
        return True

    def start_reload(self):
        p = self.player
        if p['reloading'] > 0 or p['mag'] == MAG_SIZE or p['reserve'] <= 0:
            return
        p['reloading'] = 1.1
        self.say("Reloading...")

    def update(self, dt):
        self.t += dt
        p = self.player
        if self.msg_t > 0:
            self.msg_t -= dt
        if self.hurt_flash > 0:
            self.hurt_flash -= dt
        if self.muzzle > 0:
            self.muzzle -= dt
        # decay shake, ease fov kick
        self.shake *= math.exp(-5.5 * dt)
        if self.shake < 0.01:
            self.shake = 0.0
        target_kick = 0.10 if (self.sprinting and self.moving) else 0.0
        self.fov_kick += (target_kick - self.fov_kick) * min(1.0, dt * 6)
        # head bob
        if self.moving and not self.game_over and not self.paused:
            self.bob_phase += dt * (11 if self.sprinting else 8)
        else:
            self.bob_phase += dt * 2  # idle breathing, tiny
        amp = 1.6 if self.moving else 0.4
        self.bob_y = int(math.sin(self.bob_phase) * amp)
        self.bob_x = int(math.cos(self.bob_phase * 0.5) * (1 if self.moving else 0))
        # particles
        for pt in list(self.particles):
            pt['life'] -= dt
            if pt['life'] <= 0:
                self.particles.remove(pt)
                continue
            pt['sx'] += pt['vx'] * dt * 0.12
            pt['sy'] += pt['vy'] * dt * 0.12
            pt['vy'] += 60 * dt * 0.12
        if len(self.particles) > 120:
            self.particles = self.particles[-120:]
        if self.game_over or self.paused:
            return
        if p['reloading'] > 0:
            p['reloading'] -= dt
            if p['reloading'] <= 0:
                need = MAG_SIZE - p['mag']
                take = min(need, p['reserve'])
                p['mag'] += take
                p['reserve'] -= take
                p['reloading'] = 0
                self.say("Reloaded!")
        for e in self.enemies:
            if e['hp'] <= 0:
                if e['dead_t'] > 0:
                    e['dead_t'] -= dt
                continue
            if e['flash'] > 0:
                e['flash'] -= dt
            if e['atk_cd'] > 0:
                e['atk_cd'] -= dt
            dx, dy = p['x'] - e['x'], p['y'] - e['y']
            dist = math.hypot(dx, dy)
            sees = dist < 11 and los_clear(e['x'], e['y'], p['x'], p['y'])
            if dist < ATTACK_RANGE:
                e['phase'] += dt * 10
                if e['atk_cd'] <= 0:
                    dmg = random.randint(*ATTACK_DMG) + self.wave
                    p['hp'] -= dmg
                    e['atk_cd'] = 0.8
                    self.hurt_flash = 0.4
                    self.boom(0.6)
                    self.say(f"Ouch! -{dmg} HP")
                    if p['hp'] <= 0:
                        p['hp'] = 0
                        self.game_over = True
            elif sees:
                e['phase'] += dt * 7
                nx = e['x'] + (dx / max(dist, 1e-5)) * e['speed'] * dt
                ny = e['y'] + (dy / max(dist, 1e-5)) * e['speed'] * dt
                for o in self.enemies:
                    if o is e or o['hp'] <= 0:
                        continue
                    ox, oy = e['x'] - o['x'], e['y'] - o['y']
                    od = math.hypot(ox, oy)
                    if 0 < od < 0.6:
                        nx += (ox / od) * dt * 1.2
                        ny += (oy / od) * dt * 1.2
                if not is_wall(nx, e['y']):
                    e['x'] = nx
                if not is_wall(e['x'], ny):
                    e['y'] = ny
            else:
                e['wait'] -= dt
                if e['wait'] <= 0 or math.hypot(e['wx'] - e['x'], e['wy'] - e['y']) < 0.3:
                    tx, ty = random_empty(0, None)
                    e['wx'], e['wy'] = tx, ty
                    e['wait'] = random.uniform(2, 5)
                dx, dy = e['wx'] - e['x'], e['wy'] - e['y']
                d = math.hypot(dx, dy)
                if d > 0.2:
                    e['phase'] += dt * 3
                    nx = e['x'] + dx / d * e['speed'] * 0.4 * dt
                    ny = e['y'] + dy / d * e['speed'] * 0.4 * dt
                    if not is_wall(nx, e['y']):
                        e['x'] = nx
                    if not is_wall(e['x'], ny):
                        e['y'] = ny
        # cull old corpses (keep list small)
        alive = [e for e in self.enemies if e['hp'] > 0]
        corpses = [e for e in self.enemies if e['hp'] <= 0 and e['dead_t'] > 0]
        old = [e for e in self.enemies if e['hp'] <= 0 and e['dead_t'] <= 0]
        if len(old) > 6:
            self.enemies = alive + corpses + old[:6]
        for pk in list(self.pickups):
            pk['bob'] += dt * 3
            if math.hypot(pk['x'] - p['x'], pk['y'] - p['y']) < 0.65:
                if pk['kind'] == 'hp':
                    if p['hp'] >= 100:
                        continue
                    p['hp'] = min(100, p['hp'] + 30)
                    self.say("Picked up MEDKIT +30 HP")
                else:
                    p['reserve'] += 16
                    self.say("Picked up AMMO +16")
                self.score += 25
                self.pickups.remove(pk)

def _detect_truecolor():
    if '--truecolor' in sys.argv:
        return True
    if '--256' in sys.argv:
        return False
    if os.environ.get('ASCIIFPS_TRUECOLOR') == '1':
        return True
    if os.environ.get('ASCIIFPS_256') == '1':
        return False
    ct = os.environ.get('COLORTERM', '').lower()
    if 'truecolor' in ct or '24bit' in ct:
        return True
    # Apple Terminal.app has no 24-bit support -> stay on 256 colors
    if os.environ.get('TERM_PROGRAM') == 'Apple_Terminal' and 'truecolor' not in ct:
        return False
    if os.environ.get('TERM_PROGRAM') in (
            'iTerm.app', 'WezTerm', 'kitty', 'vscode', 'Hyper',
            'Alacritty', 'ghostty', 'rio', 'foot'):
        return True
    if os.environ.get('WT_SESSION'):  # Windows Terminal
        return True
    return False


TRUECOLOR = _detect_truecolor()

# ------------------------------------------------------------ palette ---
BG = (8, 8, 14)            # deep space blue-black (fog target)
WALL_BASE = (122, 142, 150)  # industrial teal-grey
WALL_DARK = (64, 76, 84)
MORTAR = (38, 44, 50)
FLOOR_A = (74, 62, 50)
FLOOR_B = (52, 46, 38)
SKY_TOP = (4, 6, 20)
SKY_MID = (16, 24, 54)
HORIZON_GLOW = (255, 122, 40)
SUN_CORE = (255, 240, 200)
SUN_GLOW = (255, 160, 60)
DEMON = (196, 36, 30)
DEMON_DK = (110, 16, 18)
DEMON_EYE = (255, 220, 60)
HP_GREEN = (80, 255, 140)
AMMO_YEL = (255, 200, 70)
BLOOD = (235, 30, 30)
SPARK = (255, 220, 120)
CROSS = (0, 255, 150)
GUN_STEEL = (150, 160, 175)
GUN_STEEL_DK = (70, 78, 92)
GUN_WOOD = (150, 96, 44)
GUN_WOOD_DK = (92, 56, 24)
FLASH_CORE = (255, 250, 225)
FLASH_MID = (255, 190, 80)


def clamp(v, lo=0.0, hi=255.0):
    return lo if v < lo else hi if v > hi else v


def lerp(a, b, t):
    return a + (b - a) * t


def mix(c1, c2, t):
    """Blend two rgb tuples."""
    return (lerp(c1[0], c2[0], t), lerp(c1[1], c2[1], t), lerp(c1[2], c2[2], t))


def fog(col, dist, extra=0.0):
    f = min(1.0, dist / MAX_DEPTH + extra)
    f = f * f * (3 - 2 * f)  # smoothstep
    return mix(col, BG, f)


def to_int(col):
    return (int(clamp(col[0])), int(clamp(col[1])), int(clamp(col[2])))


_CUBE = [0, 95, 135, 175, 215, 255]


def to_256(col):
    """Quantize rgb to nearest xterm-256 color index."""
    r, g, b = int(clamp(col[0])), int(clamp(col[1])), int(clamp(col[2]))
    # gray ramp?
    if r == g == b or (abs(r - g) < 12 and abs(g - b) < 12):
        if r < 8:
            return 16
        if r > 248:
            return 231
        return 232 + int(round((r - 8) / 247 * 23))
    ri = min(5, int(round(r / 255 * 5)))
    gi = min(5, int(round(g / 255 * 5)))
    bi = min(5, int(round(b / 255 * 5)))
    return 16 + 36 * ri + 6 * gi + bi


# ------------------------------------------------------- terminal i/o ---
class Term:
    def __init__(self):
        self.fd = sys.stdin.fileno()
        self.orig = None
        self.buf = b''
        self.truecolor = TRUECOLOR

    def __enter__(self):
        if not sys.stdin.isatty():
            raise RuntimeError("needs a real terminal (stdin is not a tty)")
        self.orig = termios.tcgetattr(self.fd)
        tty.setraw(self.fd, termios.TCSADRAIN)
        out = sys.stdout
        out.write("\x1b[?1049h")      # alt screen
        out.write("\x1b[?25l")        # hide cursor
        out.write("\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h")  # mouse
        out.flush()
        return self

    def __exit__(self, *exc):
        try:
            out = sys.stdout
            out.write("\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?1006l")
            out.write("\x1b[0m\x1b[?25h\x1b[?1049l")
            out.flush()
        except Exception:
            pass
        try:
            if self.orig is not None:
                termios.tcsetattr(self.fd, termios.TCSADRAIN, self.orig)
        except Exception:
            pass
        return False

    def fg(self, col):
        c = to_int(col)
        if self.truecolor:
            return f"\x1b[38;2;{c[0]};{c[1]};{c[2]}m"
        return f"\x1b[38;5;{to_256(col)}m"

    def bg(self, col):
        c = to_int(col)
        if self.truecolor:
            return f"\x1b[48;2;{c[0]};{c[1]};{c[2]}m"
        return f"\x1b[48;5;{to_256(col)}m"

    # -- input: returns (keys:set[str], mouse:list) --
    # keys: 'w','W','a','s',' ','[',']','up','down','left','right','esc','r','m','M',...
    # mouse: ('motion',x,y) | ('drag',x,y) | ('lpress',x,y) | ('rpress',x,y)
    #        | ('sens',+1|-1) | ('release',)
    @staticmethod
    def _decode_mouse(cb, cx, cy, press):
        # Button-up ends the trigger hold no matter how the terminal encodes
        # it: lowercase 'm' with any code (some send the button's own code,
        # e.g. `<0;..m`), or press-style cb==3. Missing any of these variants
        # stuck left_held=True -> gun firing forever after one click.
        if not press:
            return [('release',)]
        if cb == 3:
            return [('release',)]
        if cb == 35:
            return [('motion', cx, cy)]
        if cb == 32:
            return [('drag', cx, cy)]
        if cb == 0:
            return [('lpress', cx, cy)]
        if cb == 2:
            return [('rpress', cx, cy)]
        if cb == 64:
            return [('sens', +1)]
        if cb == 65:
            return [('sens', -1)]
        return []

    def poll(self, timeout=0.0):
        keys = set()
        mouse = []
        r, _, _ = select.select([self.fd], [], [], timeout)
        if r:
            try:
                data = os.read(self.fd, 4096)
            except OSError:
                data = b''
            if data:
                self.buf += data
        # parse what we have; keep incomplete tail buffered
        while self.buf:
            # SGR mouse: ESC [ < Cb ; Cx ; Cy M/m
            m = re.match(rb'^\x1b\[<(\d+);(\d+);(\d+)([Mm])', self.buf)
            if m:
                cb, cx, cy = int(m.group(1)), int(m.group(2)), int(m.group(3))
                self.buf = self.buf[m.end():]
                mouse.extend(self._decode_mouse(cb, cx, cy, m.group(4) == b'M'))
                continue
            # legacy X10 mouse: ESC [ M Cb Cx Cy (each byte +32).
            # MUST come before generic ESC handling, otherwise the packet is
            # misread as 'M' + garbage keys (space = phantom shooting!).
            if self.buf.startswith(b'\x1b[M'):
                if len(self.buf) < 6:
                    break  # incomplete packet, wait for the rest
                cb, cx, cy = self.buf[3] - 32, self.buf[4] - 32, self.buf[5] - 32
                self.buf = self.buf[6:]
                # X10 has no press/release marker: cb==3 means release
                mouse.extend(self._decode_mouse(cb, cx, cy, cb != 3))
                continue
            if self.buf.startswith(b'\x1b'):
                # partial CSI / SS3 sequence split across reads -> wait
                if (self.buf == b'\x1bO'
                        or re.match(rb'^\x1b\[[0-9;]*$', self.buf)):
                    break
                for seq, name in ((b'\x1b[A', 'up'), (b'\x1b[B', 'down'),
                                  (b'\x1b[C', 'right'), (b'\x1b[D', 'left'),
                                  (b'\x1bOA', 'up'), (b'\x1bOB', 'down'),
                                  (b'\x1bOC', 'right'), (b'\x1bOD', 'left')):
                    if self.buf.startswith(seq):
                        keys.add(name)
                        self.buf = self.buf[len(seq):]
                        break
                else:
                    if len(self.buf) == 1:
                        # lone ESC (or incomplete seq): peek briefly for more
                        r2, _, _ = select.select([self.fd], [], [], 0.02)
                        if r2:
                            try:
                                more = os.read(self.fd, 64)
                            except OSError:
                                more = b''
                            if more:
                                self.buf += more
                                continue
                        keys.add('esc')
                        self.buf = b''
                    elif self.buf[1:2] == b'[':
                        # unknown CSI (we never query the terminal, so this
                        # should not happen): swallow the whole sequence
                        mcsi = re.match(rb'^\x1b\[[0-9;?]*[A-Za-z]', self.buf)
                        if mcsi:
                            self.buf = self.buf[mcsi.end():]
                        else:
                            break  # partial CSI, wait for the rest
                        continue
                    else:
                        # ESC + char (alt-key): treat char as key
                        keys.add(chr(self.buf[1]) if self.buf[1] < 128 else '?')
                        self.buf = self.buf[2:]
                continue
            b = self.buf[0:1]
            self.buf = self.buf[1:]
            if b == b'\x03':
                keys.add('ctrlc')
            elif b == b'\x7f':
                keys.add('backspace')
            elif b == b'\r' or b == b'\n':
                keys.add('enter')
            elif b == b' ':
                keys.add(' ')
            elif 32 <= b[0] < 127:
                keys.add(chr(b[0]))
            # else: ignore control bytes
            # trailing partial escape sequence -> wait for the rest next poll
            if (self.buf == b'\x1bO'
                    or re.match(rb'^\x1b\[[0-9;<]*$', self.buf)
                    or (self.buf.startswith(b'\x1b[M') and len(self.buf) < 6)):
                break
        return keys, mouse


# ------------------------------------------------------------- sprites ---
def enemy_px(u, v, flash, phase, attacking, dead):
    """Pixel shader for a demon. Returns 'body'|'edge'|'eye'|None-ish via color flag.
    Returns None for transparent, else (kind, shade) with kind in 0..1 brightness mult."""
    if dead:
        if v < 0.55:
            return None
        return (0.45, 0)  # dark smear
    if flash:
        if 0.15 < u < 0.85 and 0.05 < v < 0.95:
            return (2.2, 1)  # overbright white
        return None
    step = math.sin(phase)
    if v < 0.24:  # head
        if 0.34 < u < 0.66:
            # eyes
            if attacking or True:
                if 0.14 < v < 0.20 and ((0.40 < u < 0.47) or (0.53 < u < 0.60)):
                    return (1.0, 2)  # eye flag
            edge = min(u - 0.34, 0.66 - u) / 0.16
            return (0.65 + 0.5 * edge, 0)
        # horns
        if (0.26 < u < 0.34 or 0.66 < u < 0.74) and v < 0.14:
            return (0.9, 0)
        return None
    if v < 0.60:  # torso + arms
        if 0.28 < u < 0.72:
            edge = min(u - 0.28, 0.72 - u) / 0.22
            return (0.55 + 0.6 * edge, 0)
        if 0.14 < u < 0.28 or 0.72 < u < 0.86:
            if attacking and v < 0.42:
                return (1.0, 0)  # raised claws
            return (0.5, 0)
        return None
    # legs, animated
    wob = step * 0.05
    if 0.28 + wob < u < 0.44 + wob or 0.56 - wob < u < 0.72 - wob:
        return (0.6, 0)
    return None


# ------------------------------------------------------------ renderer ---
HALF = '\u2580'  # upper half block


class Frame:
    def __init__(self, cols, rows):
        self.cols = cols
        self.rows = rows
        self.top = [[BG] * cols for _ in range(rows)]
        self.bot = [[BG] * cols for _ in range(rows)]

    def px(self, x, y, col):
        if 0 <= x < self.cols and 0 <= y < self.rows * 2:
            if y % 2 == 0:
                self.top[y // 2][x] = col
            else:
                self.bot[y // 2][x] = col


def render(g, cols, rows):
    hud = 4
    view_rows = max(8, rows - hud)
    W, H = cols, view_rows * 2
    fr = Frame(cols, view_rows)
    p = g.player
    eff_fov = FOV + g.fov_kick
    shx = int((random.random() - 0.5) * g.shake * 6)
    shy = int((random.random() - 0.5) * g.shake * 8)
    horizon = H // 2 + int(g.bob_y * 2 + getattr(g, 'pitch', 0.0) * 2) + shy
    horizon = max(4, min(H - 5, horizon))
    sun_dir = 0.7
    t = g.t
    light = 1.0 if g.muzzle > 0 else 0.0

    zbuf = [MAX_DEPTH] * W
    cos_t = [0.0] * W
    sin_t = [0.0] * W

    for x in range(W):
        ra = p['angle'] - eff_fov / 2 + eff_fov * x / max(W - 1, 1)
        ca, sa = math.cos(ra), math.sin(ra)
        cos_t[x], sin_t[x] = ca, sa
        dist, side, wall_x, _, _ = cast_ray_ex(p['x'], p['y'], ra)
        dist *= math.cos(norm_angle(ra - p['angle']))
        dist = max(dist, 0.05)
        zbuf[x] = dist
        # --- wall slice ---
        base = WALL_BASE if side == 0 else WALL_DARK
        lf = 1.0 / (1.0 + dist * dist * 0.045)
        if light:
            lf = min(1.3, lf + max(0.0, (7 - dist)) / 7 * 0.8)
        wall_ph = int(H / dist)
        if wall_ph > H * 3:
            wall_ph = H * 3
        y0 = horizon - wall_ph // 2
        y1 = horizon + wall_ph // 2
        # --- sky ---
        sun_diff = abs(norm_angle(ra - sun_dir))
        sun_y = horizon - int(H * 0.22)
        for y in range(0, max(y0, 0)):
            depth = (horizon - y) / max(horizon, 1)
            if depth > 0.66:
                col = SKY_TOP
                if (x * 31 + y * 17 + int(t * 2) * 7) % 53 == 0:
                    col = (200, 210, 255)
            elif depth > 0.2:
                col = mix(SKY_MID, SKY_TOP, (depth - 0.2) / 0.46)
            else:
                col = mix(HORIZON_GLOW, SKY_MID, depth / 0.2)
            # sun disc + halo
            if sun_diff < 0.16:
                d2 = (y - sun_y) ** 2 + (sun_diff * 220) ** 2
                if d2 < 26:
                    col = SUN_CORE
                elif d2 < 130:
                    col = mix(SUN_GLOW, col, d2 / 130)
            fr.px(x, y, col)
        # --- wall pixels ---
        for y in range(max(y0, 0), min(y1, H)):
            tex_v = (y - y0) / max(wall_ph, 1)
            row = tex_v * 7.0
            colf = wall_x * 9.0 + (int(row) % 2) * 0.5
            mortar = (row % 1.0) < 0.05 or (colf % 1.0) < 0.05
            trim = tex_v < 0.03
            if trim:
                col = mix((200, 210, 220), base, 0.4)
            else:
                # subtle vertical panel variation; mortar is just shaded base
                # (keeps texture without hard stripe lines)
                panel = 0.92 + 0.08 * math.sin(wall_x * 40.0)
                if mortar:
                    panel *= 0.62
                col = (base[0] * panel, base[1] * panel, base[2] * panel)
            col = (col[0] * lf, col[1] * lf, col[2] * lf)
            if light:
                warm = max(0.0, (7 - dist)) / 7 * 60
                col = (col[0] + warm, col[1] + warm * 0.6, col[2] + warm * 0.3)
            if g.hurt_flash > 0:
                col = mix(col, (255, 40, 40), 0.45)
            fr.px(x, y, fog(col, dist))
        # --- floor pixels ---
        for y in range(max(y1, 0), H):
            dy_row = y - horizon
            if dy_row <= 0:
                dy_row = 1
            rd = (0.5 * H) / dy_row
            if rd > MAX_DEPTH:
                fr.px(x, y, BG)
                continue
            fx = p['x'] + ca * rd
            fy = p['y'] + sa * rd
            checker = (int(math.floor(fx)) + int(math.floor(fy))) & 1
            base = FLOOR_A if checker else FLOOR_B
            lf2 = 1.0 / (1.0 + rd * rd * 0.05)
            if light:
                lf2 = min(1.3, lf2 + max(0.0, (6 - rd)) / 6 * 0.7)
            col = (base[0] * lf2, base[1] * lf2, base[2] * lf2)
            fr.px(x, y, fog(col, rd))

    # --- sprites, far -> near ---
    sprites = []
    for e in g.enemies:
        if e['hp'] <= 0 and e['dead_t'] <= 0:
            continue
        dx, dy = e['x'] - p['x'], e['y'] - p['y']
        dist = math.hypot(dx, dy)
        ang = norm_angle(math.atan2(dy, dx) - p['angle'])
        if abs(ang) > eff_fov / 2 + 0.4:
            continue
        sprites.append((dist, 0, e, ang))
    for pk in g.pickups:
        dx, dy = pk['x'] - p['x'], pk['y'] - p['y']
        dist = math.hypot(dx, dy)
        ang = norm_angle(math.atan2(dy, dx) - p['angle'])
        if abs(ang) > eff_fov / 2 + 0.25:
            continue
        sprites.append((dist, 1, pk, ang))
    sprites.sort(key=lambda s: -s[0])

    for dist, kind, obj, ang in sprites:
        dist = max(dist, 0.3)
        sx = int((0.5 + ang / eff_fov) * W) + shx
        scale = 1.0 if kind == 0 else 0.5
        size = int(H / dist * scale)
        if size <= 1:
            continue
        size = min(size, H * 2)
        bob_off = int(math.sin(obj.get('bob', 0.0)) * size * 0.06) if kind == 1 else 0
        top = horizon - size // 2 + bob_off
        dead = kind == 0 and obj['hp'] <= 0
        if dead:
            k = 1.0 - max(obj['dead_t'], 0) / 0.55
            top = top + int(size * 0.5 * k)
        half = max(size // 3, 1)
        # contact shadow
        if not dead and size > 6:
            sy = min(top + size, H - 1)
            for sxx in range(sx - half, sx + half + 1):
                if 0 <= sxx < W and dist < zbuf[sxx]:
                    fr.px(sxx, sy, (5, 5, 8))
        for sxx in range(sx - half, sx + half + 1):
            if sxx < 0 or sxx >= W or dist >= zbuf[sxx] - 0.15:
                continue
            u = (sxx - (sx - half)) / max(half * 2, 1)
            for sy in range(max(top, 0), min(top + size, H)):
                v = (sy - top) / max(size, 1)
                if kind == 0:
                    atk = math.hypot(obj['x'] - p['x'], obj['y'] - p['y']) < 2.4
                    r = enemy_px(u, v, obj['flash'] > 0, obj.get('phase', 0.0), atk, dead)
                    if r is None:
                        continue
                    mult, flag = r
                    if flag == 2:
                        col = DEMON_EYE
                    elif mult > 2.0:
                        col = (255, 255, 255)
                    else:
                        col = (DEMON[0] * mult, DEMON[1] * mult, DEMON[2] * mult)
                        if dead:
                            col = (90, 12, 14)
                    if not dead and v < 0.05 and size > 24:
                        frac = max(obj['hp'], 0) / obj['maxhp']
                        col = HP_GREEN if u < frac else (60, 20, 20)
                    fr.px(sxx, sy, fog(col, dist))
                else:
                    d = abs(u - 0.5) + abs(v - 0.5) * 1.1
                    pulse = 0.75 + 0.25 * math.sin(obj.get('bob', 0.0) * 2)
                    if d > 0.5:
                        continue
                    core = (60, 255, 150) if obj['kind'] == 'hp' else (255, 190, 80)
                    if d < 0.18:
                        col = mix((255, 255, 255), core, 0.35)
                    else:
                        col = (core[0] * pulse * (1 - d), core[1] * pulse * (1 - d), core[2] * pulse * (1 - d))
                    fr.px(sxx, sy, fog(col, dist))

    # --- particles (screen space) ---
    # Aim point stays at screen-center vertically: pitch pans the world behind
    # a fixed crosshair (real-FPS feel). It must NOT follow the horizon, or
    # looking up/down pans the crosshair with the world and aiming feels dead.
    cx = W // 2 + shx
    cy = max(0, min(H - 1, H // 2 + int(g.bob_y * 2) + shy))
    for pt in g.particles:
        x = int(cx + pt['sx'] * 1.6)
        y = int(cy + pt['sy'] * 2.2)
        if 0 <= x < W and 0 <= y < H:
            a = max(0.0, pt['life'] / pt['maxlife'])
            base = BLOOD if pt['color'] == 'blood' else SPARK
            fr.px(x, y, (base[0] * a + 30, base[1] * a + 20, base[2] * a + 20))

    # --- tracer ---
    if g.muzzle > 0:
        for i in range(1, 10):
            fr.px(cx, max(0, cy - i), (255, 230 - i * 12, 150 - i * 10))

    # --- crosshair ---
    gap = 2 if g.moving else 0
    for ox, oy in ((0, 0), (-2 - gap, 0), (2 + gap, 0), (0, -3 - gap), (0, 3 + gap)):
        fr.px(cx + ox, cy + oy, CROSS)

    draw_gun(fr, g, W, H)

    # --- vignette + hurt + dark corners (single merged pass) ---
    max_r = math.hypot(W / 2, H / 2)
    hurt = min(1.0, g.hurt_flash * 2.5)
    lowhp = 0.0
    if p['hp'] <= 30 and not g.game_over:
        lowhp = (0.5 + 0.5 * math.sin(t * 5)) * (1 - p['hp'] / 30) * 0.7
    amt = max(hurt, lowhp)
    for yy in range(H):
        yyc = yy // 2
        dy = yy - H / 2
        for xx in range(W):
            r = math.hypot(xx - W / 2, dy) / max_r
            if r < 0.55:
                continue
            k_red = min(1.0, (r - 0.55) / 0.45) * amt if amt > 0.02 else 0.0
            k_dark = (r - 0.82) / 0.18 * 0.35 if r > 0.82 else 0.0
            if k_red <= 0 and k_dark <= 0:
                continue
            for buf in (fr.top, fr.bot):
                c = buf[yyc][xx]
                if k_red > 0:
                    c = mix(c, (200, 20, 20), min(1.0, k_red) * 0.8)
                if k_dark > 0:
                    c = (c[0] * (1 - k_dark), c[1] * (1 - k_dark), c[2] * (1 - k_dark))
                buf[yyc][xx] = c

    # crisp minimap last so vignette/corner darkening never dulls it
    draw_minimap_pixels(fr, g, W, H)

    return fr, zbuf, horizon


def draw_gun(fr, g, W, H):
    muzzle = g.muzzle > 0
    recoil = int(g.muzzle * 60) if g.muzzle > 0 else 0
    gw = min(64, max(34, W // 2))
    gh = gw // 3
    gx = W // 2 - gw // 2 + int(g.bob_x * 1.5)
    gy = H - gh + int(g.bob_y) - recoil
    p = g.player
    # muzzle flash star
    if muzzle:
        fcx, fcy = W // 2 + int(g.bob_x * 1.5), gy - 2
        for r in range(gw // 4 + 6):
            for a in range(12):
                ang = a / 12 * math.pi * 2 + (r * 0.3)
                rr = r * (1.0 if a % 2 == 0 else 0.45)
                fr.px(int(fcx + math.cos(ang) * rr), int(fcy + math.sin(ang) * rr * 0.7),
                      FLASH_CORE if r < 3 else FLASH_MID if r < 7 else (200, 90, 30))
    for yy in range(gh):
        v = yy / max(gh - 1, 1)
        for xx in range(gw):
            u = xx / max(gw - 1, 1)
            x, y = gx + xx, gy + yy
            if not (0 <= x < W and 0 <= y < H):
                continue
            col = None
            if v < 0.34:
                # twin barrels
                if 0.40 < u < 0.48 or 0.52 < u < 0.60:
                    hole = u % 0.08 < 0.02
                    col = GUN_STEEL_DK if hole else GUN_STEEL
                elif 0.38 < u < 0.62:
                    col = GUN_STEEL_DK
            elif v < 0.62:
                # receiver
                if 0.24 < u < 0.76:
                    edge = min(u - 0.24, 0.76 - u)
                    col = GUN_WOOD if edge > 0.06 else GUN_WOOD_DK
            elif v < 0.82:
                # pump (slides while reloading)
                slide = 0.0
                if p['reloading'] > 0:
                    slide = (1.0 - p['reloading'] / 1.1 - 0.5) * 0.2
                if 0.28 + slide < u < 0.72 + slide:
                    col = GUN_WOOD_DK if int(u * 40) % 2 == 0 else GUN_WOOD
            else:
                # grip bottom edge
                if 0.30 < u < 0.70:
                    col = (40, 30, 22)
            if col is not None:
                # simple top-light shading
                shade = 0.75 + 0.25 * (1 - v)
                fr.px(x, y, (col[0] * shade, col[1] * shade, col[2] * shade))


def serialize(term, fr):
    """Blit half-block frame to ANSI string.
    Every row starts with absolute cursor positioning -- never raw newlines.
    A row of exactly `cols` cells + '\\n' double-advances on many terminals
    (auto-wrap + linefeed), producing phantom blank lines across the screen."""
    out = []
    cur_fg = cur_bg = None
    for r in range(fr.rows):
        out.append(f"\x1b[{r + 1};1H")
        for c in range(fr.cols):
            fg, bg = fr.top[r][c], fr.bot[r][c]
            if fg != cur_fg:
                out.append(term.fg(fg))
                cur_fg = fg
            if bg != cur_bg:
                out.append(term.bg(bg))
                cur_bg = bg
            out.append(HALF)
    out.append("\x1b[0m")
    return "".join(out)


# ---------------------------------------------------------------- HUD ---
def hud_lines(term, g, cols):
    p = g.player
    hp = max(0, int(p['hp']))
    compact = cols < 90
    nb = max(6, min(24, cols // 5 if compact else 24))
    fill = int(hp / 100 * nb)
    hcol = HP_GREEN if hp > 60 else AMMO_YEL if hp > 30 else BLOOD
    hpbar = term.fg(hcol) + "█" * fill + "\x1b[0m" + term.fg((60, 60, 70)) + "░" * (nb - fill) + "\x1b[0m"
    pips = "▮" * p['mag'] + term.fg((70, 70, 80)) + "▯" * (MAG_SIZE - p['mag']) + "\x1b[0m"
    ammo = f"{term.fg(AMMO_YEL)}{pips}\x1b[0m {p['mag']}/{p['reserve']}"
    if p['reloading'] > 0:
        prog = 1.0 - p['reloading'] / 1.1
        bw = 10
        ammo = f"RELOAD {term.fg(AMMO_YEL)}{'█' * int(prog * bw)}{'░' * (bw - int(prog * bw))}\x1b[0m"
    if compact:
        l1 = f" HP {hpbar}{hp:3d} {ammo}"
    else:
        l1 = f" HP {hpbar} {hp:3d}  {ammo}  SCORE {g.score}  W{g.wave} K{g.kills} FPS {g.fps_ema:.0f}"
    msg = g.msg if g.msg_t > 0 else "Mouse look/click | WASD move (Shift sprint) | SPACE shoot | R reload | [ ] sens | ESC quit"
    l2 = f" {term.fg((255, 220, 130))}{msg[:max(0, cols - 2)]}\x1b[0m"
    l3 = f" {term.fg((90, 90, 110))}sens {g.mouse_sens:.3f} | M map | B sound | P pause | H help\x1b[0m"
    div = term.fg((40, 60, 80)) + "─" * cols + "\x1b[0m"
    return [div, l1, l2, l3]


MM_BG = (6, 10, 20)
MM_BORDER = (50, 130, 170)
MM_WALL = (120, 160, 200)
MM_FLOOR = (22, 32, 48)
MM_CONE = (30, 110, 80)
MM_PLAYER = (80, 255, 140)


def draw_minimap_pixels(fr, g, W, H):
    """Minimap drawn as chunky pixels inside the frame (top-left panel).
    No text overlay, so nothing can misalign: bordered panel, walls, view
    cone, blinking pickups, pulsing enemies, rotated player arrow."""
    if not g.show_map or g.big_map:
        return
    p = g.player
    px = 1  # compact: whole 24x21 map fits in ~26x23 px
    mw, mh = MAP_W * px, MAP_H * px
    ox, oy = 2, 2
    if ox + mw + 2 > W or oy + mh + 2 > H:
        return  # terminal too small: skip rather than draw broken
    for yy in range(oy, oy + mh + 2):
        for xx in range(ox, ox + mw + 2):
            edge = xx in (ox, ox + mw + 1) or yy in (oy, oy + mh + 1)
            fr.px(xx, yy, MM_BORDER if edge else MM_BG)
    ix, iy = ox + 1, oy + 1
    for my in range(MAP_H):
        for mx in range(MAP_W):
            col = MM_WALL if MAP[my][mx] == '#' else MM_FLOOR
            for dy in range(px):
                for dx in range(px):
                    fr.px(ix + mx * px + dx, iy + my * px + dy, col)
    # view cone
    eff = FOV + g.fov_kick
    off = -eff / 2
    while off <= eff / 2 + 1e-6:
        for dd in (1.5, 2.5, 3.5, 4.5, 5.5):
            fx = p['x'] + math.cos(p['angle'] + off) * dd
            fy = p['y'] + math.sin(p['angle'] + off) * dd
            if not is_wall(fx, fy):
                fr.px(ix + int(fx * px), iy + int(fy * px), MM_CONE)
        off += eff / 8
    # pickups blink under actors
    for pk in g.pickups:
        b = 0.55 + 0.45 * math.sin(g.t * 4 + pk.get('bob', 0.0))
        col = (80 * b + 40, 255 * b, 150 * b) if pk['kind'] == 'hp' else (255 * b, 190 * b, 80 * b)
        fr.px(ix + int(pk['x'] * px), iy + int(pk['y'] * px), col)
    # enemies pulse
    for i, e in enumerate(g.enemies):
        if e['hp'] <= 0:
            continue
        b = 0.6 + 0.4 * math.sin(g.t * 6 + i)
        col = (255 * b, 70 * b, 70 * b)
        ex, ey = ix + int(e['x'] * px), iy + int(e['y'] * px)
        fr.px(ex, ey, col)
    # compact player marker: center dot + facing tick (a full arrow spans
    # several cells at this scale and spills over the neighborhood)
    ang = p['angle']
    dx, dy = math.cos(ang), math.sin(ang)
    cxp, cyp = p['x'] * px, p['y'] * px
    for qx, qy in ((cxp, cyp),
                   (cxp + dx * 1.4, cyp + dy * 1.4)):
        X, Y = ix + int(round(qx)), iy + int(round(qy))
        if ix <= X < ix + mw and iy <= Y < iy + mh:
            fr.px(X, Y, MM_PLAYER)


# ---------------------------------------------------------------- main ---
TITLE = [
    "    _    ____   ____ ___ ___   _____ ____  ____  ",
    "   / \\  / ___| / ___|_ _|_ _| |  ___|  _ \\/ ___| ",
    "  / _ \\ \\___ \\| |    | | | |  | |_  | |_) \\___ \\ ",
    " / ___ \\ ___) | |___ | | | |  |  _| |  __/ ___) |",
    "/_/   \\_\\____/ \\____|___|___|  |_|   |_|   |____/ ",
    "",
    "  H D   E D I T I O N  -  truecolor + half-block pixels",
    "",
    "  MOUSE look/aim | HOLD LEFT to fire | RIGHT reload | WHEEL sens",
    "  W/S/A/D move (Shift+W sprint) | Q/E or arrows turn | SPACE shoot",
    "  Kill everything to advance waves. Grab the glowing pickups.",
    "",
    "  move mouse or press any key to start (ESC quits)",
]


def main():
    with Term() as term:
        g = Game()
        state = 'title'  # title | play
        last = time.time()
        last_mouse = None
        left_held = False
        last_shot = 0.0
        quit_flag = False

        while not quit_flag:
            now = time.time()
            raw_dt = now - last
            dt = min(raw_dt, 0.05)
            if raw_dt > 0:
                inst = 1.0 / max(raw_dt, 1e-4)
                g.fps_ema += (inst - g.fps_ema) * 0.05
            last = now
            try:
                cols, rows = shutil.get_terminal_size()
            except Exception:
                cols, rows = 100, 30
            cols = max(40, min(cols, 220))
            rows = max(16, min(rows, 70))

            keys, mev = term.poll()
            if 'ctrlc' in keys or 'esc' in keys:
                if state == 'title':
                    break
                if g.big_map:
                    g.big_map = False
                    keys.discard('esc')
                else:
                    break
            # --- mouse events ---
            for ev in mev:
                tag = ev[0]
                if tag in ('motion', 'drag'):
                    _, cx, cy = ev
                    if last_mouse is not None and state == 'play' and not g.paused and not g.game_over:
                        dx, dy = cx - last_mouse[0], cy - last_mouse[1]
                        if dx or dy:
                            # mouse cells are 1col x 0.5cell-tall in pixel space
                            mouse_look(g, dx, dy * 0.5)
                    last_mouse = (cx, cy)
                    if state == 'title':
                        state = 'play'
                elif tag == 'lpress':
                    last_mouse = (ev[1], ev[2])
                    if state == 'title':
                        state = 'play'
                    elif not g.paused and not g.game_over:
                        left_held = True
                        if g.shoot():
                            last_shot = now
                elif tag == 'release':
                    left_held = False
                elif tag == 'rpress':
                    if state == 'play' and not g.paused and not g.game_over:
                        g.start_reload()
                elif tag == 'sens':
                    f = 1.25 if ev[1] > 0 else 1 / 1.25
                    g.mouse_sens = max(MOUSE_SENS_MIN, min(MOUSE_SENS_MAX, g.mouse_sens * f))
                    g.say(f"Mouse sens {g.mouse_sens:.3f}")
            # --- keyboard ---
            if keys and state == 'title':
                state = 'play'
            if 'p' in keys or 'P' in keys:
                if state == 'play':
                    g.paused = not g.paused
                    left_held = False  # never resume firing out of pause
            if 'b' in keys or 'B' in keys:
                g.beep = not g.beep
                g.say(f"Sound {'ON' if g.beep else 'OFF'}")
            if 'm' in keys or 'M' in keys:
                if 'M' in keys:
                    g.big_map = not g.big_map
                else:
                    g.show_map = not g.show_map
            if 'h' in keys or 'H' in keys:
                g.say("Mouse look/click | WASD move | Q/E turn | SPACE shoot | R reload | ESC quit")
            if '[' in keys:
                g.mouse_sens = max(MOUSE_SENS_MIN, g.mouse_sens / 1.25)
                g.say(f"Mouse sens {g.mouse_sens:.3f}")
            if ']' in keys:
                g.mouse_sens = min(MOUSE_SENS_MAX, g.mouse_sens * 1.25)
                g.say(f"Mouse sens {g.mouse_sens:.3f}")
            if 'r' in keys or 'R' in keys:
                if g.game_over:
                    g = Game()
                    left_held = False
                else:
                    g.start_reload()
            if ' ' in keys:
                if state == 'play' and not g.paused and not g.game_over:
                    if g.shoot():
                        last_shot = now
                        if g.beep:
                            sys.stdout.write("\a")

            # held-key smoothing for movement
            for k in keys:
                lk = k.lower() if len(k) == 1 else k
                if lk == 'w':
                    g.held['fwd'] = now + 0.14
                    if k == 'W':
                        g.held['sprint'] = now + 0.14
                elif lk == 's':
                    g.held['back'] = now + 0.14
                elif lk == 'a':
                    g.held['sl'] = now + 0.14
                elif lk == 'd':
                    g.held['sr'] = now + 0.14
                elif k in ('left', 'q', 'Q'):
                    g.held['tl'] = now + 0.14
                elif k in ('right', 'e', 'E'):
                    g.held['tr'] = now + 0.14
            for k in list(g.held.keys()):
                if g.held[k] < now:
                    del g.held[k]

            # auto-trigger while mouse held
            if left_held and state == 'play' and not g.paused and not g.game_over:
                if now - last_shot > 0.17:
                    if g.player['mag'] <= 0:
                        g.start_reload()
                    elif g.shoot():
                        last_shot = now
                        if g.beep:
                            sys.stdout.write("\a")

            p = g.player
            g.moving = False
            g.sprinting = False
            if state == 'title':
                p['angle'] += dt * 0.25  # slow cinematic pan behind title
            if state == 'play' and not g.paused and not g.game_over:
                fwd = 'fwd' in g.held
                sprint = 'sprint' in g.held and fwd
                g.sprinting = sprint
                g.moving = bool(fwd or 'back' in g.held or 'sl' in g.held or 'sr' in g.held)
                sp = MOVE_SPEED * (SPRINT_MULT if sprint else 1.0)
                step = max(dt, 1 / 60)
                if 'tl' in g.held:
                    p['angle'] -= TURN_SPEED * step
                if 'tr' in g.held:
                    p['angle'] += TURN_SPEED * step
                mx = my = 0.0
                if fwd:
                    mx += math.cos(p['angle']) * sp * dt
                    my += math.sin(p['angle']) * sp * dt
                if 'back' in g.held:
                    mx -= math.cos(p['angle']) * sp * 0.7 * dt
                    my -= math.sin(p['angle']) * sp * 0.7 * dt
                if 'sl' in g.held:
                    mx += math.cos(p['angle'] - math.pi / 2) * sp * 0.8 * dt
                    my += math.sin(p['angle'] - math.pi / 2) * sp * 0.8 * dt
                if 'sr' in g.held:
                    mx += math.cos(p['angle'] + math.pi / 2) * sp * 0.8 * dt
                    my += math.sin(p['angle'] + math.pi / 2) * sp * 0.8 * dt
                if mx or my:
                    g.try_move(p['x'] + mx, p['y'] + my)
            hp_before = p['hp']
            g.update(dt)
            if g.game_over:
                left_held = False  # never keep firing into/after death
            if g.beep and p['hp'] < hp_before:
                sys.stdout.write("\a")

            # --- draw ---
            fr, _, horizon = render(g, cols, rows)
            frame = serialize(term, fr)
            out = [frame]
            # HUD
            base_row = (rows - 4) + 1
            for i, ln in enumerate(hud_lines(term, g, cols)):
                out.append(f"\x1b[{base_row + i};1H\x1b[0m{ln}\x1b[0m\x1b[K")
            # overlays
            if state == 'title':
                y0 = max(2, rows // 2 - len(TITLE) // 2)
                for i, ln in enumerate(TITLE):
                    x0 = max(1, (cols - len(ln)) // 2)
                    fg = (255, 255, 255) if i < 5 else (255, 220, 130)
                    out.append(f"\x1b[{y0 + i};{x0}H\x1b[0m{term.bg((5, 8, 18))}{term.fg(fg)}{ln}\x1b[0m")
            elif g.big_map:
                y0 = max(2, (rows - MAP_H) // 2 - 1)
                x0 = max(1, (cols - MAP_W) // 2)
                out.append(f"\x1b[{y0};{x0}H\x1b[0mMAP - WAVE {g.wave} (Shift+M closes)\x1b[K")
                for j, rowstr in enumerate(MAP):
                    out.append(f"\x1b[{y0 + 1 + j};{x0}H{term.fg((120, 160, 200))}{rowstr}\x1b[0m")
                out.append(f"\x1b[{y0 + 1 + int(p['y'])};{x0 + int(p['x'])}H{term.fg((80, 255, 140))}@\x1b[0m")
                for e in g.enemies:
                    if e['hp'] > 0:
                        out.append(f"\x1b[{y0 + 1 + int(e['y'])};{x0 + int(e['x'])}H{term.fg((255, 80, 80))}E\x1b[0m")
            elif g.paused:
                s = "-- PAUSED (P to resume) --"
                out.append(f"\x1b[{rows // 2};{(cols - len(s)) // 2}H\x1b[0m{term.fg((255, 220, 130))}{s}\x1b[0m")
            elif g.game_over:
                lines = ["GAME OVER", f"SCORE {g.score}   KILLS {g.kills}   WAVE {g.wave}",
                         "Press R to restart, ESC to quit"]
                y0 = rows // 2 - 1
                for i, ln in enumerate(lines):
                    out.append(f"\x1b[{y0 + i};{(cols - len(ln)) // 2}H\x1b[0m{term.fg((255, 70, 70))}{ln}\x1b[0m")
            sys.stdout.write("".join(out))
            sys.stdout.flush()

            elapsed = time.time() - now
            time.sleep(max(0, 1 / 30 - elapsed))


# ---------------------------------------------------------------- tests ---
def run_test():
    # pure-function tests, no terminal needed
    assert mix((0, 0, 0), (255, 255, 255), 0.5) == (127.5, 127.5, 127.5)
    assert to_int((300, -5, 128.7)) == (255, 0, 128)
    assert to_256((0, 0, 0)) == 16 and to_256((255, 255, 255)) == 231
    assert 16 <= to_256((200, 30, 30)) <= 231
    assert fog((100, 100, 100), 0) == (100, 100, 100)
    f = fog((100, 100, 100), MAX_DEPTH * 2)
    assert f == BG, f
    # enemy pixel shader
    assert enemy_px(0.5, 0.1, False, 0.0, False, False) is not None  # head
    assert enemy_px(0.02, 0.1, False, 0.0, False, False) is None     # transparent
    assert enemy_px(0.5, 0.9, False, 0.0, False, True) is not None   # corpse
    assert enemy_px(0.5, 0.5, True, 0.0, False, False)[0] > 2.0      # flash hot
    # mouse parser: synthetic SGR + arrows
    t = Term.__new__(Term)
    t.fd, t.buf, t.truecolor = -1, b'', True
    orig_read, orig_select = os.read, select.select
    try:
        os.read = lambda fd, n: chunks.pop(0) if chunks else b''
        select.select = lambda r, w, e, t=None: ([r[0]], [], [])
        chunks = [b'\x1b[<0;10;20M\x1b[Aa ']
        keys, mev = Term.poll(t)
        assert 'up' in keys and 'a' in keys and ' ' in keys, (keys, mev)
        assert mev and mev[0][0] == 'lpress' and mev[0][1:] == (10, 20), mev
        # legacy X10 packets decode as mouse, never as keys
        # (regression: X10 was misread as 'M'+space = phantom firing + flashing)
        t.buf = b''
        chunks = [b'\x1b[M 0%']  # left press at (16,5): ' '=0,'0'=16,'%'=5
        keys, mev = Term.poll(t)
        assert keys == set(), keys
        assert mev == [('lpress', 16, 5)], mev
        t.buf = b''
        chunks = [b'\x1b[MC0%\x1b[M#0%']  # motion at (16,5), then release
        keys, mev = Term.poll(t)
        assert keys == set(), keys
        assert mev == [('motion', 16, 5), ('release',)], mev
        # every button-up encoding must end a trigger hold (stuck-hold
        # regression: one click kept firing forever)
        t.buf = b''
        chunks = [b'\x1b[<0;10;20m\x1b[<3;10;20M\x1b[<3;10;20m']
        keys, mev = Term.poll(t)
        assert keys == set(), keys
        assert mev == [('release',), ('release',), ('release',)], mev
        # packets split across reads stay buffered, never leak as keys
        t.buf = b''
        chunks = [b'\x1b[M ']
        keys, mev = Term.poll(t)
        assert keys == set() and mev == [] and t.buf == b'\x1b[M ', (keys, mev, t.buf)
        chunks = [b'0%']
        keys, mev = Term.poll(t)
        assert mev == [('lpress', 16, 5)] and keys == set(), (keys, mev)
    finally:
        os.read, select.select = orig_read, orig_select
    # game logic reuse
    g = Game()
    assert len(g.enemies) > 0
    # spawn must have an open view (regression: old spawn faced a wall 0.5 away)
    assert not is_wall(g.player['x'], g.player['y'])
    d0, _ = cast_ray(g.player['x'], g.player['y'], g.player['angle'])
    assert d0 > 2.0, f"spawn faces wall {d0}"
    g.pitch = 0.0
    mouse_look(g, 5, -2)
    assert g.player['angle'] > 0 and g.pitch > 0
    # crosshair stays at screen-center vertically when pitching
    # (regression: it followed the horizon, so looking up/down felt dead)
    def _cross_rows():
        f, _, _ = render(g, 60, 20)
        return sorted(yy for yy in range(32)
                      if f.top[yy // 2][30] == CROSS or f.bot[yy // 2][30] == CROSS)
    g.pitch = 0.0
    rows0 = _cross_rows()
    assert 16 in rows0, rows0
    g.pitch = 8.0
    assert _cross_rows() == rows0, (rows0, _cross_rows())
    g.pitch = 0.0
    # pixel minimap: wall cell crisp inside panel, gone when toggled off
    g.enemies = []
    g.pickups = []
    g.show_map = True
    fmm, _, _ = render(g, 80, 30)
    assert fmm.bot[1][3] == MM_WALL, fmm.bot[1][3]  # map (0,0) is wall
    g.show_map = False
    fmm2, _, _ = render(g, 80, 30)
    assert fmm2.bot[1][3] != MM_WALL
    g.show_map = True
    # tiny render + serialize smoke test (no terminal needed)
    fr, zbuf, hor = render(g, 40, 18)
    assert len(zbuf) == 40 and 0 < hor < 2 * 18
    fake = Term.__new__(Term)
    fake.truecolor = True
    s = serialize(fake, fr)
    assert HALF in s and s.startswith("\x1b[1;1H")
    # frame geometry: CUP-positioned rows, no raw newlines (those cause
    # wrap-artifact blank lines), exactly cols cells per row
    assert "\n" not in s, "raw newline in frame"
    assert s.count(HALF) == 40 * (18 - 4), s.count(HALF)
    rows = re.split(r'\x1b\[\d+;1H', s)
    body = [re.sub(r'\x1b\[[0-9;]+m', '', r) for r in rows if r]
    assert len(body) == 18 - 4, len(body)
    assert all(r == HALF * 40 for r in body), [r[:20] for r in body if r != HALF * 40][:2]
    print("ALL HD TESTS PASSED")


if __name__ == '__main__':
    if '--test' in sys.argv:
        run_test()
    else:
        try:
            main()
        except (KeyboardInterrupt, RuntimeError) as e:
            try:
                sys.stdout.write("\x1b[0m\x1b[?25h\x1b[?1049l\n")
                sys.stdout.flush()
            except Exception:
                pass
            if isinstance(e, RuntimeError):
                sys.stderr.write(str(e) + "\n")
