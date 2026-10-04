use bevy::prelude::*;
use rand::RngExt;
use rand::rng;
use std::collections::{HashMap, HashSet};

use crate::{
    CELLSIZE, Environment,
    cells::{FlowerBase, PCell, PCellRole, Pollinated, revert_flower},
    hives::BeeSpeed,
};

const RABBIT_COLOR: Color = Color::srgb(0.85, 0.8, 0.75);
const RABBIT_Z: f32 = 1.5;
const INITIAL_RABBITS: usize = 4;
const MAX_RABBITS: usize = 10;
const RABBIT_RANGE: i32 = 80;
const STEP_TIME: f32 = 1.0; // seconds per hop
const EAT_TIME: f32 = 2.0;
const STARVE_TIME: f32 = 150.0; // seconds before death
const MATE_INTERVAL: f32 = 40.0; // seconds between mating
const MATE_RANGE: i32 = 6; // tiles
const FED_LIMIT: f32 = STARVE_TIME * 0.5; // minim food to mate

#[derive(Component)]
pub struct Rabbit {
    grid: IVec2, // center tile of the 3x3 body
    target: Option<IVec2>,
    step_timer: f32,
    eating: Option<f32>, // time left on the current meal
    hunger: f32,
    mate_timer: f32,
}

pub(crate) fn spawn_rabbit(commands: &mut Commands, grid: IVec2) {
    commands.spawn((
        Rabbit {
            grid,
            target: None,
            step_timer: 0.0,
            eating: None,
            hunger: 0.0,
            mate_timer: 0.0,
        },
        Sprite::from_color(RABBIT_COLOR, Vec2::splat(CELLSIZE * 3.0)),
        Transform::from_xyz(
            grid.x as f32 * CELLSIZE,
            grid.y as f32 * CELLSIZE,
            RABBIT_Z,
        ),
    )).with_children(|r| {
        for x in [-1.0,1.0] {
            r.spawn((
                Sprite::from_color(Color::BLACK, Vec2::splat(CELLSIZE)),
                Transform::from_xyz(x*CELLSIZE, CELLSIZE, 0.1)
            ));
        }
    });
}

fn stand_tile(environment: &Environment, target: IVec2) -> IVec2 {
    target.clamp(environment.min + IVec2::ONE, environment.max - IVec2::ONE)
}

pub fn spawn_initial_rabbits(mut commands: Commands, environment: Res<Environment>) {
    let mut rng = rng();
    for _ in 0..INITIAL_RABBITS {
        let grid = IVec2::new(
            rng.random_range(environment.min.x + 1..=environment.max.x - 1),
            rng.random_range(environment.min.y + 1..=environment.max.y - 1),
        );
        spawn_rabbit(&mut commands, grid);
    }
}

pub fn rabbit_ai(
    mut commands: Commands,
    time: Res<Time>,
    speed: Res<BeeSpeed>,
    environment: Res<Environment>,
    mut rabbits: Query<(Entity, &mut Rabbit, &mut Transform)>,
    mut cells: Query<(Entity, &mut PCell, &mut PCellRole, Option<&FlowerBase>, Has<Pollinated>)>,
) {
    let dt = time.delta_secs() * speed.0;
    let mut rng = rng();

    // finds white flowers
    let white: HashMap<IVec2, Color> = cells
        .iter()
        .filter_map(|(_, cell, role, _, pollinated)| match *role {
            PCellRole::Flower(color) if pollinated => Some((cell.grid, color)),
            _ => None,
        })
        .collect();
    let mut eaten: Vec<(IVec2, Color)> = Vec::new();

    let mut taken: HashSet<IVec2> = 
        rabbits.iter().filter_map(|(_,r,_)| r.target).collect();

    for (entity, mut rabbit, mut transform) in &mut rabbits {
        rabbit.hunger += dt;
        rabbit.mate_timer += dt;
        if rabbit.hunger >= STARVE_TIME {
            commands.entity(entity).despawn();
            continue;
        }

        if let Some(t) = rabbit.target
            && (!white.contains_key(&t) || eaten.iter().any(|(p, _)| *p == t))
        {
            taken.remove(&t);
            rabbit.target = None;
            rabbit.eating = None;
        }

        // eat flower
        if let Some(t) = rabbit.target && stand_tile(&environment, t) == rabbit.grid
        {
            let left = rabbit.eating.unwrap_or(EAT_TIME) - dt;
            if left <= 0.0 {
                eaten.push((t, white[&t]));
                rabbit.target = None;
                rabbit.eating = None;
                rabbit.hunger = 0.0;
            } else {
                rabbit.eating = Some(left);
            }
            continue;
        }

        // find white mid flower
        if rabbit.target.is_none() {
            let from = rabbit.grid;
            rabbit.target = white
                .keys()
                .copied()
                .filter(|p| {
                    !taken.contains(p) && (*p - from).length_squared() <= RABBIT_RANGE * RABBIT_RANGE
                })
                .min_by_key(|p| (*p - from).length_squared());
            if let Some(t) = rabbit.target {
                taken.insert(t);
            }
        }

        // wander or go to target
        rabbit.step_timer -= dt;
        for _ in 0..64 {
            if rabbit.step_timer > 0.0 {
                break;
            }
            rabbit.step_timer += STEP_TIME;
            let next = match rabbit.target {
                Some(target) => {
                    let delta = stand_tile(&environment, target) - rabbit.grid;
                    if delta.x.abs() >= delta.y.abs() {
                        rabbit.grid + IVec2::new(delta.x.signum(), 0)
                    } else {
                        rabbit.grid + IVec2::new(0, delta.y.signum())
                    }
                }
                None => {
                    let dirs = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
                    rabbit.grid + dirs[rng.random_range(0..dirs.len())]
                }
            };
            // stays in map
            if environment.inside(next - IVec2::ONE) && environment.inside(next + IVec2::ONE) {
                rabbit.grid = next;
                transform.translation.x = next.x as f32 * CELLSIZE;
                transform.translation.y = next.y as f32 * CELLSIZE;
            }
        }
    }

    // removes a flower on eat
    for (center, color) in eaten {
        for (entity, mut cell, mut role, base, _) in &mut cells {
            let d = cell.grid - center;
            if d.x.abs() > 1 || d.y.abs() > 1 {
                continue;
            }
            let part_of_flower = match *role {
                PCellRole::Flower(_) => d == IVec2::ZERO,
                PCellRole::Petal(c) => d != IVec2::ZERO && c == color,
                _ => false,
            };
            if part_of_flower {
                revert_flower(&mut commands, entity, &mut cell, &mut role, base);
            }
        }
    }
}

pub fn rabbit_mating(mut commands: Commands, mut rabbits: Query<(Entity, &mut Rabbit)>) {
    let population = rabbits.iter().count();
    if population >= MAX_RABBITS {
        return;
    }
    let ready: Vec<(Entity, IVec2)> = rabbits
        .iter()
        .filter(|(_, r)| r.mate_timer >= MATE_INTERVAL && r.hunger < FED_LIMIT)
        .map(|(e, r)| (e, r.grid))
        .collect();

    let mut used = HashSet::new();
    let mut born = 0;
    for i in 0..ready.len() {
        let (a, pos_a) = ready[i];
        if used.contains(&a) {
            continue;
        }
        for &(b, pos_b) in &ready[i + 1..] {
            if used.contains(&b) || population + born >= MAX_RABBITS {
                continue;
            }
            if (pos_a - pos_b).abs().max_element() <= MATE_RANGE {
                used.insert(a);
                used.insert(b);
                born += 1;
                spawn_rabbit(&mut commands, pos_a);
                break;
            }
        }
    }
    for (entity, mut rabbit) in &mut rabbits {
        if used.contains(&entity) {
            rabbit.mate_timer = 0.0;
        }
    }
}