"""Regression tests: python3 -m unittest -v."""
import copy
import math
import random
import unittest
from unittest.mock import patch

import fps_hd as fps


class GameTests(unittest.TestCase):
    def setUp(self):
        random.seed(1234)
        self.g = fps.Game()

    def test_map_rays_and_reachable_spawns(self):
        self.assertTrue(all(len(row) == fps.MAP_W for row in fps.MAP))
        reachable = fps.distance_field(1.5, 1.5)
        for y, row in enumerate(fps.MAP):
            for x, tile in enumerate(row):
                if tile == '#':
                    continue
                for angle in range(0, 360, 15):
                    self.assertGreaterEqual(fps.cast_ray(x + .5, y + .5, math.radians(angle))[0], 0)
        for _ in range(100):
            x, y = fps.random_empty(6, self.g.player)
            self.assertIn((int(x), int(y)), reachable)
            self.assertGreaterEqual(math.hypot(x - self.g.player['x'], y - self.g.player['y']), 6)

    def test_out_of_bounds_is_solid(self):
        for x, y in [(-.1, 1.5), (1.5, -.1), (fps.MAP_W, 2), (2, fps.MAP_H)]:
            self.assertTrue(fps.is_wall(x, y))

    def test_pause_freezes_everything_and_blocks_reload(self):
        self.g.player['mag'] = 0
        self.g.paused = True
        before = copy.deepcopy(self.g.__dict__)
        self.g.start_reload()
        self.g.shoot()
        self.g.update(.05)
        self.assertEqual(before, self.g.__dict__)

    def test_fire_rate_and_reload_conserve_ammo(self):
        self.g.enemies = []
        self.assertTrue(self.g.shoot())
        mag = self.g.player['mag']
        self.assertFalse(self.g.shoot())
        self.assertEqual(self.g.player['mag'], mag)
        for _ in range(math.ceil(fps.SHOT_INTERVAL / .05)):
            self.g.update(.05)
        self.assertTrue(self.g.shoot())
        total = self.g.player['mag'] + self.g.player['reserve']
        self.g.start_reload()
        for _ in range(23):
            self.g.update(.05)
        self.assertEqual(self.g.player['mag'], fps.MAG_SIZE)
        self.assertEqual(total, self.g.player['mag'] + self.g.player['reserve'])

    def test_wall_blocks_melee(self):
        # Two walkable cells separated diagonally by a solid corner.
        self.g.player.update(x=1.9, y=2.1)
        enemy = self.g.enemies[0]
        enemy.update(x=2.1, y=1.9)
        self.g.enemies = [enemy]
        with patch.object(fps, 'los_clear', return_value=False):
            self.g.update(.05)
        self.assertEqual(self.g.player['hp'], 100)
        self.assertFalse(fps.los_clear(4.5, 3.5, 7.5, 3.5))

    def test_enemy_navigates_maze(self):
        enemy = self.g.enemies[0]
        self.g.player.update(x=4.5, y=3.5)
        enemy.update(x=7.5, y=3.5, wake=0)
        self.g.enemies = [enemy]
        self.g.pickups = []
        # The two-cell cover block forces a detour.
        for _ in range(1500):
            self.g.update(.05)
            self.assertFalse(fps.is_wall(enemy['x'], enemy['y']))
            if self.g.player['hp'] < 100:
                break
        self.assertLess(self.g.player['hp'], 100)

    def test_shotgun_hits_group_and_interrupts_attack(self):
        self.g.player.update(x=1.5, y=1.5, angle=0)
        self.g.enemies = self.g.enemies[:2]
        for i, enemy in enumerate(self.g.enemies):
            enemy.update(x=3.5, y=1.5 + i * .2, hp=58, wake=0, windup=.3)
        self.g.shoot()
        self.assertEqual(self.g.kills, 2)
        self.assertTrue(all(e['windup'] == 0 for e in self.g.enemies))
        self.assertGreater(self.g.hit_marker, 0)
        self.assertGreater(self.g.intermission, 0)

    def test_attack_is_telegraphed_and_can_be_dodged(self):
        enemy = self.g.enemies[0]
        self.g.player.update(x=2.5, y=1.5, angle=0)
        enemy.update(x=3.2, y=1.5, wake=0, kind='grunt', atk_cd=0, windup=0)
        self.g.enemies = [enemy]
        self.g.update(.05)
        self.assertEqual(self.g.player['hp'], 100)
        self.assertGreater(enemy['windup'], 0)
        self.g.held = {'back': 1}
        self.assertTrue(self.g.dash())
        self.assertFalse(self.g.dash())
        for _ in range(14):
            self.g.update(.05)
        self.assertEqual(self.g.player['hp'], 100)

    def test_dash_does_not_cross_cover(self):
        self.g.player.update(x=4.5, y=3.5, angle=0)
        self.g.enemies = []
        self.g.dash()
        for _ in range(5):
            self.g.update(.05)
        self.assertLess(self.g.player['x'], 5)
        self.assertFalse(fps.is_wall(self.g.player['x'], self.g.player['y']))

    def test_wave_break_resupply_and_victory(self):
        self.g.enemies = []
        self.g.player['hp'] = 50
        self.g.finish_wave()
        self.assertEqual(self.g.player['hp'], 75)
        self.assertFalse(self.g.shoot())
        for _ in range(81):
            self.g.update(.05)
        self.assertEqual(self.g.wave, 2)
        self.assertEqual(self.g.player['mag'], fps.MAG_SIZE)
        self.assertTrue(any(e['kind'] == 'runner' for e in self.g.enemies))
        self.g.wave = fps.FINAL_WAVE
        self.g.finish_wave()
        self.assertTrue(self.g.victory)
        before = copy.deepcopy(self.g.__dict__)
        self.g.update(.05)
        self.assertEqual(before, self.g.__dict__)
        self.assertFalse(self.g.shoot())

    def test_vertical_mouse_drift_does_not_change_aim(self):
        self.g.player.update(x=1.5, y=1.5, angle=0)
        enemy = self.g.enemies[0]
        enemy.update(x=5.5, y=1.5, hp=1000, wake=0)
        self.g.enemies = [enemy]
        fps.mouse_look(self.g, 0, 100)
        self.assertEqual(self.g.pitch, 0)
        self.g.shoot()
        self.assertLess(enemy['hp'], 1000)

    def test_complete_run_has_enough_ammo_and_reaches_victory(self):
        # Perfect-aim stationary bot checks progression, not human difficulty.
        for _ in range(12000):
            visible = [e for e in self.g.enemies if e['hp'] > 0 and
                       fps.los_clear(self.g.player['x'], self.g.player['y'], e['x'], e['y'])]
            if visible:
                e = min(visible, key=lambda e: math.hypot(e['x'] - self.g.player['x'], e['y'] - self.g.player['y']))
                self.g.player['angle'] = math.atan2(e['y'] - self.g.player['y'], e['x'] - self.g.player['x'])
            self.g.shoot()
            self.g.update(1 / 60)
            if self.g.game_over or self.g.victory:
                break
        self.assertTrue(self.g.victory)
        self.assertEqual(self.g.kills, sum(3 + wave * 2 for wave in range(1, fps.FINAL_WAVE + 1)))

    def test_enemies_end_an_idle_run(self):
        self.g.pickups = []
        for _ in range(2400):
            self.g.update(.05)
            if self.g.game_over:
                break
        self.assertTrue(self.g.game_over)
        self.assertEqual(self.g.player['hp'], 0)

    def test_restart_resets_run_state(self):
        self.g.wave = fps.FINAL_WAVE
        self.g.finish_wave()
        self.g.player['hp'] = 0
        self.g.game_over = True
        self.g.dash_cooldown = 1
        self.g.reset()
        self.assertFalse(self.g.victory or self.g.game_over or self.g.auto_fire)
        self.assertEqual(self.g.wave, 1)
        self.assertEqual(self.g.player['hp'], 100)
        self.assertEqual(self.g.player['mag'], fps.MAG_SIZE)
        self.assertEqual(self.g.dash_cooldown, 0)

    def test_tactical_map_freezes_gameplay(self):
        self.g.big_map = True
        before = copy.deepcopy(self.g.__dict__)
        self.g.update(.05)
        self.g.shoot()
        self.g.dash()
        self.assertEqual(before, self.g.__dict__)

    def test_minimap_player_uses_containing_cell(self):
        self.g.pickups = []
        self.g.enemies = []
        for x, y in [(1.5, 1.5), (1.99, 1.99), (4.8, 3.5), (8.5, 3.8)]:
            self.g.player.update(x=x, y=y, angle=0)
            frame = fps.Frame(80, 26)
            fps.draw_minimap_pixels(frame, self.g, 80, 52)
            markers = [(cx, cy) for cy in range(52) for cx in range(80)
                       if (frame.top if cy % 2 == 0 else frame.bot)[cy // 2][cx] == fps.MM_PLAYER]
            self.assertEqual(markers, [(3 + math.floor(x), 3 + math.floor(y))])
            # The east-facing marker must not paint over cover at (5, 3).
            if x == 4.8:
                self.assertEqual(frame.top[3][8], fps.MM_WALL)

    def test_minimap_visible_at_minimum_size(self):
        frame, _, _ = fps.render(self.g, 40, 16)
        self.assertTrue(any(fps.MM_PLAYER in row for row in frame.top + frame.bot))

    def test_serializer_compares_encoded_colors(self):
        term = fps.Term.__new__(fps.Term)
        term.truecolor = False
        frame = fps.Frame(3, 1)
        frame.top[0] = [(80.0, 80.0, 80.0), (80.1, 80.1, 80.1), (80.2, 80.2, 80.2)]
        output = fps.serialize(term, frame)
        self.assertEqual(output.count('38;5;'), 1)
        self.assertEqual(output.count(fps.HALF), 3)

    def test_hud_fits_minimum_terminal(self):
        term = fps.Term.__new__(fps.Term)
        term.truecolor = False
        for width in (40, 60, 100):
            for line in fps.hud_lines(term, self.g, width):
                plain = fps.re.sub(r'\x1b\[[0-9;]*m', '', line)
                self.assertLessEqual(len(plain), width)


class InputTests(unittest.TestCase):
    def test_split_mouse_and_unknown_csi_do_not_swallow_keys(self):
        term = fps.Term.__new__(fps.Term)
        term.fd, term.buf = -1, b''
        chunks = [b'\x1b[<0;1', b'0;20M\x1b[3~w\x1b[1;5Cq']
        with patch.object(fps.select, 'select', return_value=([-1], [], [])), patch.object(fps.os, 'read', side_effect=chunks):
            self.assertEqual(term.poll(), (set(), []))
            keys, mouse = term.poll()
        self.assertEqual(keys, {'w', 'q'})
        self.assertEqual(mouse, [('lpress', 10, 20)])
        self.assertEqual(term.buf, b'')

    def test_modified_mouse_buttons(self):
        self.assertEqual(fps.Term._decode_mouse(4, 2, 3, True), [('lpress', 2, 3)])
        self.assertEqual(fps.Term._decode_mouse(48, 2, 3, True), [('drag', 2, 3)])


if __name__ == '__main__':
    unittest.main()
