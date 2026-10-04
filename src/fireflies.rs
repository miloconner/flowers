use bevy::prelude::*;
use rand::RngExt;

use crate::lighting::{DayNightCycle, TileLight};
use crate::{CELLSIZE, Environment};

const MAX_FIREFLIES: usize = 40;
const LIGHT_RADIUS: f32 = 8.0;

#[derive(Component)]
pub(crate) struct Firefly {
    phase: f32,
    frequency: f32,
    move_timer: f32,
}

impl Firefly {
    fn glow(&self) -> f32 {
        (0.5 + 0.5 * self.phase.sin()).powi(2)
    }
}

fn glow_color(glow: f32, visibility: f32) -> Color {
    Color::srgba(
        0.55 + 0.45 * glow,
        1.0,
        0.025,
        (0.55 + 0.45 * glow) * visibility,
    )
}

pub(crate) fn update_fireflies(
    mut commands: Commands,
    time: Res<Time>,
    cycle: Res<DayNightCycle>,
    environment: Res<Environment>,
    mut spawn_timer: Local<f32>,
    mut flies: Query<(
        Entity,
        &mut Firefly,
        &mut Sprite,
        &mut Transform,
        &mut TileLight,
    )>,
) {
    let daylight = cycle.daylight();
    let visibility = 1.0 - daylight;
    if visibility <= 0.0 {
        for (entity, ..) in &mut flies {
            // Remove the body and its light only after dawn fades them to zero.
            commands.entity(entity).despawn();
        }
        *spawn_timer = 0.0;
        return;
    }

    let dt = time.delta_secs();
    let mut rng = rand::rng();
    for (_, mut fly, mut sprite, mut transform, mut light) in &mut flies {
        // Each bug owns its pulse clock and picks a fresh speed each cycle.
        fly.phase += dt * fly.frequency * std::f32::consts::TAU;
        if fly.phase >= std::f32::consts::TAU {
            fly.phase = fly.phase.rem_euclid(std::f32::consts::TAU);
            fly.frequency = rng.random_range(3.0..6.0);
        }
        let pulse = fly.glow();
        sprite.color = glow_color(pulse, visibility);
        light.intensity = (0.04 + 0.18 * pulse) * visibility;

        fly.move_timer -= dt;
        while fly.move_timer <= 0.0 {
            fly.move_timer += rng.random_range(0.3..0.7);
            let directions = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
            let next = light.grid + directions[rng.random_range(0..directions.len())];
            if environment.inside(next) {
                light.grid = next;
            }
        }
        transform.translation.x = light.grid.x as f32 * CELLSIZE;
        transform.translation.y = light.grid.y as f32 * CELLSIZE;
    }

    // Existing bugs fade through dawn; new arrivals only spawn in full night.
    if daylight > 0.0 {
        *spawn_timer = 0.0;
        return;
    }
    // Keep a sparse population, with staggered arrivals and fresh locations each night.
    let size = environment.max.as_vec2() - environment.min.as_vec2() + Vec2::ONE;
    let limit = ((size.x * size.y / 1800.0).ceil() as usize).clamp(1, MAX_FIREFLIES);
    *spawn_timer -= dt;
    if flies.iter().count() >= limit || *spawn_timer > 0.0 {
        return;
    }
    *spawn_timer = rng.random_range(0.15..0.5);
    let grid = IVec2::new(
        rng.random_range(environment.min.x..=environment.max.x),
        rng.random_range(environment.min.y..=environment.max.y),
    );
    let fly = Firefly {
        phase: rng.random_range(0.0..std::f32::consts::TAU),
        frequency: rng.random_range(3.0..6.0),
        move_timer: rng.random_range(0.3..0.7),
    };
    let pulse = fly.glow();
    commands.spawn((
        fly,
        Sprite::from_color(glow_color(pulse, visibility), Vec2::splat(CELLSIZE)),
        // Emissive bodies stay visible above the cloud/night overlay at z=3.
        Transform::from_xyz(grid.x as f32 * CELLSIZE, grid.y as f32 * CELLSIZE, 4.0),
        TileLight {
            grid,
            radius: LIGHT_RADIUS,
            intensity: (0.04 + 0.18 * pulse) * visibility,
        },
    ));
}
