//! Nighttime lightning and fire that consumes connected plant cells.
use bevy::prelude::*;
use noise::{NoiseFn, OpenSimplex};
use rand::{RngExt, seq::IteratorRandom};
use std::collections::{HashMap, HashSet};

use crate::cells::{FlowerBase, Occupied, PCell, PCellRole, Pollinated};
use crate::lighting::{DayNightCycle, TileLight};
use crate::{CELLSIZE, Environment};

const LIGHTNING_CHANCE_PER_NIGHT: f64 = 1.0;
const FLASH_SECONDS: f32 = 0.35;
const FIRE_RADIUS: f32 = 6.0;
const FIRE_SPREAD_INTERVAL: f32 = 0.175;
const NEIGHBORS: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

#[derive(Resource, Default)]
pub(crate) struct LightningStorm {
    was_night: bool,
    strike_in: Option<f32>,
}

#[derive(Resource)]
pub(crate) struct FireNoise(OpenSimplex);

impl Default for FireNoise {
    fn default() -> Self {
        Self(OpenSimplex::new(rand::rng().random()))
    }
}

#[derive(Component)]
pub(crate) struct Burning {
    age: f32,
    duration: f32,
    spread_in: f32,
    visual: Entity,
}

#[derive(Component)]
pub(crate) struct Flame;

// Preserve transforms supporting living descendants until those descendants go.
#[derive(Component)]
pub(crate) struct BurntBranch;

#[derive(Component)]
pub(crate) struct LightningFlash {
    remaining: f32,
    alpha: f32,
}

fn ignite(commands: &mut Commands, entity: Entity, grid: IVec2, role: PCellRole) {
    let mut rng = rand::rng();
    let visual = commands
        .spawn((
            Flame,
            Sprite::from_color(Color::srgb(1.0, 0.6, 0.0), Vec2::splat(CELLSIZE)),
            Transform::from_xyz(grid.x as f32 * CELLSIZE, grid.y as f32 * CELLSIZE, 4.2),
            TileLight {
                grid,
                radius: FIRE_RADIUS,
                intensity: 0.5,
            },
        ))
        .id();
    commands.entity(entity).insert(Burning {
        age: 0.0,
        duration: if matches!(role, PCellRole::Flower(_) | PCellRole::Petal(_)) {
            rng.random_range(0.75..1.25)
        } else {
            rng.random_range(1.5..2.5)
        },
        spread_in: rng.random_range(0.1..0.25),
        visual,
    });
}

pub(crate) fn night_lightning(
    mut commands: Commands,
    time: Res<Time>,
    cycle: Res<DayNightCycle>,
    environment: Res<Environment>,
    mut storm: ResMut<LightningStorm>,
    plants: Query<(Entity, &PCell, &PCellRole), Without<Burning>>,
) {
    let night = cycle.daylight() < 0.15;
    let mut rng = rand::rng();
    if !night {
        storm.was_night = false;
        storm.strike_in = None;
        return;
    }
    if !storm.was_night {
        storm.was_night = true;
        storm.strike_in = rng
            .random_bool(LIGHTNING_CHANCE_PER_NIGHT)
            .then(|| rng.random_range(1.0..8.0));
        return;
    }
    let Some(remaining) = storm.strike_in.as_mut() else {
        return;
    };
    *remaining -= time.delta_secs();
    if *remaining > 0.0 {
        return;
    }
    storm.strike_in = None;
    let Some((entity, cell, role)) = plants.iter().choose(&mut rng) else {
        return;
    };
    ignite(&mut commands, entity, cell.grid, *role);

    let size = (environment.max - environment.min + IVec2::ONE).as_vec2() * CELLSIZE;
    let center = (environment.min.as_vec2() + environment.max.as_vec2()) * (CELLSIZE * 0.5);
    commands.spawn((
        LightningFlash {
            remaining: FLASH_SECONDS,
            alpha: 0.65,
        },
        Sprite::from_color(Color::srgba(0.8, 0.9, 1.0, 0.65), size),
        Transform::from_xyz(center.x, center.y, 6.0),
    ));
    // A jagged, tile-width bolt connects the top of the world to the struck cell.
    let mut start = cell.grid.as_vec2() * CELLSIZE;
    let top = environment.max.y as f32 * CELLSIZE;
    while start.y < top {
        let end = Vec2::new(
            (start.x + rng.random_range(-3.0..3.0) * CELLSIZE).clamp(
                environment.min.x as f32 * CELLSIZE,
                environment.max.x as f32 * CELLSIZE,
            ),
            (start.y + rng.random_range(3.0..7.0) * CELLSIZE).min(top),
        );
        let delta = end - start;
        let midpoint = (start + end) * 0.5;
        commands.spawn((
            LightningFlash {
                remaining: FLASH_SECONDS,
                alpha: 1.0,
            },
            Sprite::from_color(
                Color::srgb(0.8, 0.9, 1.0),
                Vec2::new(CELLSIZE, delta.length()),
            ),
            Transform::from_xyz(midpoint.x, midpoint.y, 7.0)
                .with_rotation(Quat::from_rotation_z(-delta.x.atan2(delta.y))),
        ));
        start = end;
    }
}

pub(crate) fn update_fire(
    mut commands: Commands,
    time: Res<Time>,
    noise: Res<FireNoise>,
    mut occupied: ResMut<Occupied>,
    plants: Query<(Entity, &PCell, &PCellRole), Without<Burning>>,
    mut burning: Query<(Entity, &PCell, &mut Burning)>,
    mut flames: Query<(&mut Sprite, &mut TileLight), With<Flame>>,
) {
    let fuel: HashMap<_, _> = plants
        .iter()
        .map(|(entity, cell, role)| (cell.grid, (entity, *role)))
        .collect();
    let mut ignitions = HashSet::new();
    for (entity, cell, mut fire) in &mut burning {
        fire.age += time.delta_secs();
        fire.spread_in -= time.delta_secs();
        if fire.spread_in <= 0.0 {
            fire.spread_in = FIRE_SPREAD_INTERVAL;
            for offset in NEIGHBORS {
                let grid = cell.grid + offset;
                if let Some(&(neighbor, role)) = fuel.get(&grid)
                    && ignitions.insert(neighbor)
                {
                    ignite(&mut commands, neighbor, grid, role);
                }
            }
        }

        if fire.age >= fire.duration {
            occupied.positions.remove(&cell.grid);
            commands.entity(fire.visual).despawn();
            // Removing plant behavior leaves children in place; despawning a
            // parent here would destroy the whole branch before it can burn.
            commands
                .entity(entity)
                .remove::<(PCell, PCellRole, FlowerBase, Pollinated, Sprite, Burning)>()
                .insert(BurntBranch);
            continue;
        }

        if let Ok((mut sprite, mut light)) = flames.get_mut(fire.visual) {
            // Spatial x/y noise evolves along its third (z) axis over time.
            let sample = noise.0.get([
                cell.grid.x as f64 * 0.4,
                cell.grid.y as f64 * 0.4,
                time.elapsed_secs_f64() * 3.0,
            ]);
            let heat = ((sample + 1.0) * 0.5).clamp(0.0, 1.0) as f32;
            let fade = ((fire.duration - fire.age) / 0.3).clamp(0.0, 1.0);
            sprite.color = Color::srgba(1.0, 0.35 + 0.65 * heat, 0.0, fade);
            light.intensity = (0.35 + 0.3 * heat) * fade;
        }
    }
}

pub(crate) fn fade_lightning(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut LightningFlash, &mut Sprite)>,
) {
    for (entity, mut flash, mut sprite) in &mut flashes {
        flash.remaining -= time.delta_secs();
        if flash.remaining <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            let fade = flash.remaining / FLASH_SECONDS;
            sprite.color = Color::srgba(0.8, 0.9, 1.0, flash.alpha * fade * fade);
        }
    }
}

pub(crate) fn clean_burnt_branches(
    mut commands: Commands,
    branches: Query<(Entity, Option<&Children>), With<BurntBranch>>,
) {
    for (entity, children) in &branches {
        if children.is_none_or(|children| children.is_empty()) {
            commands.entity(entity).despawn();
        }
    }
}
