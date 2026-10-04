use bevy::prelude::*;
use std::collections::HashSet;

use crate::{Environment, CELLSIZE};

#[derive(Resource, Default)]
pub struct Occupied {pub positions: HashSet<IVec2>}

#[derive(Component)]
pub struct Plant;

#[derive(Component)]
pub struct PCell {
    pub grid: IVec2,
    growth: f32,
    target: Option<IVec2>,
    blocker: bool
}

#[derive(Component, Clone, Copy, PartialEq)]
pub enum PCellRole {
    Stem,
    Leaf,
    Root
}

impl PCellRole {
    fn color(self) -> Color {
        match self {
            PCellRole::Stem => Color::srgb(0.12, 0.42, 0.16),
            PCellRole::Leaf => Color::srgb(0.24, 0.72, 0.25),
            PCellRole::Root => Color::srgb(0.6, 0.33, 0.0)
        }
    }
}

pub fn spawn_pcell(
    parent: &mut ChildSpawnerCommands,
    role: PCellRole,
    position: Vec2, 
    grid_pos: IVec2,
    occupied: &mut Occupied, 
    blocker: bool
) {
    occupied.positions.insert(grid_pos);
    parent.spawn((
        PCell { grid: grid_pos, growth: 0.0, target: None, blocker },
        role,
        Sprite::from_color(role.color(), Vec2::new(CELLSIZE,CELLSIZE)),
        Transform::from_xyz(position.x, position.y, 0.0),
    ));
}

pub fn grow_cells(
    mut commands: Commands,
    time: Res<Time>,
    environment: Res<Environment>,
    mut cells: Query<(Entity, &mut PCell, &mut PCellRole)>,
    mut occupied: ResMut<Occupied>
) {
    for (entity, mut cell, mut role) in &mut cells {
        cell.growth += time.delta_secs();
        if cell.blocker {
            continue;
        }

        if cell.growth < 2.0 {continue;}

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
            PCellRole::Stem => if !t_env.sun && t_env.dirt {
                Some(PCellRole::Root)
            } else {
                None
            }
            PCellRole::Leaf => Some(PCellRole::Leaf)
        };

        let Some(child_role) = child_role else {
            continue;
        };

        let offset = (target - curr).as_vec2() * CELLSIZE as f32;

        cell.growth = 0.0;
        commands.entity(entity).with_children(|parent| {
            spawn_pcell(
                parent,
                child_role,
                offset,
                target,
                &mut occupied,
                false
            );
        });

        if *role == PCellRole::Leaf {
            *role = PCellRole::Stem;
        }
    }
}

pub fn update_cells(
    mut cells: Query<
        (&mut PCell, &mut Sprite, &PCellRole)
    >,
    mut occupied: ResMut<Occupied>,
    environment: Res<Environment>
)   {
    for (mut cell, mut sprite, role) in &mut cells {
        
        sprite.color = role.color();
        if cell.blocker {continue;}
        let curr = cell.grid;
        let t_env = environment.get(curr);
        occupied.positions.insert(curr);
        if cell.target.is_some() {
            continue;
        }

        let candidates = [
            curr+IVec2::new(0,1),
            curr+IVec2::new(0,-1),
            curr+IVec2::new(1,0),
            curr+IVec2::new(-1,0),]
            .into_iter().filter(|c| environment.inside(*c)).collect::<Vec<_>>();
        
        if *role == PCellRole::Leaf {
            let mut best = None;
            let mut blight = t_env.light;

            for &candidate in &candidates {
                if occupied.positions.contains(&candidate) {
                    continue;
                }

                let c_env = environment.get(candidate);

                if c_env.light > blight {
                    blight = c_env.light;
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
    mut cells: Query<(Entity, &PCell, &mut PCellRole, &ChildOf)>,
    children_query: Query<&Children>,
    child_of_query: Query<&ChildOf>,
) {
    let mut to_kill = Vec::new();
    for (entity, cell, role, child_of) in cells.iter() {
        if *role != PCellRole::Stem {
            continue;
        }
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
        if child_of_query.iter_ancestors(entity).any(|a| kill_set.contains(&a)) {
            continue;
        }
        let Ok(parent_grid) = cells.get(parent).map(|(_, c, _, _)| c.grid) else {
            continue;
        };

        occupied.positions.remove(&grid);
        for d in children_query.iter_descendants(entity) {
            if let Ok((_, c, _, _)) = cells.get(d) {
                occupied.positions.remove(&c.grid);
            }
        }

        commands.entity(entity).despawn();

        if let Ok((_, _, mut p_role, _)) = cells.get_mut(parent) {
            if *p_role != PCellRole::Root {
                *p_role = PCellRole::Leaf;
            }
        }

        let offset = (grid - parent_grid).as_vec2() * CELLSIZE;
        commands.entity(parent).with_children(|p| {
            spawn_pcell(p, PCellRole::Leaf, offset, grid, &mut occupied, true);
        });
    }
}

// fn get_grid_pos(position: Vec2) -> IVec2 {
//     IVec2::new(
//         (position.x / CELLSIZE as f32).round() as i32,
//         (position.y / CELLSIZE as f32).round() as i32
//     )
// }
