//! Kirikiri screen → Bevy texture.
//!
//! Kirikiroid2 composites the screen's layer tree (background, message window
//! with its rendered text, …) off-screen; see [`crate::host`]. This module
//! copies the composited RGBA8 pixels into one Bevy [`Image`] when they
//! changed (`krkr_layer_get_revision` hashes them), and shows it with a
//! [`Sprite`]. The bytes go straight into the texture (`Rgba8UnormSrgb`, the
//! byte order Kirikiroid2 renders in; no per-pixel conversion) and are
//! uploaded to the GPU by Bevy's render-asset extraction. Nothing is read
//! back from the GPU.
//!
//! The sprite is on [`KIRIKIRI_RENDER_LAYER`]. Spawn your own [`Camera2d`]
//! and add [`KirikiriCamera`]; that layer is added to its [`RenderLayers`],
//! so other cameras do not draw the Kirikiri screen.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use krkr_kag::KirikiriCamera;
use krkr_sys as sys;

use crate::host::KrkrScreen;

/// Render layer used for the Kirikiri screen sprite.
pub const KIRIKIRI_RENDER_LAYER: usize = 27;

/// Marks the sprite showing the Kirikiri screen.
#[derive(Component, Debug)]
pub struct KirikiriScreen;

/// The texture holding the Kirikiri screen.
#[derive(Resource, Debug, Default)]
pub struct KirikiriScreenImage {
    pub image: Option<Handle<Image>>,
    pub size: UVec2,
    revision: Option<u64>,
}

#[derive(Default)]
pub struct KrkrRenderPlugin;

impl Plugin for KrkrRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KirikiriScreenImage>()
            .add_systems(
                PostUpdate,
                (setup_kirikiri_camera, crate::host::message_window, sync_screen).chain(),
            );
    }
}

type NewKirikiriCameras<'w, 's> =
    Query<'w, 's, (Entity, Option<&'static RenderLayers>, Has<Camera2d>), Added<KirikiriCamera>>;

/// Adds [`KIRIKIRI_RENDER_LAYER`] to cameras that just got [`KirikiriCamera`].
fn setup_kirikiri_camera(
    mut commands: Commands,
    added: NewKirikiriCameras,
    all: Query<(), With<KirikiriCamera>>,
) {
    for (entity, layers, is_2d) in &added {
        if !is_2d {
            warn!("bevy_krkr: KirikiriCamera on {entity} which has no Camera2d");
        }
        let layers = layers
            .cloned()
            .unwrap_or_default()
            .with(KIRIKIRI_RENDER_LAYER);
        commands.entity(entity).insert(layers);
    }
    if !added.is_empty() && all.iter().count() > 1 {
        warn!("bevy_krkr: more than one KirikiriCamera; the screen is drawn by all of them");
    }
}

/// Copies the composited screen into the screen texture when it changed.
pub(crate) fn sync_screen(
    mut commands: Commands,
    kirikiri: Option<NonSend<KrkrScreen>>,
    mut screen: ResMut<KirikiriScreenImage>,
    mut images: ResMut<Assets<Image>>,
    sprites: Query<Entity, With<KirikiriScreen>>,
) {
    let layer = kirikiri.as_ref().map_or(std::ptr::null_mut(), |s| s.as_ptr());
    if layer.is_null() {
        if screen.image.is_some() {
            sprites.iter().for_each(|e| commands.entity(e).despawn());
            *screen = KirikiriScreenImage::default();
        }
        return;
    }

    // SAFETY: layer is the live handle held by KrkrScreen; main thread via NonSend.
    let (revision, width, height) = unsafe {
        (
            sys::krkr_layer_get_revision(layer),
            sys::krkr_layer_get_width(layer),
            sys::krkr_layer_get_height(layer),
        )
    };
    if screen.revision == Some(revision) || width == 0 || height == 0 {
        return;
    }
    // SAFETY: the C ABI guarantees width*height*4 readable bytes for the
    // snapshot taken by krkr_layer_get_revision, until the next call on it.
    let pixels = unsafe {
        let ptr = sys::krkr_layer_get_pixels(layer);
        if ptr.is_null() {
            return;
        }
        std::slice::from_raw_parts(ptr, width as usize * height as usize * 4)
    };

    let size = UVec2::new(width, height);
    let updated = match screen.image.clone().filter(|_| screen.size == size) {
        Some(handle) => match images.get_mut(&handle) {
            Some(mut image) => {
                match image.data.as_mut() {
                    Some(data) => data.copy_from_slice(pixels),
                    None => image.data = Some(pixels.to_vec()),
                }
                true
            }
            None => false,
        },
        None => false,
    };
    if !updated {
        let image = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.to_vec(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        let handle = images.add(image);
        sprites.iter().for_each(|e| commands.entity(e).despawn());
        commands.spawn((
            Sprite::from_image(handle.clone()),
            RenderLayers::layer(KIRIKIRI_RENDER_LAYER),
            KirikiriScreen,
            Name::new("Kirikiri screen"),
        ));
        screen.image = Some(handle);
        screen.size = size;
    }
    screen.revision = Some(revision);
}
