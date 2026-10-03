use bevy::prelude::*;
use std::collections::HashSet;
const CELLSIZE: f32 = 20.0;

#[derive(Resource)]
struct Environment {sun: bool, dirt:bool}

#[derive(Resource, Default)]
struct Occupied {positions: HashSet<IVec2>}

#[derive(Component)]
struct Plant;

#[derive(Component)]
#[require(Sprite = default_sprite(), Transform = origin_transform())]
struct Organism;

fn default_sprite() -> Sprite {
    Sprite::from_color(Color::srgb(0.2, 0.7, 1.0), Vec2::splat(100.0))
}

fn origin_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, 0.0)
}

#[derive(Component)]
struct PCell {
    growth: f32,
    target: Option<IVec2>
}

#[derive(Component, Clone, Copy, PartialEq)]
enum PCellRole {
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

fn spawn_pcell(parent: &mut ChildSpawnerCommands,role: PCellRole,position: Vec2) {

    parent.spawn((
        PCell { growth: 0.0, target: None },
        role,
        Sprite::from_color(role.color(), Vec2::new(CELLSIZE,CELLSIZE)),
        Transform::from_xyz(position.x, position.y, 0.0),
    ));
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(Environment {sun: true, dirt: false})
        .insert_resource(Occupied::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (update_cells,grow_cells).chain())
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn(Organism);

    commands
        .spawn((Plant, Transform::from_xyz(-240.0, -80.0, 0.0)))
        .with_children(|plant| {
            spawn_pcell(plant, PCellRole::Root,Vec2::ZERO);
        });
}

fn grow_cells(
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

        let new_role = match *role {
            PCellRole::Leaf => 
            if cell.growth >= 4.0 {
                if environment.sun {
                    println!("Leaf to stem");
                    Some(PCellRole::Stem)
                } else {
                    None
                }
            } else {
                Some(*role)
            }
            _ => Some(*role)
        };

        if let Some(new_role) = new_role {
            if *role != new_role {
                *role = new_role;
                cell.growth = 0.0;
                continue;
            }
        } else {
            commands.entity(entity).despawn();
            continue;
        }

        let child_role = match *role {
            PCellRole::Root => Some(PCellRole::Leaf),
            PCellRole::Stem => if environment.sun {
                Some(PCellRole::Leaf)
            } else if environment.dirt {
                Some(PCellRole::Root)
            } else {
                None
            }
            PCellRole::Leaf => None
        };

        let Some(child_role) = child_role else {
            continue;
        };
        let Some(target) = cell.target else {
            continue;
        };

        if occupied.positions.contains(&target) {
            cell.target = None;
            continue;
        }

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
    }
}

fn update_cells(
    mut cells: Query<
        (&mut PCell, &mut Sprite, &PCellRole, &GlobalTransform)
    >,
    mut occupied: ResMut<Occupied>,
)   {
    for (mut cell, mut sprite, role, transform) in &mut cells {
        sprite.color = role.color();
        let curr = get_grid_pos(transform);
        occupied.positions.insert(curr);
        if cell.target.is_some() {
            continue;
        }

        let candidates = [
            curr+IVec2::new(0,1),
            curr+IVec2::new(0,-1),
            curr+IVec2::new(1,0),
            curr+IVec2::new(-1,0),];
        
        for candidate in candidates {
            if !occupied.positions.contains(&candidate) {
                println!("Found target");
                cell.target = Some(candidate);
                break;
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