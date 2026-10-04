use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use noise::{NoiseFn, Perlin};
use rand::RngExt;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};

use crate::{
    CELLSIZE, Environment,
    cells::{PCell, PCellRole},
};

const SIDES: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
const GOLD: Color = Color::srgb(0.95, 0.68, 0.19);
const DARK: Color = Color::srgb(0.48, 0.28, 0.07);
const BLACK: Color = Color::srgb(0.025, 0.018, 0.01);
const OVAL_MEAN: f32 = 11.0;
const OVAL_STD_DEV: f32 = 2.0;
const WARP_RATIO: f32 = 0.18;
const NOISE_FREQUENCY: f64 = 2.0;
// World-space tiles: broad patches of high/low spawn likelihood.
const SPAWN_NOISE_SCALE: f64 = 0.035;
const MAX_SPAWN_CHANCE: f64 = 0.02;

const BEE_RANGE: i32 = 100;
const INITIAL_BEE_COUNT: usize = 5;
const BEE_Z: f32 = 2.0;
const HIVE_Z: f32 = 0.5;
const INSIDE_HIVE_Z: f32 = 0.2;
const BEE_COLOR: Color = Color::srgb(1.0, 0.85, 0.1);

fn spawn_chance(noise: &Perlin, p: IVec2) -> f64 {
    let value = noise.get([
        p.x as f64 * SPAWN_NOISE_SCALE + 17.31,
        p.y as f64 * SPAWN_NOISE_SCALE + 43.79,
    ]);
    let density = ((value + 1.0) * 0.5).clamp(0.0, 1.0);
    MAX_SPAWN_CHANCE * density.powi(3)
}

fn oval_dimension(rng: &mut impl RngExt) -> f32 {
    // Box–Muller normal sample; dimensions are full diameters in tiles.
    let u: f32 = rng.random_range(f32::EPSILON..1.0);
    let angle: f32 = rng.random_range(0.0..std::f32::consts::TAU);
    (OVAL_MEAN + OVAL_STD_DEV * (-2.0 * u.ln()).sqrt() * angle.cos()).max(3.0)
}

fn warped_oval(origin: IVec2, rng: &mut impl RngExt) -> HashSet<IVec2> {
    let width = oval_dimension(rng);
    let height = oval_dimension(rng);
    let seed: u32 = rng.random();
    let perlin = Perlin::new(seed);
    let strength = Vec2::new(width, height) * WARP_RATIO;
    let radius = Vec2::new(width, height) * 0.5;
    // Pad the sampling region so outward warps aren't clipped at the bounds.
    let extent = (radius + strength + Vec2::ONE).ceil().as_ivec2();
    let mut mask = HashSet::new();
    for y in -extent.y..=extent.y {
        for x in -extent.x..=extent.x {
            let nx = (x as f64 / width as f64 + 0.5) * NOISE_FREQUENCY;
            let ny = (y as f64 / height as f64 + 0.5) * NOISE_FREQUENCY;
            let warp_x = perlin.get([nx, ny, seed as f64]) as f32 * strength.x;
            let warp_y = perlin.get([nx, ny, seed as f64 + 100.0]) as f32 * strength.y;
            let dx = (x as f32 + warp_x) / radius.x;
            let dy = (y as f32 + warp_y) / radius.y;
            if dx * dx + dy * dy <= 1.0 {
                mask.insert(IVec2::new(x, y));
            }
        }
    }
    // Pixel warping can leave detached specks: keep the largest cardinal blob.
    let mut largest = HashSet::new();
    while let Some(&start) = mask.iter().next() {
        let mut component = HashSet::from([start]);
        let mut frontier = vec![start];
        mask.remove(&start);
        while let Some(p) = frontier.pop() {
            for d in SIDES {
                let next = p + d;
                if mask.remove(&next) {
                    component.insert(next);
                    frontier.push(next);
                }
            }
        }
        if component.len() > largest.len() {
            largest = component;
        }
    }
    // Place the topmost tile at the construction origin.
    let anchor = largest
        .iter()
        .copied()
        .max_by_key(|p| (p.y, -p.x.abs(), p.x))
        .unwrap_or(IVec2::ZERO);
    if largest.is_empty() {
        largest.insert(anchor);
    }
    largest.into_iter().map(|p| origin + p - anchor).collect()
}

#[derive(Component)]
pub struct Bee {
    pub grid: IVec2,
    pub target: Option<IVec2>,
    pub timer: f32,
    pub flower_cooldown: f32,
    pub gather_timer: f32,
    pub inside_hive: Option<usize>,
    pub hive_timer: f32,
    pub flowers_collected: u32,
    pub entrance_cooldown: f32,
    pub hive_target: Option<(usize, IVec2)>,
}

fn spawn_bee(commands: &mut Commands, grid: IVec2) {
    commands.spawn((
        Bee {
            grid,
            target: None,
            timer: 0.0,
            flower_cooldown: 0.0,
            gather_timer: 0.0,
            inside_hive: None,
            hive_timer: 0.0,
            flowers_collected: 0,
            entrance_cooldown: 0.0,
            hive_target: None,
        },
        Sprite::from_color(BEE_COLOR, Vec2::splat(CELLSIZE)),
        Transform::from_xyz(grid.x as f32 * CELLSIZE, grid.y as f32 * CELLSIZE, BEE_Z),
    ));
}

pub(crate) fn spawn_initial_bees(mut commands: Commands, environment: Res<Environment>) {
    let mut rng = rand::rng();
    for _ in 0..INITIAL_BEE_COUNT {
        let grid = IVec2::new(
            rng.random_range(environment.min.x..=environment.max.x),
            rng.random_range(environment.min.y..=environment.max.y),
        );
        spawn_bee(&mut commands, grid);
    }
}

pub(crate) fn spawn_bee_on_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(cursor) = window.cursor_position() else {
        return;
    };

    let Ok(world_position) = camera.0.viewport_to_world_2d(camera.1, cursor) else {
        return;
    };

    let grid = IVec2::new(
        (world_position.x / CELLSIZE).round() as i32,
        (world_position.y / CELLSIZE).round() as i32,
    );

    spawn_bee(&mut commands, grid);
}

#[derive(Resource, Default)]
pub(crate) struct HiveState {
    hives: Vec<Hive>,
    active_hive: Option<usize>,
}

#[derive(Resource)]
pub(crate) struct BeeSpeed(pub f32);

impl Default for BeeSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

struct Hive {
    origin: IVec2,
    phase: i32,
    plan: HashSet<IVec2>,
    built: HashSet<IVec2>,
    black_candidates: Vec<IVec2>,
    black: HashSet<IVec2>,
    planned_black: HashSet<IVec2>,
    entities: HashMap<IVec2, Entity>,
}

impl Hive {
    fn is_dark(&self, p: IVec2) -> bool {
        (p.y - self.origin.y + self.phase).rem_euclid(2) == 1
    }

    fn color(&self, p: IVec2) -> Color {
        if self.black.contains(&p) {
            BLACK
        } else if self.is_dark(p) {
            DARK
        } else {
            GOLD
        }
    }

    fn generate(origin: IVec2, rng: &mut impl RngExt) -> Self {
        loop {
            let shape = warped_oval(IVec2::ZERO, rng);
            // Anchor construction at the bottom, growing upward from the dirt base.
            let base = shape
                .iter()
                .copied()
                .min_by_key(|p| (p.y, p.x.abs(), p.x))
                .unwrap_or(IVec2::ZERO);
            let local_plan: HashSet<_> = shape.into_iter().map(|p| p - base).collect();
            let plan = local_plan.into_iter().map(|p| origin + p).collect();
            let mut hive = Self {
                origin,
                phase: rng.random_range(0..2),
                plan,
                built: HashSet::new(),
                black_candidates: Vec::new(),
                black: HashSet::new(),
                planned_black: HashSet::new(),
                entities: HashMap::new(),
            };
            let mut positions: Vec<_> = hive.plan.iter().copied().collect();
            positions.sort_by_key(|p| (-p.y, p.x));
            for p in positions {
                // Every dark tile gets one independent 5% entrance roll.
                if hive.is_dark(p) && rng.random_bool(0.05) {
                    hive.black_candidates.push(p);
                }
            }
            // Resolve random entrances while enforcing the cardinal safety rule.
            for &p in &hive.black_candidates {
                if black_allowed(p, &hive.plan, &hive.planned_black) {
                    hive.planned_black.insert(p);
                }
            }
            if hive.planned_black.is_empty() {
                // Force an entrance only where a black tile is actually valid.
                let valid: Vec<_> = hive
                    .plan
                    .iter()
                    .copied()
                    .filter(|&p| hive.is_dark(p) && black_allowed(p, &hive.plan, &HashSet::new()))
                    .collect();
                let Some(&forced) = valid.get(rng.random_range(0..valid.len().max(1))) else {
                    // This shape has no enclosed dark tile; regenerate it.
                    continue;
                };
                if valid.is_empty() {
                    continue;
                }
                hive.black_candidates.push(forced);
                hive.planned_black.insert(forced);
            }
            return hive;
        }
    }

    fn grow(&mut self, rng: &mut impl RngExt) -> Option<IVec2> {
        let next = if self.built.is_empty() {
            Some(self.origin)
        } else {
            self.plan
                .iter()
                .copied()
                .filter(|p| !self.built.contains(p))
                .filter(|p| SIDES.iter().any(|d| self.built.contains(&(*p + *d))))
                .map(|p| {
                    let sides = SIDES
                        .iter()
                        .filter(|d| self.built.contains(&(p + **d)))
                        .count();
                    let diagonals = [
                        IVec2::new(-1, -1),
                        IVec2::new(1, -1),
                        IVec2::new(-1, 1),
                        IVec2::new(1, 1),
                    ]
                    .iter()
                    .filter(|d| self.built.contains(&(p + **d)))
                    .count();
                    let dent = (self.built.contains(&(p + IVec2::X))
                        && self.built.contains(&(p - IVec2::X)))
                        || (self.built.contains(&(p + IVec2::Y))
                            && self.built.contains(&(p - IVec2::Y)));
                    // Distance keeps the early blob compact; support fills dents.
                    let distance = (p - self.origin).as_vec2().length();
                    let score =
                        sides as f32 * 5.0 + diagonals as f32 * 2.0 + if dent { 8.0 } else { 0.0 }
                            - distance * 1.3
                            + rng.random_range(0.0..4.0);
                    (p, score)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(p, _)| p)
        };
        if let Some(p) = next {
            self.built.insert(p);
        }
        for &p in &self.black_candidates {
            if self.planned_black.contains(&p)
                && self.built.contains(&p)
                && black_allowed(p, &self.built, &self.black)
            {
                self.black.insert(p);
            }
        }
        next
    }

    fn next_build_target(&self, from: IVec2) -> Option<IVec2> {
        if self.built.is_empty() {
            return Some(self.origin);
        }
        self.plan
            .iter()
            .copied()
            .filter(|p| {
                !self.built.contains(p) && SIDES.iter().any(|d| self.built.contains(&(*p + *d)))
            })
            .min_by_key(|p| {
                let distance = *p - from;
                let support = SIDES
                    .iter()
                    .filter(|d| self.built.contains(&(*p + **d)))
                    .count();
                (
                    distance.x * distance.x + distance.y * distance.y,
                    -(support as i32),
                )
            })
    }
}

fn black_allowed(p: IVec2, material: &HashSet<IVec2>, black: &HashSet<IVec2>) -> bool {
    SIDES
        .iter()
        .all(|d| material.contains(&(p + *d)) && !black.contains(&(p + *d)))
}

pub(crate) fn generate_initial_hives(environment: Res<Environment>, mut state: ResMut<HiveState>) {
    let mut rng = rand::rng();
    let spawn_noise = Perlin::new(rng.random());
    let mut candidates: Vec<_> = environment
        .tiles
        .iter()
        .filter_map(|(&p, tile)| tile.dirt.then_some(p))
        .collect();
    candidates.shuffle(&mut rng);
    let mut reserved = HashSet::new();
    for origin in candidates {
        if reserved.contains(&origin) || !rng.random_bool(spawn_chance(&spawn_noise, origin)) {
            continue;
        }
        let hive = Hive::generate(origin, &mut rng);
        let clear = hive.plan.iter().all(|p| {
            environment.inside(*p)
                && !reserved.contains(p)
                && (p.y != origin.y || environment.get(*p).dirt)
        });
        if clear {
            for &p in &hive.plan {
                reserved.insert(p);
                reserved.extend(SIDES.map(|d| p + d));
            }
            state.hives.push(hive);
        }
    }
    let count = state.hives.len();
    info!("Generated {} hive plans at startup", count);
}

pub(crate) fn bee_speed_controls(keys: Res<ButtonInput<KeyCode>>, mut speed: ResMut<BeeSpeed>) {
    if keys.just_pressed(KeyCode::Digit1) {
        speed.0 = 1.0;
    } else if keys.just_pressed(KeyCode::Digit2) {
        speed.0 = 5.0;
    } else if keys.just_pressed(KeyCode::Digit3) {
        speed.0 = 10.0;
    } else if keys.just_pressed(KeyCode::Digit4) {
        speed.0 = 50.0;
    } else if keys.just_pressed(KeyCode::Digit5) {
        speed.0 = 200.0;
    }
}

/// Bees periodically choose a random flower in range, gather from it, then
/// add one tile to a planned hive. When a built black entrance exists, a bee
/// sometimes enters it and remains inside for a short random interval.
pub(crate) fn bee_foraging_and_building(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<BeeSpeed>,
    environment: Res<Environment>,
    flowers: Query<(&PCell, &PCellRole)>,
    mut bee_queries: ParamSet<(Query<(&mut Bee, &mut Transform)>, Query<&mut Sprite>)>,
    mut state: ResMut<HiveState>,
) {
    let mut rng = rand::rng();
    let flower_positions: Vec<IVec2> = flowers
        .iter()
        .filter_map(|(cell, role)| matches!(*role, PCellRole::Flower(_)).then_some(cell.grid))
        .collect();
    let range_squared = i64::from(BEE_RANGE).pow(2);

    {
        let mut bees = bee_queries.p0();
        for (mut bee, mut transform) in &mut bees {
            let dt = time.delta_secs() * speed.0;

            if state.active_hive.is_none() {
                state.active_hive = state
                    .hives
                    .iter()
                    .enumerate()
                    .filter(|(_, hive)| hive.built.len() < hive.plan.len())
                    .min_by_key(|(_, hive)| {
                        let d = hive.origin - bee.grid;
                        d.x * d.x + d.y * d.y
                    })
                    .map(|(index, _)| index);
            }

            if bee.inside_hive.is_some() {
                bee.hive_timer -= dt;
                transform.translation.z = INSIDE_HIVE_Z;
                if bee.hive_timer <= 0.0 {
                    bee.inside_hive = None;
                    transform.translation.z = BEE_Z;
                } else {
                    continue;
                }
            }

            bee.entrance_cooldown -= dt;
            if bee.entrance_cooldown <= 0.0
                && bee.target.is_none()
                && bee.hive_target.is_none()
                && let Some(index) = state.active_hive
                && let Some(hive) = state.hives.get(index)
                && !hive.black.is_empty()
                && rng.random_bool((dt * 0.35).clamp(0.0, 0.35) as f64)
            {
                let entrances: Vec<_> = hive.black.iter().copied().collect();
                let entrance = entrances[rng.random_range(0..entrances.len())];
                bee.grid = entrance;
                bee.inside_hive = Some(index);
                bee.hive_timer = rng.random_range(1.0..=5.0);
                bee.entrance_cooldown = 2.0;
                transform.translation.x = entrance.x as f32 * CELLSIZE;
                transform.translation.y = entrance.y as f32 * CELLSIZE;
                transform.translation.z = INSIDE_HIVE_Z;
                continue;
            }

            // After gathering, walk to the exact planned tile before building it.
            if let Some((index, target)) = bee.hive_target {
                if bee.grid == target {
                    bee.hive_target = None;
                    let mut finished_origin = None;
                    if let Some(hive) = state.hives.get_mut(index) {
                        if let Some(position) = hive.grow(&mut rng) {
                            let entity = commands
                                .spawn((
                                    Sprite::from_color(hive.color(position), Vec2::splat(CELLSIZE)),
                                    Transform::from_xyz(
                                        position.x as f32 * CELLSIZE,
                                        position.y as f32 * CELLSIZE,
                                        HIVE_Z,
                                    ),
                                ))
                                .id();
                            hive.entities.insert(position, entity);
                        }
                        if !hive.black.is_empty() && rng.random_bool(0.35) {
                            let entrances: Vec<_> = hive.black.iter().copied().collect();
                            let entrance = entrances[rng.random_range(0..entrances.len())];
                            bee.grid = entrance;
                            bee.inside_hive = Some(index);
                            bee.hive_timer = rng.random_range(1.0..=5.0);
                            transform.translation.x = entrance.x as f32 * CELLSIZE;
                            transform.translation.y = entrance.y as f32 * CELLSIZE;
                            transform.translation.z = INSIDE_HIVE_Z;
                        }
                        if hive.built.len() >= hive.plan.len() {
                            finished_origin = Some(hive.origin);
                        }
                    }
                    if let Some(origin) = finished_origin {
                        state.active_hive = state
                            .hives
                            .iter()
                            .enumerate()
                            .filter(|(_, hive)| hive.built.len() < hive.plan.len())
                            .min_by_key(|(_, hive)| {
                                let d = hive.origin - origin;
                                d.x * d.x + d.y * d.y
                            })
                            .map(|(next, _)| next);
                    }
                    continue;
                }
            }

            // Reaching a flower starts a short gathering period.
            if let Some(target) = bee.target {
                if target == bee.grid {
                    if bee.gather_timer <= 0.0 {
                        bee.gather_timer = rng.random_range(0.8..=2.0);
                    }
                    bee.gather_timer -= dt;
                    if bee.gather_timer <= 0.0 {
                        bee.target = None;
                        bee.flower_cooldown = rng.random_range(1.0..=3.0);
                        bee.flowers_collected += 1;
                        if bee.flowers_collected >= 5 {
                            bee.flowers_collected = 0;
                            if let Some(index) = state.active_hive {
                                if let Some(hive) = state.hives.get_mut(index) {
                                    if let Some(position) = hive.next_build_target(bee.grid) {
                                        bee.hive_target = Some((index, position));
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
            }

            bee.flower_cooldown -= dt;
            bee.timer -= dt;
            if bee.flower_cooldown <= 0.0 && (bee.target.is_none() || rng.random_bool(0.08)) {
                let nearby: Vec<_> = flower_positions
                    .iter()
                    .copied()
                    .filter_map(|p| {
                        let d = p - bee.grid;
                        let squared = i64::from(d.x).pow(2) + i64::from(d.y).pow(2);
                        (squared <= range_squared).then_some((p, (squared as f32).sqrt()))
                    })
                    .collect();
                let closest_distance = nearby
                    .iter()
                    .map(|(_, distance)| *distance)
                    .fold(f32::INFINITY, f32::min);
                // Keep the target random, but within 20 tiles of the closest
                // available flower's distance from this bee.
                let choices: Vec<_> = nearby
                    .into_iter()
                    .filter(|(_, distance)| *distance <= closest_distance + 20.0)
                    .map(|(p, _)| p)
                    .collect();
                bee.target = choices
                    .get(rng.random_range(0..choices.len().max(1)))
                    .copied();
                if bee.target.is_some() {
                    bee.flower_cooldown = rng.random_range(1.5..=4.0);
                }
            }

            // Consume all elapsed movement intervals, capped to avoid a runaway
            // loop at very high speed.
            let mut moves = 0;
            while bee.timer <= 0.0 && moves < 64 {
                bee.timer += 0.2;
                let next_target = bee.hive_target.map(|(_, target)| target).or(bee.target);
                let next = match next_target {
                    Some(target) => {
                        let delta = target - bee.grid;
                        if delta.x.abs() >= delta.y.abs() {
                            bee.grid + IVec2::new(delta.x.signum(), 0)
                        } else {
                            bee.grid + IVec2::new(0, delta.y.signum())
                        }
                    }
                    None => {
                        let directions = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
                        bee.grid + directions[rng.random_range(0..directions.len())]
                    }
                };
                if environment.inside(next) {
                    bee.grid = next;
                    transform.translation.x = next.x as f32 * CELLSIZE;
                    transform.translation.y = next.y as f32 * CELLSIZE;
                }
                moves += 1;
            }
        }
    }

    // Keep the complete blueprint visible, but reveal black entrances only
    // after their built-tile constraints are satisfied.
    let mut sprites = bee_queries.p1();
    for hive in &state.hives {
        for (&position, &entity) in &hive.entities {
            if let Ok(mut sprite) = sprites.get_mut(entity) {
                sprite.color = hive.color(position);
            }
        }
    }
}
