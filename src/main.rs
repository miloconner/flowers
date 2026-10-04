mod cells;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::{HashMap};
use noise::{NoiseFn, OpenSimplex};
use cells::*;
use rand::RngExt;
use rand::rng;

const CELLSIZE: f32 = 5.0;

#[derive(Clone,Copy)]
struct TileEnvironment {
    light: f32,
    sun: bool,
    dirt: bool
}

#[derive(Resource, Default)]
struct Environment {
    tiles: HashMap<IVec2, TileEnvironment>,
    min: IVec2,
    max: IVec2
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
struct LightTile;

#[derive(Component)]
#[require(Sprite = default_sprite(), Transform = origin_transform())]
pub struct Organism;



fn default_sprite() -> Sprite {
    Sprite::from_color(Color::srgb(0.2, 0.7, 1.0), Vec2::splat(100.0))
}

fn origin_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, 0.0)
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // .insert_resource(Environment {sun: true, dirt: false})
        .insert_resource(Occupied::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (update_cells,grow_cells,check_suffocation).chain())
        .run();
}

fn setup(mut commands: Commands, mut occupied: ResMut<Occupied>, window: Single<&Window, With<PrimaryWindow>>) {
    commands.spawn(Camera2d);
    // commands.spawn(Organism);

    let hx = (window.width() / CELLSIZE / 2.0).ceil() as i32;
    let hy = (window.height() / CELLSIZE / 2.0).ceil() as i32;

    let mut env = Environment{
        tiles: HashMap::new(),
        min: IVec2::new(-hx,-hy),
        max: IVec2::new(hx,hy)
    };

    const NSCALE: f64 = 0.005;

    let sperlin = OpenSimplex::new(21345);
    let dperlin = OpenSimplex::new(68292);

    for x in env.min.x..=env.max.x {
        for y in env.min.y..=env.max.y {
            let grid_pos = IVec2::new(x,y);
            let pos = Vec3::new(x as f32 * CELLSIZE, y as f32 * CELLSIZE,0.0);
            
            let wx = x as f64 * CELLSIZE as f64 * NSCALE;
            let wy = y as f64 * CELLSIZE as f64 * NSCALE;

            let lval = sperlin.get([
                wx,
                wy,
            ]);

            let light = ((lval + 1.0)/2.0) as f32;
            let sun = lval > 0.0;

            let dirt = dperlin.get([
                wx, wy
            ]) < 0.0;

            env.tiles.insert(
                grid_pos,
                TileEnvironment {
                    light, sun, dirt
                }
            );

            if dirt {
                commands.spawn((
                    DirtTile,
                    Sprite::from_color(
                        Color::srgb(0.35,0.2,0.08),
                        Vec2::splat(CELLSIZE)
                    ),
                    Transform::from_translation(
                        pos-Vec3::Z
                    )
                ));
            }

            if !sun {
                commands.spawn((
                    LightTile,
                    Sprite::from_color(
                        Color::srgba(0.0,0.0,0.0,1.0-0.5*light),
                        Vec2::splat(CELLSIZE)
                    ),
                    Transform::from_translation(
                        pos + Vec3::Z
                    )
                ));
            }
        }
    }

    let mut rng = rng();
    let mut roots_spawned = 0;

    while roots_spawned < 10 {
        let root_pos = IVec2::new(
            rng.random_range(env.min.x..=env.max.x),
            rng.random_range(env.min.y..=env.max.y)
        );

        if !env.get(root_pos).dirt || occupied.positions.contains(&root_pos) {
            continue;
        }

        let world_pos = Vec3::new(
            root_pos.x as f32 * CELLSIZE,
            root_pos.y as f32 * CELLSIZE,
            0.0,
        );

        commands
            .spawn((
                Plant,
                Transform::from_translation(world_pos),
                Visibility::default()
            ))
            .with_children(|plant| {
                spawn_pcell(
                    plant,
                    PCellRole::Root,
                    Vec2::ZERO,
                    root_pos,
                    &mut occupied,
                    false,
                );
            });

        roots_spawned += 1;
    }

    commands.insert_resource(env);
}
