//! Tile lighting shared by clouds, the day/night cycle, and local light sources.
use bevy::prelude::*;
use std::collections::HashMap;

use crate::{Environment, LightNoise, LightTile, SUN_THRESHOLD, light_value, shade};

/// Noon to midnight takes two minutes at 1x; a complete cycle takes four.
const HALF_CYCLE_SECONDS: f64 = 120.0;
const FADE_SECONDS: f64 = 10.0;

#[derive(Resource)]
pub(crate) struct DayNightCycle {
    elapsed: f64,
    speed: f64,
}

impl Default for DayNightCycle {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            speed: 1.0,
        }
    }
}

impl DayNightCycle {
    fn advance(&mut self, seconds: f64) {
        self.elapsed = (self.elapsed + seconds * self.speed).rem_euclid(2.0 * HALF_CYCLE_SECONDS);
    }

    pub(crate) fn daylight(&self) -> f32 {
        // Hold full day/night between short, smooth dusk and dawn fades.
        let phase = self.elapsed % HALF_CYCLE_SECONDS;
        let fade_start = (HALF_CYCLE_SECONDS - FADE_SECONDS) / 2.0;
        let progress = ((phase - fade_start) / FADE_SECONDS).clamp(0.0, 1.0);
        let fade = (progress * progress * (3.0 - 2.0 * progress)) as f32;
        if self.elapsed < HALF_CYCLE_SECONDS {
            1.0 - fade
        } else {
            fade
        }
    }
}

/// Spawn this component to illuminate nearby tiles, including under clouds.
/// Position and radius are in grid cells. Intensity 1 lights the center fully.
/// For example: `commands.spawn(TileLight { grid: IVec2::ZERO,
/// radius: 12.0, intensity: 0.9 });`
/// Move the light by updating `grid`; despawn it to turn it off.
#[derive(Component)]
pub(crate) struct TileLight {
    pub grid: IVec2,
    pub radius: f32,
    pub intensity: f32,
}

impl TileLight {
    fn contribution(&self, grid: IVec2) -> f32 {
        if self.radius <= 0.0 || self.intensity <= 0.0 {
            return 0.0;
        }
        let distance = grid.as_vec2().distance(self.grid.as_vec2());
        let falloff = (1.0 - distance / self.radius).clamp(0.0, 1.0);
        self.intensity * falloff * falloff
    }
}

pub(crate) fn advance_day_night(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cycle: ResMut<DayNightCycle>,
) {
    let previous_speed = cycle.speed;
    if keys.just_pressed(KeyCode::KeyU) {
        cycle.speed = 1.0;
    } else if keys.just_pressed(KeyCode::KeyI) {
        cycle.speed = 5.0;
    } else if keys.just_pressed(KeyCode::KeyO) {
        cycle.speed = 10.0;
    }
    if cycle.speed != previous_speed {
        info!("Day/night speed: {}x (U: 1x, I: 5x, O: 10x)", cycle.speed);
    }
    cycle.advance(time.delta_secs_f64());
}

pub(crate) fn animate_light(
    time: Res<Time>,
    noise: Res<LightNoise>,
    cycle: Res<DayNightCycle>,
    lights: Query<&TileLight>,
    mut env: ResMut<Environment>,
    mut tiles: Query<(&LightTile, &mut Sprite)>,
) {
    let daylight = cycle.daylight();
    let night = 1.0 - daylight;
    let t = time.elapsed_secs_f64();
    // Only visit each light's footprint, so a large fire does not require
    // testing every flame against every tile in the world.
    let mut local_lighting: HashMap<IVec2, f32> = HashMap::new();
    for light in &lights {
        if light.radius <= 0.0 || light.intensity <= 0.0 {
            continue;
        }
        let extent = IVec2::splat(light.radius.ceil() as i32);
        let min = (light.grid - extent).max(env.min);
        let max = (light.grid + extent).min(env.max);
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let grid = IVec2::new(x, y);
                let contribution = light.contribution(grid);
                if contribution > 0.0 {
                    *local_lighting.entry(grid).or_default() += contribution;
                }
            }
        }
    }

    for (tile, mut sprite) in &mut tiles {
        let cloud = light_value(&noise.0, tile.grid.x, tile.grid.y, t);
        let local_light = local_lighting
            .get(&tile.grid)
            .copied()
            .unwrap_or_default()
            .clamp(0.0, 1.0);
        // One shared overlay: local lights lift both cloud shadows and darkness.
        // All scene sprites remain below this layer, including bees and water.
        let darkness = (shade(cloud) * daylight + 0.88 * night) * (1.0 - local_light);
        sprite.color = if night < 0.001 && cloud > 0.4 {
            Color::srgba(
                1.0,
                1.0,
                0.0,
                0.01 * cloud as f32 * daylight * (1.0 - local_light),
            )
        } else {
            Color::srgba(0.012 * night, 0.035 * night, 0.11 * night, darkness)
        };

        if let Some(environment) = env.tiles.get_mut(&tile.grid) {
            environment.light =
                (daylight * ((cloud + 1.0) / 2.0) as f32 + local_light).clamp(0.0, 1.0);
            // Artificial lights provide light for growth, but are not sunlight.
            environment.sun = daylight > 0.15 && cloud > SUN_THRESHOLD;
        }
    }
}
