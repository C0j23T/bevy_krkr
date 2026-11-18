use bevy::{
    DefaultPlugins,
    app::{App, PluginGroup, Startup, Update},
    camera::Camera2d,
    color::Color,
    ecs::{
        component::Component,
        query::With,
        system::{Commands, Res, Single},
    },
    math::vec2,
    sprite::Sprite,
    time::Time,
    transform::components::Transform,
    utils::default,
    window::{Window, WindowPlugin},
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, update)
        .run();
}

#[derive(Component)]
struct Marker;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((Sprite::from_color(Color::WHITE, vec2(200.0, 200.0)), Marker));
}

fn update(mut sprite: Single<&mut Transform, With<Marker>>, time: Res<Time>) {
    sprite.rotate_y(time.delta_secs());
}
