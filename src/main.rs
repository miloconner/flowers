use bevy::prelude::*;

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

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn(Organism);
}
