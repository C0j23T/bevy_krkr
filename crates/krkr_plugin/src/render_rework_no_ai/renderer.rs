// krkr renderer

use bevy::{platform::collections::HashMap, render::{render_resource::{ComputePipeline, Texture}, renderer::{RenderDevice, RenderQueue}}};

struct KrkrRenderer {
    device: RenderDevice,
    queue: RenderQueue,

    textures: HashMap<u64, Texture>,
    
    pipeline: ComputePipeline,
}
