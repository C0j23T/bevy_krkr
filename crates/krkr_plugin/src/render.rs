mod gpu;

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::RenderLayers,
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
        Render, RenderApp, RenderSystems,
    },
    sprite::{Anchor, SpriteAlphaMode},
};
use krkr_sys as sys;

use crate::host::KrkrScreen;
use crate::KirikiriCamera;

pub const KIRIKIRI_RENDER_LAYER: usize = 27;

#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
pub struct KirikiriScreen;

#[derive(Resource, Reflect, Debug, Default)]
#[reflect(Resource)]
pub struct KirikiriScene {
    pub size: UVec2,
    #[reflect(ignore)]
    entity: Option<Entity>,
    #[reflect(ignore)]
    image: Option<Handle<Image>>,
}

#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(RenderApp)]
struct SceneTexture(Option<(Handle<Image>, GpuImage)>);

#[derive(Default)]
pub struct KrkrRenderPlugin;

impl Plugin for KrkrRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KirikiriScene>()
            .init_resource::<SceneTexture>()
            .register_type::<KirikiriScene>()
            .register_type::<KirikiriScreen>()
            .add_plugins(ExtractResourcePlugin::<SceneTexture>::default())
            .add_systems(
                PostUpdate,
                (setup_camera, show_scene)
                    .chain()
                    .before(bevy::asset::AssetEventSystems),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(Render, import_texture.in_set(RenderSystems::PrepareAssets));
        }
    }
    fn finish(&self, app: &mut App) {
        initialize(
            app.world().resource::<RenderDevice>().clone(),
            app.world().resource::<RenderQueue>().clone(),
        );
    }
}

pub(crate) fn reset(world: &mut World) {
    if let Some(mut scene) = world.get_resource_mut::<KirikiriScene>() {
        let entity = scene.entity.take();
        scene.image = None;
        scene.size = UVec2::ZERO;
        if let Some(entity) = entity {
            world.despawn(entity);
        }
    }
    if let Some(mut texture) = world.get_resource_mut::<SceneTexture>() {
        texture.0 = None;
    }
}

pub fn initialize(device: RenderDevice, queue: RenderQueue) {
    gpu::initialize(device, queue);
}

pub fn texture_usage() -> (usize, u64) {
    gpu::texture_usage()
}

pub fn flush() {
    gpu::flush();
}

pub fn read_pixels(texture: u64) -> Result<Vec<u8>, String> {
    gpu::read_pixels(texture)
}

fn setup_camera(
    mut commands: Commands,
    cameras: Query<(Entity, Option<&RenderLayers>), Added<KirikiriCamera>>,
) {
    for (entity, layers) in &cameras {
        commands.entity(entity).insert(
            layers
                .cloned()
                .unwrap_or_default()
                .with(KIRIKIRI_RENDER_LAYER),
        );
    }
}

fn import_texture(
    scene: Res<SceneTexture>,
    mut images: ResMut<RenderAssets<GpuImage>>,
    mut previous: Local<Option<AssetId<Image>>>,
) {
    let current = scene.0.as_ref().map(|(handle, _)| handle.id());
    if *previous != current {
        if let Some(id) = previous.take() {
            images.remove(id);
        }
    }
    if let Some((handle, image)) = &scene.0 {
        images.insert(handle.id(), image.clone());
    }
    *previous = current;
}

fn show_scene(
    mut commands: Commands,
    screen: Option<NonSend<KrkrScreen>>,
    mut scene: ResMut<KirikiriScene>,
    mut texture: ResMut<SceneTexture>,
    mut images: ResMut<Assets<Image>>,
) {
    let (mut width, mut height) = (0, 0);
    let id = screen.as_ref().map_or(0, |screen| unsafe {
        sys::krkr_scene_build(screen.as_ptr(), &mut width, &mut height)
    });
    if id == 0 || width <= 0 || height <= 0 {
        gpu::flush();
        scene.clear(&mut commands);
        texture.0 = None;
        return;
    }
    let Some(image) = gpu::image(id) else {
        return;
    };
    let size = UVec2::new(width as u32, height as u32);
    let resized = scene.image.is_none() || scene.size != size;
    if resized {
        scene.image = Some(images.add(Image::new_uninit(
            Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )));
    }
    scene.size = size;
    let handle = scene.image.as_ref().unwrap().clone();
    texture.0 = Some((handle.clone(), image));
    let mut sprite = Sprite::from_image(handle);
    sprite.custom_size = Some(size.as_vec2());
    sprite.alpha_mode = SpriteAlphaMode::Opaque;
    let transform = Transform::from_xyz(-(width as f32) / 2.0, height as f32 / 2.0, 0.0);
    if let Some(entity) = scene.entity {
        if resized {
            commands.entity(entity).insert((sprite, transform));
        }
    } else {
        scene.entity = Some(
            commands
                .spawn((
                    KirikiriScreen,
                    sprite,
                    transform,
                    Anchor::TOP_LEFT,
                    RenderLayers::layer(KIRIKIRI_RENDER_LAYER),
                ))
                .id(),
        );
    }
}

impl KirikiriScene {
    pub fn clear(&mut self, commands: &mut Commands) {
        if let Some(entity) = self.entity.take() {
            commands.entity(entity).try_despawn();
        }
        self.image = None;
        self.size = UVec2::ZERO;
    }
}
