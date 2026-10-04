use bevy::prelude::*;
use rand::RngExt;
use rand::rng;
use std::collections::{HashMap, HashSet};

use crate::{CELLSIZE, Environment};

const BRANCE: f64 = 0.15;
const RANCE: f64 = 0.1;
const FLOWER_DELAY: f32 = 5.0;
const FLOWER_GRACE_PERIOD: f32 = 20.0;
const WILT_DURATION: f32 = 6.0;
const CLOUD_KILLING: bool = false;
// Light is roughly 0..1; penalty at one grid step, falling as 1 / distance².
const CROWDING_PENALTY: f32 = 0.08;

const STEM_COLOR: Color = Color::srgb(0.12, 0.42, 0.16);
const LEAF_COLOR: Color = Color::srgb(0.24, 0.72, 0.25);
const ROOT_COLOR: Color = Color::srgb(0.6, 0.33, 0.0);
const FLOWER_CENTER_COLOR: Color = Color::srgb(1.0, 0.85, 0.0);
const WILT_GRAY_COLOR: Color = Color::srgb(0.70, 0.70, 0.65);

fn growth_score(
    position: IVec2,
    origin: IVec2,
    environment: &Environment,
    occupied: &Occupied,
) -> f32 {
    let mut penalty = 0.0;
    for y in -4..=4 {
        for x in -4..=4 {
            let nearby = position + IVec2::new(x, y);
            let distance_squared = x * x + y * y;
            // Do not penalize a leaf for its own cell.
            if distance_squared > 0 && nearby != origin && occupied.positions.contains(&nearby) {
                penalty += CROWDING_PENALTY / distance_squared as f32;
            }
        }
    }
    environment.get(position).light - penalty
}

#[derive(Resource, Default)]
pub struct Occupied {
    pub positions: HashSet<IVec2>,
}

#[derive(Component)]
pub struct Plant;

#[derive(Component)]
pub struct PCell {
    pub grid: IVec2,
    growth: f32,
    leaf_age: f32,
    flower_age: f32,
    wilt: Option<Wilting>,
    target: Option<IVec2>,
    blocker: bool,
}

struct Wilting {
    elapsed_seconds: f32,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct FlowerBase(PCellRole);

impl Wilting {
    fn color(&self, flower: PCellRole, restored: PCellRole) -> Color {
        let progress = (self.elapsed_seconds / WILT_DURATION).clamp(0.0, 1.0);
        let restore_start = 0.7;
        match flower {
            PCellRole::Petal(_) if progress < 0.3 => {
                blend_color(flower.color(), WILT_GRAY_COLOR, progress / 0.3)
            }
            PCellRole::Petal(_) if progress < restore_start => WILT_GRAY_COLOR,
            PCellRole::Petal(_) => blend_color(
                WILT_GRAY_COLOR,
                restored.color(),
                (progress - restore_start) / (1.0 - restore_start),
            ),
            PCellRole::Flower(_) if progress < restore_start => flower.color(),
            PCellRole::Flower(_) => blend_color(
                flower.color(),
                restored.color(),
                (progress - restore_start) / (1.0 - restore_start),
            ),
            _ => restored.color(),
        }
    }
}

fn blend_color(from: Color, to: Color, amount: f32) -> Color {
    let from = from.to_srgba();
    let to = to.to_srgba();
    let amount = amount.clamp(0.0, 1.0);
    Color::srgba(
        from.red + (to.red - from.red) * amount,
        from.green + (to.green - from.green) * amount,
        from.blue + (to.blue - from.blue) * amount,
        from.alpha + (to.alpha - from.alpha) * amount,
    )
}

#[derive(Component, Clone, Copy, PartialEq)]
pub enum PCellRole {
    Stem,
    Leaf,
    Root,
    Flower(Color),
    Petal(Color),
}

impl PCellRole {
    fn color(self) -> Color {
        match self {
            PCellRole::Stem => STEM_COLOR,
            PCellRole::Leaf => LEAF_COLOR,
            PCellRole::Root => ROOT_COLOR,
            PCellRole::Flower(_) => FLOWER_CENTER_COLOR,
            PCellRole::Petal(color) => color,
        }
    }
}

fn pcell_bundle(role: PCellRole, position: Vec2, grid: IVec2, blocker: bool) -> impl Bundle {
    (
        PCell {
            grid,
            growth: 0.0,
            leaf_age: 0.0,
            flower_age: 0.0,
            wilt: None,
            target: None,
            blocker,
        },
        role,
        Sprite::from_color(role.color(), Vec2::splat(CELLSIZE)),
        Transform::from_xyz(position.x, position.y, 0.0),
    )
}

pub fn spawn_pcell(
    parent: &mut ChildSpawnerCommands,
    role: PCellRole,
    position: Vec2,
    grid_pos: IVec2,
    occupied: &mut Occupied,
    blocker: bool,
) {
    occupied.positions.insert(grid_pos);
    parent.spawn(pcell_bundle(role, position, grid_pos, blocker));
}

pub fn grow_cells(
    mut commands: Commands,
    time: Res<Time>,
    environment: Res<Environment>,
    mut cells: Query<(Entity, &mut PCell, &mut PCellRole)>,
    mut occupied: ResMut<Occupied>,
) {
    let mut rng = rng();
    for (entity, mut cell, mut role) in &mut cells {
        cell.growth += time.delta_secs();
        if cell.blocker || matches!(*role, PCellRole::Flower(_) | PCellRole::Petal(_)) {
            continue;
        }

        if cell.growth < 0.1 {
            continue;
        }

        let curr = cell.grid;

        let Some(target) = cell.target else {
            continue;
        };

        if occupied.positions.contains(&target) {
            cell.target = None;
            continue;
        }

        let t_env = environment.get(target);
        let child_role = match *role {
            PCellRole::Root => Some(PCellRole::Leaf),
            PCellRole::Stem => {
                if t_env.dirt {
                    if !t_env.sun {
                        Some(PCellRole::Root)
                    } else {
                        if !rng.random_bool(RANCE) {
                            Some(PCellRole::Root)
                        } else {
                            None
                        }
                    }
                } else {
                    None
                }
            }
            PCellRole::Leaf => Some(PCellRole::Leaf),
            PCellRole::Flower(_) | PCellRole::Petal(_) => None,
        };

        let Some(child_role) = child_role else {
            continue;
        };

        let offset = (target - curr).as_vec2() * CELLSIZE as f32;

        cell.growth = 0.0;
        if child_role == PCellRole::Root {
            occupied.positions.insert(target);
            commands.spawn(pcell_bundle(
                child_role,
                target.as_vec2() * CELLSIZE, // world position
                target,
                false,
            ));
        } else {
            commands.entity(entity).with_children(|parent| {
                spawn_pcell(parent, child_role, offset, target, &mut occupied, false);
            });
        }

        if !rng.random_bool(BRANCE) {
            if *role == PCellRole::Leaf {
                *role = PCellRole::Stem;
            }
        }
    }
}

pub fn update_cells(
    mut cells: Query<(&mut PCell, &mut Sprite, &PCellRole)>,
    mut occupied: ResMut<Occupied>,
    environment: Res<Environment>,
) {
    for (mut cell, mut sprite, role) in &mut cells {
        sprite.color = role.color();
        if cell.blocker || matches!(*role, PCellRole::Flower(_) | PCellRole::Petal(_)) {
            continue;
        }
        let curr = cell.grid;
        occupied.positions.insert(curr);
        // Leaves reconsider moving light and crowding each frame.
        if *role != PCellRole::Leaf && cell.target.is_some() {
            continue;
        }

        let candidates = [
            curr + IVec2::new(0, 1),
            curr + IVec2::new(0, -1),
            curr + IVec2::new(1, 0),
            curr + IVec2::new(-1, 0),
        ]
        .into_iter()
        .filter(|c| environment.inside(*c))
        .collect::<Vec<_>>();

        if *role == PCellRole::Leaf {
            let mut best = None;
            let mut best_score = growth_score(curr, curr, &environment, &occupied);

            for &candidate in &candidates {
                if occupied.positions.contains(&candidate) {
                    continue;
                }

                let score = growth_score(candidate, curr, &environment, &occupied);
                if score > best_score {
                    best_score = score;
                    best = Some(candidate);
                }
            }
            cell.target = best;
        } else {
            for &candidate in &candidates {
                if !occupied.positions.contains(&candidate) {
                    cell.target = Some(candidate);
                    break;
                }
            }
        }
    }
}

pub fn check_suffocation(
    mut commands: Commands,
    mut occupied: ResMut<Occupied>,
    mut cells: Query<(Entity, &PCell, &mut PCellRole)>,
    children_query: Query<&Children>,
    child_of_query: Query<&ChildOf>,
) {
    let mut to_kill = Vec::new();
    for (entity, cell, role) in cells.iter() {
        if *role != PCellRole::Stem {
            continue;
        }
        let Ok(child_of) = child_of_query.get(entity) else {
            continue;
        };
        let curr = cell.grid;
        let neighbors = [
            curr + IVec2::new(0, 1),
            curr + IVec2::new(0, -1),
            curr + IVec2::new(1, 0),
            curr + IVec2::new(-1, 0),
        ];

        if neighbors.iter().all(|pos| occupied.positions.contains(pos)) {
            to_kill.push((entity, curr, child_of.parent()));
        }
    }
    let kill_set: HashSet<Entity> = to_kill.iter().map(|(entity, _, _)| *entity).collect();
    for (entity, grid, parent) in to_kill {
        if child_of_query
            .iter_ancestors(entity)
            .any(|a| kill_set.contains(&a))
        {
            continue;
        }
        let Ok(parent_grid) = cells.get(parent).map(|(_, c, _)| c.grid) else {
            continue;
        };

        occupied.positions.remove(&grid);
        for d in children_query.iter_descendants(entity) {
            if let Ok((_, c, _)) = cells.get(d) {
                occupied.positions.remove(&c.grid);
            }
        }

        commands.entity(entity).despawn();

        if let Ok((_, _, mut p_role)) = cells.get_mut(parent) {
            if *p_role == PCellRole::Stem {
                *p_role = PCellRole::Leaf;
            }
        }

        let offset = (grid - parent_grid).as_vec2() * CELLSIZE;
        commands.entity(parent).with_children(|p| {
            spawn_pcell(p, PCellRole::Leaf, offset, grid, &mut occupied, true);
        });
    }
}

pub fn bloom_cells(
    mut commands: Commands,
    time: Res<Time>,
    environment: Res<Environment>,
    mut occupied: ResMut<Occupied>,
    mut cells: Query<(Entity, &mut PCell, &mut PCellRole, &mut Sprite)>,
) {
    let mut centers = HashMap::new();
    let mut positions = HashMap::new();
    let mut bloomed = false;
    for (entity, mut cell, role, _) in &mut cells {
        positions.insert(cell.grid, entity);
        match *role {
            PCellRole::Leaf => {
                // Only sunlit time counts toward blooming, including blocking leaves.
                if CLOUD_KILLING && !environment.get(cell.grid).sun {
                    continue;
                }
                cell.leaf_age += time.delta_secs();
                if cell.leaf_age >= FLOWER_DELAY {
                    centers.insert(cell.grid, (entity, None));
                    bloomed = true;
                }
            }
            PCellRole::Flower(color) => {
                centers.insert(cell.grid, (entity, Some(color)));
            }
            _ => cell.leaf_age = 0.0,
        }
    }
    if !bloomed {
        return;
    }

    let mut rng = rng();
    let mut starts: Vec<_> = centers
        .iter()
        .filter_map(|(&position, &(_, color))| color.is_none().then_some(position))
        .collect();
    // Stable tie-breaking for flowers that bloom in the same frame.
    starts.sort_by_key(|p| (p.y, p.x));
    for center in starts {
        // Inherit from the nearest overlapping flower, without merging colors.
        // Equal distances prefer the lower y, then lower x coordinate.
        let mut neighbors = Vec::new();
        for y in -2..=2 {
            for x in -2..=2 {
                let neighbor = center + IVec2::new(x, y);
                if let Some(&(_, Some(color))) = centers.get(&neighbor) {
                    neighbors.push(((x * x + y * y, neighbor.y, neighbor.x), color));
                }
            }
        }
        let color = neighbors
            .into_iter()
            .min_by_key(|(key, _)| *key)
            .map(|(_, color)| color)
            .unwrap_or_else(|| Color::hsl(rng.random_range(0.0..360.0), 0.8, 0.6));
        centers.get_mut(&center).unwrap().1 = Some(color);
        let parent = centers[&center].0;
        for y in -1..=1 {
            for x in -1..=1 {
                let position = center + IVec2::new(x, y);
                if !environment.inside(position)
                    || (CLOUD_KILLING && !environment.get(position).sun)
                {
                    continue;
                }
                // Reserve other centers, including leaves blooming this frame.
                if position != center && centers.contains_key(&position) {
                    continue;
                }
                let role = if position == center {
                    PCellRole::Flower(color)
                } else {
                    PCellRole::Petal(color)
                };
                if let Some(&entity) = positions.get(&position) {
                    if let Ok((_, mut cell, mut old_role, mut sprite)) = cells.get_mut(entity) {
                        // First bloom owns the tile: never repaint an existing flower.
                        if matches!(*old_role, PCellRole::Flower(_) | PCellRole::Petal(_)) {
                            continue;
                        }
                        commands.entity(entity).insert(FlowerBase(*old_role));
                        *old_role = role;
                        cell.target = None;
                        cell.flower_age = 0.0;
                        cell.wilt = None;
                        sprite.color = role.color();
                    }
                } else if occupied.positions.insert(position) {
                    // Empty petal tiles belong to their center, so branch
                    // removal also removes these petals and their occupancy.
                    commands.entity(parent).with_children(|children| {
                        children
                            .spawn(pcell_bundle(
                                role,
                                (position - center).as_vec2() * CELLSIZE,
                                position,
                                false,
                            ))
                            // A petal grown into empty space leaves a leaf behind.
                            .insert(FlowerBase(PCellRole::Leaf));
                    });
                }
            }
        }
    }
}

pub fn wilt_cells(
    mut commands: Commands,
    time: Res<Time>,
    environment: Res<Environment>,
    mut cells: Query<(
        Entity,
        &mut PCell,
        &mut PCellRole,
        &mut Sprite,
        Option<&FlowerBase>,
    )>,
) {
    if !CLOUD_KILLING {
        return;
    }

    let dt = time.delta_secs();
    for (entity, mut cell, mut role, mut sprite, base) in &mut cells {
        if !matches!(*role, PCellRole::Flower(_) | PCellRole::Petal(_)) {
            continue;
        }
        let previous_age = cell.flower_age;
        cell.flower_age += dt;
        if let Some(wilt) = cell.wilt.as_mut() {
            wilt.elapsed_seconds += dt;
        } else if !environment.get(cell.grid).sun {
            let eligible_seconds = (cell.flower_age - FLOWER_GRACE_PERIOD).max(0.0)
                - (previous_age - FLOWER_GRACE_PERIOD).max(0.0);
            // Shade starts wilting immediately after the grace period.
            // Carry over this frame's eligible time for consistent timing.
            if eligible_seconds > 0.0 {
                cell.wilt = Some(Wilting {
                    elapsed_seconds: eligible_seconds,
                });
            }
        }
        if let Some(wilt) = &cell.wilt {
            let restored = base.map_or(PCellRole::Leaf, |base| base.0);
            sprite.color = wilt.color(*role, restored);
            if wilt.elapsed_seconds >= WILT_DURATION {
                *role = restored;
                sprite.color = restored.color();
                cell.flower_age = 0.0;
                cell.leaf_age = 0.0;
                cell.wilt = None;
                cell.target = None;
                commands.entity(entity).remove::<FlowerBase>();
            }
        }
    }
}

// fn get_grid_pos(position: Vec2) -> IVec2 {
//     IVec2::new(
//         (position.x / CELLSIZE as f32).round() as i32,
//         (position.y / CELLSIZE as f32).round() as i32
//     )
// }
