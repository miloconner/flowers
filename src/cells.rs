use bevy::prelude::*;
use std::collections::HashSet;

use crate::{Environment, CELLSIZE};

#[derive(Resource, Default)]
pub struct Occupied {positions: HashSet<IVec2>}

#[derive(Component)]
pub struct Plant;

#[derive(Component)]
pub struct PCell {
    growth: f32,
    target: Option<IVec2>
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

pub fn spawn_pcell(parent: &mut ChildSpawnerCommands,role: PCellRole,position: Vec2) {

    parent.spawn((
        PCell { growth: 0.0, target: None },
        role,
        Sprite::from_color(role.color(), Vec2::new(CELLSIZE,CELLSIZE)),
        Transform::from_xyz(position.x, position.y, 0.0),
    ));
}


pub fn grow_cells(
    mut commands: Commands,
    time: Res<Time>,
    environment: Res<Environment>,
    mut cells: Query<(Entity, &mut PCell, &mut PCellRole, &GlobalTransform)>,
    mut occupied: ResMut<Occupied>
) {
    for (entity, mut cell, mut role, transform) in &mut cells {
        cell.growth += time.delta_secs();

        if cell.growth < 2.0 {continue;}

        let curr = get_grid_pos(transform);
        let mut t_env = environment.get(curr);

        // let new_role = match *role {
        //     PCellRole::Leaf => 
        //     if cell.growth >= 4.0 {
        //         if t_env.sun {
        //             println!("Leaf to stem");
        //             Some(PCellRole::Stem)
        //         } else {
        //             None
        //         }
        //     } else {
        //         Some(*role)
        //     }
        //     _ => Some(*role)
        // };

        // if let Some(new_role) = new_role {
        //     if *role != new_role {
        //         *role = new_role;
        //         cell.growth = 0.0;
        //         continue;
        //     }
        // } else {
        //     occupied.positions.remove(&curr);
        //     commands.entity(entity).despawn();
        //     continue;
        // }

        let Some(target) = cell.target else {
            continue;
        };

        if occupied.positions.contains(&target) {
            cell.target = None;
            continue;
        }

        t_env = environment.get(target);
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

        occupied.positions.insert(target);
        cell.growth = 0.0;
        commands.entity(entity).with_children(|parent| {
            spawn_pcell(
                parent,
                child_role,
                offset
            );
        });

        if *role == PCellRole::Leaf {
            *role = PCellRole::Stem;
        }
    }
}

pub fn update_cells(
    mut cells: Query<
        (&mut PCell, &mut Sprite, &PCellRole, &GlobalTransform)
    >,
    mut occupied: ResMut<Occupied>,
    environment: Res<Environment>
)   {
    for (mut cell, mut sprite, role, transform) in &mut cells {
        
        sprite.color = role.color();
        let curr = get_grid_pos(transform);
        let t_env = environment.get(curr);
        occupied.positions.insert(curr);
        if cell.target.is_some() {
            continue;
        }

        let candidates = [
            curr+IVec2::new(0,1),
            curr+IVec2::new(0,-1),
            curr+IVec2::new(1,0),
            curr+IVec2::new(-1,0),];
        
        if *role == PCellRole::Leaf {
            let mut best = None;
            let mut blight = t_env.light;

            for candidate in candidates {
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
        
            for candidate in candidates {
                if !occupied.positions.contains(&candidate) {
                    println!("Found target");
                    cell.target = Some(candidate);
                    break;
                }
            }
        }
    }
}

fn get_grid_pos(transform: &GlobalTransform) -> IVec2 {
    let pos = transform.translation();
    IVec2::new(
        (pos.x/CELLSIZE as f32).round() as i32,
        (pos.y/CELLSIZE as f32).round() as i32
    )
}