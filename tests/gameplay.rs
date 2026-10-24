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
fn pickups_are_collected_and_corpses_fade_during_wave_break() {
    let mut game = arena();
    game.player.pos = Vec2::new(1.5, 7.5);
    game.player.angle = 0.0;
    game.enemies = vec![enemy(Vec2::new(3.0, 7.5), Kind::Grunt)];
    assert!(game.shoot());
    assert!(game.break_time > 0.0);
    game.player.hp = 50;
    let reserve = game.player.reserve;
    game.pickups = vec![
        Pickup {
            pos: Vec2::new(2.5, 7.5),
            kind: PickupKind::Medkit,
        },
        Pickup {
            pos: Vec2::new(2.5, 7.5),
            kind: PickupKind::Ammo,
        },
    ];
    for _ in 0..60 {
        game.tick(
            1.0 / 120.0,
            Controls {
                forward: 1.0,
                ..Controls::default()
            },
        );
    }
    assert!(game.break_time > 0.0);
    assert_eq!(game.wave, 1);
    assert!(game.pickups.is_empty());
    assert_eq!(game.player.hp, 80);
    assert_eq!(game.player.reserve, reserve + AMMO_PICKUP);
    assert_eq!(game.enemies[0].corpse, 0.0);
    assert_eq!(game.enemies[0].flash, 0.0);
}

#[test]
fn shotgun_hits_follow_the_visible_body() {
    // Up close, the crosshair is on a brute's shoulder well off its centre.
    let mut game = arena();
    game.player.pos = Vec2::new(1.5, 7.5);
    game.player.angle = 0.0;
    game.enemies = vec![enemy(Vec2::new(3.0, 8.1), Kind::Brute)];
    assert!(game.shoot());
    assert_eq!(game.enemies[0].hp, Kind::Brute.health() - 72);

    // At range, only the pellet spread beyond the body still connects.
    let damage_at = |lateral: f32| {
        let mut game = arena();
        game.player.pos = Vec2::new(1.5, 11.5);
        game.player.angle = 0.0;
        game.enemies = vec![enemy(Vec2::new(11.5, 11.5 + lateral), Kind::Grunt)];
        game.shoot();
        Kind::Grunt.health() - game.enemies[0].hp
    };
    let centred = damage_at(0.0);
    let fringe = damage_at(1.2);
    assert!(centred > fringe && fringe > 0, "{centred} {fringe}");
    assert_eq!(damage_at(1.8), 0);
}

#[test]
fn enemies_out_of_sight_separate_even_from_the_same_spot() {
    let mut game = arena();
    game.player.pos = Vec2::new(4.5, 3.5);
    game.enemies = vec![
        enemy(Vec2::new(7.5, 3.5), Kind::Grunt),
        enemy(Vec2::new(7.5, 3.5), Kind::Grunt),
    ];
    assert!(!world::visible(game.enemies[0].pos, game.player.pos));
    let routes = world::routes(game.player.pos);
    let steps = |pos: Vec2| {
        let (x, y) = pos.cell();
        routes[y as usize * world::WIDTH + x as usize]
    };
    let start = steps(game.enemies[0].pos);
    advance(&mut game, 1.5);
    assert!((game.enemies[0].pos - game.enemies[1].pos).length() > 0.4);
    // Separation must not hold either enemy at a cell centre it needs to leave.
    for e in &game.enemies {
        assert!(world::fits(e.pos, 0.18));
        assert!(steps(e.pos) < start);
    }
}

#[test]
fn medkits_restock_and_ammo_comes_from_kills_and_wave_resupply() {
    let mut game = arena();
    game.player.pos = Vec2::new(1.5, 1.5);
    game.player.angle = 0.0;
    game.enemies = (0..3)
        .map(|i| enemy(Vec2::new(3.5, 1.3 + i as f32 * 0.2), Kind::Grunt))
        .collect();
    let reserve = game.player.reserve;
    assert!(game.shoot());
    assert_eq!(game.kills, 3);
    let ammo = |game: &Game| {
        game.pickups
            .iter()
            .filter(|p| p.kind == PickupKind::Ammo)
            .count()
    };
    assert_eq!(ammo(&game), 1, "every third kill drops shells");
    advance(&mut game, 4.1);
    assert_eq!(game.wave, 2);
    assert_eq!(game.player.reserve, reserve + WAVE_RESUPPLY);
    assert_eq!(ammo(&game), 1, "uncollected drops stay");
    for spot in MEDKIT_SPOTS {
        assert!(
            game.pickups
                .iter()
                .any(|p| p.kind == PickupKind::Medkit && p.pos == spot)
        );
    }
}

#[test]
fn running_dry_drops_one_crate_in_the_farther_side_lane() {
    let mut game = arena();
    game.player.pos = Vec2::new(4.5, 12.5);
    game.player.shells = 0;
    game.player.reserve = 0;
    game.enemies = vec![enemy(Vec2::new(16.5, 1.5), Kind::Grunt)];
    advance(&mut game, 0.5);
    assert_eq!(game.pickups.len(), 1);
    assert_eq!(game.pickups[0].kind, PickupKind::Ammo);
    assert_eq!(game.pickups[0].pos, Vec2::new(16.5, 6.5));
    assert!(world::fits(game.pickups[0].pos, 0.22));
}

#[test]
fn winning_offers_endless_waves_and_runs_record_the_best_score() {
    let mut game = arena();
    game.wave = FINAL_WAVE;
    game.best = 50;
    game.player.pos = Vec2::new(1.5, 1.5);
    game.player.angle = 0.0;
    game.enemies = vec![enemy(Vec2::new(3.5, 1.5), Kind::Grunt)];
    game.shoot();
    assert_eq!(game.phase, Phase::Won);
    assert!(game.new_best);
    assert_eq!(game.best, game.score);
    game.continue_endless();
    assert_eq!(game.phase, Phase::Playing);
    advance(&mut game, 4.1);
    assert_eq!(game.wave, FINAL_WAVE + 1);
    assert_eq!(game.alive(), 15);
    assert_eq!(game.wave_label(), "WAVE 6 ENDLESS");

    let mut late = arena();
    late.phase = Phase::Won;
    late.wave = 30;
    late.continue_endless();
    advance(&mut late, 4.1);
    assert_eq!(late.alive(), MAX_ENEMIES);

    // A losing run below the best leaves it unchanged.
    let mut lost = arena();
    lost.best = 10_000;
    lost.player.hp = 1;
    lost.player.pos = Vec2::new(2.5, 1.5);
    lost.enemies = vec![enemy(Vec2::new(3.2, 1.5), Kind::Grunt)];
    advance(&mut lost, 1.5);
    assert_eq!(lost.phase, Phase::Lost);
    assert!(!lost.new_best);
    assert_eq!(lost.best, 10_000);
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
