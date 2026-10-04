mod cells;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::{HashMap};
use noise::{NoiseFn, OpenSimplex};
use cells::*;
use rand::RngExt;
use rand::rng;

const CELLSIZE: f32 = 5.0;
const SLANT: f64 = 0.35;
const WIND_SPEED: f64 = 0.005;
const SWAY: f64 = 0.03;
const NSCALE: f64 = 0.005;


#[derive(Resource)]
struct LightNoise(OpenSimplex);
fn light_value(n: &OpenSimplex, x: i32, y: i32, t: f64) -> f64 {
    let wx = x as f64 * CELLSIZE as f64 * NSCALE + 500.37;
    let wy = y as f64 * CELLSIZE as f64 * NSCALE + 500.91;
    let sx = wx
        + wy * SLANT
        + t * WIND_SPEED
        + (t * 0.7 + wy * 3.0).sin() * SWAY;
    n.get([sx, wy])
}
fn shade(l: f64) -> f32 {
    let s = ((0.1 - l) / 0.2).clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    let light01 = (l + 1.0) / 2.0;
    (s * (1.0 - 0.5 * light01)) as f32
}

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
struct LightTile {grid: IVec2}

#[derive(Component)]
#[require(Sprite = default_sprite(), Transform = origin_transform())]
pub struct Organism;



fn default_sprite() -> Sprite {
    Sprite::from_color(Color::srgb(0.2, 0.7, 1.0), Vec2::splat(100.0))
}

fn origin_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, 0.0)
}

fn animate_light(
    time: Res<Time>,
    noise: Res<LightNoise>,
    mut env: ResMut<Environment>,
    mut tiles: Query<(&LightTile, &mut Sprite)>,
) {
    let t = time.elapsed_secs_f64();
    let env = &mut *env;
    for (tile, mut sprite) in &mut tiles {
        let l = light_value(&noise.0, tile.grid.x, tile.grid.y, t);
        if l > 0.4 {
            sprite.color = Color::srgba(1.0,1.0,0.0, 0.01*l as f32);
        } else {
            sprite.color = Color::srgba(0.0, 0.0, 0.0, shade(l));
        }
        if let Some(e) = env.tiles.get_mut(&tile.grid) {
            e.light = ((l + 1.0)/2.0) as f32;
            e.sun = l > 0.0
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // .insert_resource(Environment {sun: true, dirt: false})
        .insert_resource(Occupied::default())
        .add_systems(Update, (animate_light))
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


    let lnoise = OpenSimplex::new(21345);
    let dperlin = OpenSimplex::new(68292);

    for x in env.min.x..=env.max.x {
        for y in env.min.y..=env.max.y {
            let grid_pos = IVec2::new(x,y);
            let pos = Vec3::new(x as f32 * CELLSIZE, y as f32 * CELLSIZE,0.0);
            
            let wx = x as f64 * CELLSIZE as f64 * NSCALE;
            let wy = y as f64 * CELLSIZE as f64 * NSCALE;

            let lval = light_value(&lnoise,x,y,0.0);

            let light = ((lval + 1.0)/2.0) as f32;
            let sun = lval > 0.0;

            let dval = dperlin.get([
                wx, wy
            ]);
            let dirt = dval > 0.2;

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
                        Color::srgba(0.35,0.2,0.08,1.0-0.1*(dval as f32 +1.0)/2.0),
                        Vec2::splat(CELLSIZE)
                    ),
                    Transform::from_translation(
                        pos-Vec3::Z
                    )
                ));
            }

            commands.spawn((
                LightTile {grid: grid_pos},
                Sprite::from_color(
                    Color::srgba(0.0,0.0,0.0,shade(lval)),
                    Vec2::splat(CELLSIZE),
                ),
                Transform::from_translation(
                    pos + Vec3::Z
                )
            ));
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
    commands.insert_resource(LightNoise(lnoise));
    commands.insert_resource(env);
}
