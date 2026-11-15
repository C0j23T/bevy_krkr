#[cfg(feature = "video-ffmpeg")]
use bevy::render::render_resource::Texture;

#[cfg(feature = "video-ffmpeg")]
pub(crate) struct VideoTexture(Texture);