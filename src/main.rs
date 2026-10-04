mod cells;
mod fire;
mod fireflies;
mod hives;
mod lighting;
mod rabbits;

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};
use cells::*;
use noise::{NoiseFn, OpenSimplex};
use rand::RngExt;
use rand::rng;
use std::collections::HashMap;

const CELLSIZE: f32 = 5.0;
const SLANT: f64 = 0.35;
const WIND_SPEED: f64 = 0.05;
const SWAY: f64 = 0.03;
const NSCALE: f64 = 0.005;
const SUN_THRESHOLD: f64 = -0.1;
const RAIN_CLOUD_THRESHOLD: f64 = -0.3;
const NIGHT_RAIN_CLOUD_THRESHOLD: f64 = 0.0;
const RAIN_SPLASH_ROLL_INTERVAL: f32 = 0.1;
const RAIN_SPLASH_CHANCE_PER_TILE_PER_ROLL: f64 = 0.0003;
const RAIN_SPLASH_START_ALPHA: f32 = 0.1;
const RAIN_WATER_AMOUNT: f32 = 1.0;
const INITIAL_ROOT_COUNT: usize = 10;
const WATER_UPDATE_INTERVAL: f32 = 0.1;
const WATER_EVAPORATION_PER_TICK: f32 = 0.002;
const WATER_SPREAD_FRACTION_PER_TICK: f32 = 0.06;
const WATER_MIN_SPREAD_AMOUNT: f32 = 0.08;
const WATER_MIN_AMOUNT: f32 = 0.01;

const ORGANISM_COLOR: Color = Color::srgb(0.2, 0.7, 1.0);
const DIRT_COLOR: Color = Color::srgb(0.35, 0.2, 0.08);
const SHADOW_COLOR: Color = Color::srgb(0.0, 0.0, 0.0);
const RAIN_SPLASH_COLOR: Color = Color::srgb(0.15, 0.55, 1.0);

#[derive(Resource)]
struct LightNoise(OpenSimplex);

#[derive(Resource, Default)]
struct RainClock {
    elapsed: f32,
}

#[derive(Resource, Default)]
struct WaterField {
    amounts: HashMap<IVec2, f32>,
    entities: HashMap<IVec2, Entity>,
    elapsed: f32,
}

#[derive(Resource)]
struct SimulationPaused(bool);

fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut paused: ResMut<SimulationPaused>) {
    if keys.just_pressed(KeyCode::KeyP) {
        paused.0 = !paused.0;
        info!("Simulation {}", if paused.0 { "paused" } else { "running" });
    }
}

fn simulation_running(paused: Res<SimulationPaused>) -> bool {
    !paused.0
}

fn light_value(n: &OpenSimplex, x: i32, y: i32, t: f64) -> f64 {
    let wx = x as f64 * CELLSIZE as f64 * NSCALE + 500.37;
    let wy = y as f64 * CELLSIZE as f64 * NSCALE + 500.91;
    let sx = wx + wy * SLANT + t * WIND_SPEED + (t * 0.7 + wy * 3.0).sin() * SWAY;
    n.get([sx, wy])
}

fn shade(l: f64) -> f32 {
    let s = ((0.1 - l) / 0.2).clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    let light01 = (l + 1.0) / 2.0;
    (s * (1.0 - 0.5 * light01)) as f32
}

#[derive(Clone, Copy)]
struct TileEnvironment {
    light: f32,
    sun: bool,
    dirt: bool,
}

#[derive(Resource, Default)]
struct Environment {
    tiles: HashMap<IVec2, TileEnvironment>,
    min: IVec2,
    max: IVec2,
}

impl Environment {
    fn get(&self, position: IVec2) -> TileEnvironment {
        self.tiles
            .get(&position)
            .copied()
            .unwrap_or(TileEnvironment {
                light: 0.0,
                sun: false,
                dirt: false,
            })
    }

    fn inside(&self, p: IVec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

#[derive(Component)]
struct DirtTile;

#[derive(Component)]
struct LightTile {
    grid: IVec2,
}

#[derive(Component)]
struct WaterTile;

#[derive(Component)]
#[require(Sprite = default_sprite(), Transform = origin_transform())]
pub struct Organism;

fn default_sprite() -> Sprite {
    Sprite::from_color(ORGANISM_COLOR, Vec2::splat(100.0))
}

fn origin_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, 0.0)
}

// Startup and rain use exactly the same plant hierarchy and initial cell state.
pub(crate) fn spawn_root_plant(
    commands: &mut Commands,
    occupied: &mut Occupied,
    environment: &Environment,
    grid: IVec2,
) -> bool {
    if !environment.inside(grid)
        || !environment.get(grid).dirt
        || occupied.positions.contains(&grid)
    {
        return false;
    }
    commands
        .spawn((
            Plant,
            Transform::from_xyz(grid.x as f32 * CELLSIZE, grid.y as f32 * CELLSIZE, 0.0),
            Visibility::default(),
        ))
        .with_children(|plant| {
            spawn_pcell(plant, PCellRole::Root, Vec2::ZERO, grid, occupied, false);
        });
    true
}

fn respawn_plants_when_empty(
    mut commands: Commands,
    environment: Res<Environment>,
    mut occupied: ResMut<Occupied>,
    plants: Query<(), With<PCell>>,
) {
    if !plants.is_empty() {
        return;
    }

    occupied.positions.clear();
    let mut rng = rng();
    let mut spawned = 0;
    let mut attempts = 0;
    while spawned < INITIAL_ROOT_COUNT && attempts < INITIAL_ROOT_COUNT * 1000 {
        attempts += 1;
        let grid = IVec2::new(
            rng.random_range(environment.min.x..=environment.max.x),
            rng.random_range(environment.min.y..=environment.max.y),
        );
        if spawn_root_plant(&mut commands, &mut occupied, &environment, grid) {
            spawned += 1;
        }
    }
}

fn spawn_rain_splashes(
    time: Res<Time>,
    noise: Res<LightNoise>,
    cycle: Res<lighting::DayNightCycle>,
    mut clock: ResMut<RainClock>,
    mut water: ResMut<WaterField>,
    environment: Res<Environment>,
    light_tiles: Query<&LightTile>,
) {
    clock.elapsed += time.delta_secs();
    let now = time.elapsed_secs_f64();
    let rain_threshold = NIGHT_RAIN_CLOUD_THRESHOLD
        + (RAIN_CLOUD_THRESHOLD - NIGHT_RAIN_CLOUD_THRESHOLD) * f64::from(cycle.daylight());

    let rainy_positions: Vec<_> = light_tiles
        .iter()
        .filter_map(|tile| {
            (light_value(&noise.0, tile.grid.x, tile.grid.y, now) < rain_threshold)
                .then_some(tile.grid)
        })
        .collect();

    if rainy_positions.is_empty() {
        clock.elapsed = clock.elapsed.min(RAIN_SPLASH_ROLL_INTERVAL);
        return;
    }

    let mut rng = rng();

    while clock.elapsed >= RAIN_SPLASH_ROLL_INTERVAL {
        clock.elapsed -= RAIN_SPLASH_ROLL_INTERVAL;

        for &origin in &rainy_positions {
            if !rng.random_bool(RAIN_SPLASH_CHANCE_PER_TILE_PER_ROLL) {
                continue;
            }

            let offsets: &[IVec2] = match rng.random_range(0..3) {
                0 => &[IVec2::ZERO],

                1 => &[IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE],

                _ => &[IVec2::ZERO, IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y],
            };

            for &offset in offsets {
                let position = origin + offset;

                if !environment.inside(position) {
                    continue;
                }

                *water.amounts.entry(position).or_default() += RAIN_WATER_AMOUNT;
            }
        }
    }
}

fn update_water(
    mut commands: Commands,
    time: Res<Time>,
    environment: Res<Environment>,
    mut water: ResMut<WaterField>,
    mut water_tiles: Query<&mut Sprite, With<WaterTile>>,
) {
    water.elapsed += time.delta_secs();

    while water.elapsed >= WATER_UPDATE_INTERVAL {
        water.elapsed -= WATER_UPDATE_INTERVAL;

        let snapshot = water.amounts.clone();

        let mut changes: HashMap<IVec2, f32> = HashMap::new();

        for (&position, &amount) in &snapshot {
            if amount < WATER_MIN_SPREAD_AMOUNT {
                continue;
            }

            let neighbors: Vec<_> = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y]
                .into_iter()
                .map(|offset| position + offset)
                .filter(|neighbor| {
                    environment.inside(*neighbor)
                        && snapshot.get(neighbor).copied().unwrap_or_default() < amount
                })
                .collect();

            if neighbors.is_empty() {
                continue;
            }

            let total_spread = amount * WATER_SPREAD_FRACTION_PER_TICK;

            let per_neighbor = total_spread / neighbors.len() as f32;

            *changes.entry(position).or_default() -= total_spread;

            for neighbor in neighbors {
                *changes.entry(neighbor).or_default() += per_neighbor;
            }
        }

        for (position, change) in changes {
            *water.amounts.entry(position).or_default() += change;
        }

        for amount in water.amounts.values_mut() {
            *amount = (*amount - WATER_EVAPORATION_PER_TICK).max(0.0);
        }

        water
            .amounts
            .retain(|_, amount| *amount >= WATER_MIN_AMOUNT);
    }

    let old_positions: Vec<_> = water.entities.keys().copied().collect();

    for position in old_positions {
        let entity = water.entities[&position];

        if let Some(&amount) = water.amounts.get(&position) {
            if let Ok(mut sprite) = water_tiles.get_mut(entity) {
                sprite.color =
                    RAIN_SPLASH_COLOR.with_alpha(RAIN_SPLASH_START_ALPHA * amount.min(1.0));
            }
        } else {
            commands.entity(entity).despawn();
            water.entities.remove(&position);
        }
    }

    let new_positions: Vec<_> = water
        .amounts
        .iter()
        .filter_map(|(&position, &amount)| {
            (!water.entities.contains_key(&position)).then_some((position, amount))
        })
        .collect();

    for (position, amount) in new_positions {
        let entity = commands
            .spawn((
                WaterTile,
                Sprite::from_color(
                    RAIN_SPLASH_COLOR.with_alpha(RAIN_SPLASH_START_ALPHA * amount.min(1.0)),
                    Vec2::splat(CELLSIZE),
                ),
                Transform::from_xyz(
                    position.x as f32 * CELLSIZE,
                    position.y as f32 * CELLSIZE,
                    1.0,
                ),
            ))
            .id();

        water.entities.insert(position, entity);
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(1200, 800),
                ..default()
            }),
            ..default()
        }))
        // .insert_resource(Environment {sun: true, dirt: false})
        .insert_resource(Occupied::default())
        .insert_resource(RainClock::default())
        .insert_resource(WaterField::default())
        .insert_resource(SimulationPaused(true))
        .init_resource::<hives::HiveState>()
        .init_resource::<hives::BeeSpeed>()
        .init_resource::<lighting::DayNightCycle>()
        .init_resource::<fire::LightningStorm>()
        .init_resource::<fire::FireNoise>()
        .add_systems(
            Startup,
            (
                setup,
                hives::generate_initial_hives,
                hives::spawn_initial_bees,
                rabbits::spawn_initial_rabbits,
            )
                .chain(),
        )
        .add_systems(Update, hives::spawn_bee_on_click.run_if(simulation_running))
        .add_systems(Update, toggle_pause)
        .add_systems(
            Update,
            (
                lighting::advance_day_night,
                fireflies::update_fireflies,
                fire::night_lightning,
                fire::update_fire,
                fire::fade_lightning,
                fire::clean_burnt_branches,
                respawn_plants_when_empty,
                lighting::animate_light,
                spawn_rain_splashes,
                update_water,
                hives::bee_speed_controls,
                update_cells,
                grow_cells,
                check_suffocation,
                wilt_cells,
                bloom_cells,
                hives::bee_foraging_and_building,
                rabbits::rabbit_ai,
                rabbits::rabbit_mating,
            )
                .chain()
                .run_if(simulation_running),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut occupied: ResMut<Occupied>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    commands.spawn(Camera2d);

    // commands.spawn(Organism);

    let hx = (window.width() / CELLSIZE / 2.0).ceil() as i32;

    let hy = (window.height() / CELLSIZE / 2.0).ceil() as i32;

    let mut env = Environment {
        tiles: HashMap::new(),
        min: IVec2::new(-hx, -hy),
        max: IVec2::new(hx, hy),
    };

    let lnoise = OpenSimplex::new(25315);
    let dperlin = OpenSimplex::new(67292);

    for x in env.min.x..=env.max.x {
        for y in env.min.y..=env.max.y {
            let grid_pos = IVec2::new(x, y);

            let pos = Vec3::new(x as f32 * CELLSIZE, y as f32 * CELLSIZE, 0.0);

            let wx = x as f64 * CELLSIZE as f64 * NSCALE;

            let wy = y as f64 * CELLSIZE as f64 * NSCALE;

            let lval = light_value(&lnoise, x, y, 0.0);

            let light = ((lval + 1.0) / 2.0) as f32;

            let sun = lval > SUN_THRESHOLD;

            let dval = dperlin.get([wx, wy]);

            let dirt = dval > 0.2;

            env.tiles
                .insert(grid_pos, TileEnvironment { light, sun, dirt });

            if dirt {
                commands.spawn((
                    DirtTile,
                    Sprite::from_color(
                        DIRT_COLOR.with_alpha(1.0 - 0.1 * (dval as f32 + 1.0) / 2.0),
                        Vec2::splat(CELLSIZE),
                    ),
                    Transform::from_translation(pos - Vec3::Z),
                ));
            }

            commands.spawn((
                LightTile { grid: grid_pos },
                Sprite::from_color(SHADOW_COLOR.with_alpha(shade(lval)), Vec2::splat(CELLSIZE)),
                Transform::from_xyz(pos.x, pos.y, 3.0),
            ));
        }
    }

    let mut rng = rng();
    let mut roots_spawned = 0;

    while roots_spawned < INITIAL_ROOT_COUNT {
        let root_pos = IVec2::new(
            rng.random_range(env.min.x..=env.max.x),
            rng.random_range(env.min.y..=env.max.y),
        );

        if spawn_root_plant(&mut commands, &mut occupied, &env, root_pos) {
            roots_spawned += 1;
        }
    }

    commands.insert_resource(LightNoise(lnoise));

    commands.insert_resource(env);
}
