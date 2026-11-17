mod operations;
mod mesh;
mod renderer;
mod shader;
mod surface_sync;
#[cfg(feature = "video-ffmpeg")]
pub(crate) mod video;

#[cfg(feature = "test-render-stats")]
pub use renderer::{RenderStats, stats};

use bevy::{
    asset::RenderAssetUsages,
    camera::{ScalingMode, visibility::RenderLayers},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        erased_render_asset::prepare_erased_assets,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::{RenderAssets, prepare_assets},
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        renderer::{RenderAdapter, RenderDevice, RenderQueue},
        texture::GpuImage,
    },
    sprite::{Anchor, SpriteAlphaMode},
    sprite_render::{MeshMaterial2d, SpriteMeshMaterial},
    window::PrimaryWindow,
};
use krkr_sys as sys;

use crate::KirikiriCamera;
use crate::host::KrkrScreen;

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
            render_app.add_systems(
                Render,
                import_texture
                    .in_set(RenderSystems::PrepareAssets)
                    .after(prepare_assets::<GpuImage>)
                    .before(prepare_erased_assets::<MeshMaterial2d<SpriteMeshMaterial>>),
            );
        }
    }
    fn finish(&self, app: &mut App) {
        initialize_with_adapter(
            app.world().resource::<RenderDevice>().clone(),
            app.world().resource::<RenderQueue>().clone(),
            app.world().resource::<RenderAdapter>().clone(),
        );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            surface_sync::install(render_app);
        }
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
    renderer::initialize(device, queue, None);
}

fn initialize_with_adapter(device: RenderDevice, queue: RenderQueue, adapter: RenderAdapter) {
    renderer::initialize(device, queue, Some(adapter));
}

pub fn flush() {
    renderer::flush();
}

pub fn read_pixels(texture: u64) -> Result<Vec<u8>, String> {
    renderer::read_pixels(texture)
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
    if !scene.is_changed() {
        return;
    }
    let current = scene.0.as_ref().map(|(handle, _)| handle.id());
    if *previous != current {
        if let Some(id) = previous.take() {
            images.remove(id);
        }
        *previous = current;
    }
    if let Some((handle, image)) = &scene.0 {
        images.insert(handle.id(), image.clone());
    }
}

fn show_scene(
    mut commands: Commands,
    screen: Option<NonSend<KrkrScreen>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&mut Camera, &mut Projection), With<KirikiriCamera>>,
    mut scene: ResMut<KirikiriScene>,
    mut texture: ResMut<SceneTexture>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SpriteMeshMaterial>>,
    mut screen_materials: Query<
        (&MeshMaterial2d<SpriteMeshMaterial>, &mut Transform),
        With<KirikiriScreen>,
    >,
) {
    let (mut width, mut height) = (0, 0);
    let id = screen.as_ref().map_or(0, |screen| unsafe {
        sys::krkr_scene_build(screen.as_ptr(), &mut width, &mut height)
    });
    if id == 0 || width <= 0 || height <= 0 {
        flush();
        scene.clear(&mut commands);
        texture.0 = None;
        return;
    }

    let Some(image) = renderer::image(id) else {
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

    let viewport_size = windows
        .single()
        .map_or(size, |window| window.resolution.physical_size());
    if viewport_size.x == 0 || viewport_size.y == 0 {
        return;
    }
    let scale = (viewport_size.as_vec2() / size.as_vec2()).min_element();
    let displayed = size.as_vec2() * scale;
    for (mut camera, mut projection) in &mut cameras {
        if camera.viewport.is_some() {
            camera.viewport = None;
        }
        if let Projection::Orthographic(orthographic) = &*projection
            && !matches!(orthographic.scaling_mode, ScalingMode::Fixed { width, height }
                if width == viewport_size.x as f32 && height == viewport_size.y as f32)
        {
            if let Projection::Orthographic(orthographic) = &mut *projection {
                orthographic.scaling_mode = ScalingMode::Fixed {
                    width: viewport_size.x as f32,
                    height: viewport_size.y as f32,
                };
            }
        }
    }

    let handle = scene.image.as_ref().unwrap().clone();
    let binding_changed = texture
        .0
        .as_ref()
        .is_none_or(|(previous_handle, previous)| {
            previous_handle.id() != handle.id()
                || previous.texture_view.id() != image.texture_view.id()
                || previous.sampler.id() != image.sampler.id()
        });
    if binding_changed {
        for (material, _) in &screen_materials {
            if let Some(material) = materials.get_mut(&material.0) {
                // Replacing a GpuImage does not rebuild an existing material binding.
                material.into_inner();
            }
        }
        texture.0 = Some((handle, image));
    }

    let transform = Transform::from_xyz(-displayed.x / 2.0, displayed.y / 2.0, 0.0)
        .with_scale(Vec3::new(scale, scale, 1.0));
    for (_, mut current) in &mut screen_materials {
        if *current != transform {
            *current = transform;
        }
    }
    if let Some(entity) = scene.entity {
        if resized {
            let handle = scene.image.as_ref().unwrap().clone();
            let mut sprite = Sprite::from_image(handle);
            sprite.custom_size = Some(size.as_vec2());
            sprite.alpha_mode = SpriteAlphaMode::Opaque;
            commands.entity(entity).insert((sprite, transform));
        }
    } else {
        let handle = scene.image.as_ref().unwrap().clone();
        let mut sprite = Sprite::from_image(handle);
        sprite.custom_size = Some(size.as_vec2());
        sprite.alpha_mode = SpriteAlphaMode::Opaque;
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
