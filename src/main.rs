mod cells;

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use noise::{NoiseFn, Perlin};
use cells::*;

const CELLSIZE: f32 = 20.0;

#[derive(Clone,Copy)]
struct TileEnvironment {
    light: f32,
    sun: bool,
    dirt: bool
}

#[derive(Resource, Default)]
struct Environment {tiles: HashMap<IVec2, TileEnvironment>}
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
        .add_systems(Update, (update_cells,grow_cells).chain())
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn(Organism);

    let mut env = Environment::default();

    let sperlin = Perlin::new(1);
    let dperlin = Perlin::new(2);

    for x in -20..20 {
        for y in -20..20 {
            let grid_pos = IVec2::new(x,y);
            let pos = Vec3::new(x as f32 * CELLSIZE, y as f32 * CELLSIZE,0.0);
            
            let lval = sperlin.get([
                x as f64 * 0.1,
                y as f64 * 0.1,
            ]);

            let light = ((lval + 1.0)/2.0) as f32;
            let sun = lval > 0.0;

            let dirt = dperlin.get([
                x as f64 * 0.1,
                y as f64 * 0.1,
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
    commands.insert_resource(env);

    commands
        .spawn((Plant, Transform::from_xyz(-240.0, -80.0, 0.0)))
        .with_children(|plant| {
            spawn_pcell(plant, PCellRole::Root,Vec2::ZERO);
        });
}
