use terminalshooter::{
    game::*,
    world::{self, Vec2},
};

fn arena() -> Game {
    let mut game = Game::new(42);
    game.start();
    game.pickups.clear();
    game
}
fn enemy(pos: Vec2, kind: Kind) -> Enemy {
    let mut enemy = Enemy::new(pos, kind);
    enemy.wake = 0.0;
    enemy
}
fn advance(game: &mut Game, seconds: f32) {
    for _ in 0..(seconds * 120.0).ceil() as usize {
        game.tick(1.0 / 120.0, Controls::default());
    }
}

#[test]
fn spawned_enemies_are_reachable_spaced_and_safe() {
    for seed in 1..50 {
        let game = Game::new(seed);
        let routes = world::routes(game.player.pos);
        assert_eq!(game.alive(), 5);
        for (i, e) in game.enemies.iter().enumerate() {
            assert!(world::fits(e.pos, 0.18));
            assert!((e.pos - game.player.pos).length() >= 5.0);
            let (x, y) = e.pos.cell();
            assert_ne!(routes[y as usize * world::WIDTH + x as usize], u16::MAX);
            assert!(
                game.enemies[..i]
                    .iter()
                    .all(|other| (e.pos - other.pos).length() > 1.2)
            );
        }
    }
}

#[test]
fn shotgun_catches_a_group_and_clear_starts_break() {
    let mut game = arena();
    game.player.pos = Vec2::new(1.5, 1.5);
    game.player.angle = 0.0;
    game.enemies = vec![
        enemy(Vec2::new(3.5, 1.5), Kind::Grunt),
        enemy(Vec2::new(3.5, 1.7), Kind::Grunt),
    ];
    assert!(game.shoot());
    assert_eq!(game.kills, 2);
    assert_eq!(game.score, 300);
    assert!(game.break_time > 0.0);
    assert!(!game.shoot());
    advance(&mut game, 4.1);
    assert_eq!(game.wave, 2);
    assert_eq!(game.player.shells, MAGAZINE);
    assert!(game.enemies.iter().any(|e| e.kind == Kind::Runner));
}

#[test]
fn wall_blocks_shotgun() {
    let mut game = arena();
    game.player.pos = Vec2::new(4.5, 3.5);
    game.player.angle = 0.0;
    game.enemies = vec![enemy(Vec2::new(7.5, 3.5), Kind::Grunt)];
    game.shoot();
    assert_eq!(game.enemies[0].hp, 58);
}

#[test]
fn attack_windup_allows_interrupt_and_dodge() {
    let mut game = arena();
    game.player.pos = Vec2::new(2.5, 1.5);
    game.player.angle = 0.0;
    game.enemies = vec![enemy(Vec2::new(3.2, 1.5), Kind::Brute)];
    advance(&mut game, 0.1);
    assert!(game.enemies[0].windup > 0.0);
    assert_eq!(game.player.hp, 100);
    game.shoot();
    assert_eq!(game.enemies[0].windup, 0.0);
    advance(&mut game, 0.35);
    game.dash(Controls {
        forward: -1.0,
        ..Controls::default()
    });
    advance(&mut game, 0.9);
    assert_eq!(game.player.hp, 100);
    assert!(game.player.pos.x < 2.0);
}

#[test]
fn dash_collides_with_cover_and_has_a_cooldown() {
    let mut game = arena();
    game.enemies.clear();
    game.player.pos = Vec2::new(4.5, 3.5);
    game.player.angle = 0.0;
    game.dash(Controls::default());
    advance(&mut game, 0.2);
    assert!(game.player.pos.x < 4.8);
    game.dash(Controls::default());
    assert_eq!(game.player.dash_time, 0.0);
}

#[test]
fn diagonal_and_cardinal_movement_have_equal_speed() {
    let mut a = arena();
    let mut b = arena();
    a.enemies.clear();
    b.enemies.clear();
    let start = a.player.pos;
    for _ in 0..30 {
        a.tick(
            1.0 / 120.0,
            Controls {
                forward: 1.0,
                ..Controls::default()
            },
        );
        b.tick(
            1.0 / 120.0,
            Controls {
                forward: 1.0,
                strafe: 1.0,
                ..Controls::default()
            },
        );
    }
    assert!(((a.player.pos - start).length() - (b.player.pos - start).length()).abs() < 0.001);
}

#[test]
fn reload_conserves_ammo_and_fire_has_one_cooldown() {
    let mut game = arena();
    game.enemies.clear();
    assert!(game.shoot());
    assert!(!game.shoot());
    advance(&mut game, SHOT_DELAY + 0.01);
    assert!(game.shoot());
    let total = game.player.shells + game.player.reserve;
    game.reload();
    assert!(!game.shoot());
    advance(&mut game, RELOAD_TIME + 0.01);
    assert_eq!(game.player.shells, MAGAZINE);
    assert_eq!(total, game.player.shells + game.player.reserve);
}

#[test]
fn title_pause_map_and_end_screens_freeze_simulation() {
    for phase in [
        Phase::Title,
        Phase::Paused,
        Phase::Map,
        Phase::Won,
        Phase::Lost,
        Phase::Help,
    ] {
        let mut game = arena();
        game.phase = phase;
        let player = game.player.clone();
        advance(&mut game, 2.0);
        game.reload();
        game.dash(Controls::default());
        assert!(!game.shoot());
        assert_eq!(game.player, player);
        assert_eq!(game.time, 0.0);
    }
}

#[test]
fn enemy_goes_around_cover_and_idle_player_eventually_loses() {
    let mut game = arena();
    game.player.pos = Vec2::new(4.5, 3.5);
    game.enemies = vec![enemy(Vec2::new(7.5, 3.5), Kind::Grunt)];
    for _ in 0..120 * 60 {
        game.tick(1.0 / 120.0, Controls::default());
        assert!(!world::solid(game.enemies[0].pos));
        if game.phase == Phase::Lost {
            break;
        }
    }
    assert_eq!(game.phase, Phase::Lost);
}

#[test]
fn complete_five_wave_run_has_no_ammo_or_navigation_deadlock() {
    for seed in [1, 7, 42] {
        let mut game = Game::new(seed);
        game.start();
        for _ in 0..120 * 240 {
            let target = game
                .enemies
                .iter()
                .filter(|e| e.alive() && world::visible(game.player.pos, e.pos))
                .min_by(|a, b| {
                    (a.pos - game.player.pos)
                        .length()
                        .total_cmp(&(b.pos - game.player.pos).length())
                });
            if let Some(enemy) = target {
                game.player.angle = (enemy.pos - game.player.pos).angle();
            }
            game.tick(
                1.0 / 120.0,
                Controls {
                    fire: true,
                    ..Controls::default()
                },
            );
            if matches!(game.phase, Phase::Won | Phase::Lost) {
                break;
            }
        }
        assert_eq!(game.phase, Phase::Won, "seed {seed}, wave {}", game.wave);
        assert_eq!(game.kills, 45);
    }
}
