use std::sync::RwLock;

pub const MAX_MESH_SURFACES: usize = 64;
pub const MAX_MESH_DIVISION: u32 = 254;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MeshSurface {
    pub control_points: [f32; 32],
    pub models: [[f32; 16]; 32],
    pub placement: [f32; 4],
    pub geometry: [f32; 4],
    pub offset: [f32; 4],
    pub kind: u32,
    pub inherit_mask: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MeshCommand {
    pub target: u64,
    pub source: u64,
    pub mask: u64,
    pub surfaces: *const MeshSurface,
    pub surface_count: u32,
    pub division_x: u32,
    pub division_y: u32,
    pub blend: u32,
    pub opacity: f32,
    pub tint: [f32; 4],
    pub color: [f32; 4],
}

pub struct MeshRequest {
    pub target: u64,
    pub source: u64,
    pub mask: u64,
    pub surfaces: Vec<MeshSurface>,
    pub division_x: u32,
    pub division_y: u32,
    pub blend: u32,
    pub opacity: f32,
    pub tint: [f32; 4],
    pub color: [f32; 4],
}

pub type MeshSink = fn(&MeshRequest) -> Result<(), String>;
static SINK: RwLock<Option<MeshSink>> = RwLock::new(None);

pub fn set_mesh_sink(sink: MeshSink) {
    *SINK.write().unwrap() = Some(sink);
}

impl MeshRequest {
    unsafe fn copy(command: &MeshCommand) -> Result<Self, String> {
        if command.target == 0 || command.source == 0
            || command.surface_count == 0 || command.surface_count as usize > MAX_MESH_SURFACES
            || command.surfaces.is_null()
            || !command.surfaces.is_aligned()
            || !(1..=MAX_MESH_DIVISION).contains(&command.division_x)
            || !(1..=MAX_MESH_DIVISION).contains(&command.division_y)
            || ![0, 1, 3, 4, 6, 21].contains(&command.blend)
            || !command.opacity.is_finite()
            || !command.tint.iter().chain(&command.color).all(|v| v.is_finite())
        {
            return Err("invalid mesh command".into());
        }
        let surfaces = unsafe {
            std::slice::from_raw_parts(command.surfaces, command.surface_count as usize)
        }.to_vec();
        for (i, surface) in surfaces.iter().enumerate() {
            if surface.kind > 3
                || !surface.control_points.iter()
                    .chain(surface.models.iter().flatten())
                    .chain(&surface.placement).chain(&surface.geometry).chain(&surface.offset)
                    .all(|v| v.is_finite())
                || (surface.kind == 1 && i + 1 < surfaces.len()
                    && (surface.geometry[2] == 0.0 || surface.geometry[3] == 0.0))
            {
                return Err("invalid mesh surface".into());
            }
        }
        Ok(Self {
            target: command.target, source: command.source, mask: command.mask,
            surfaces, division_x: command.division_x, division_y: command.division_y,
            blend: command.blend, opacity: command.opacity, tint: command.tint, color: command.color,
        })
    }
}

/// Copy and execute a complete GPU surface mesh operation.
///
/// # Safety
/// `command` and its `surface_count` initialized surfaces must remain readable and
/// properly aligned until this function returns. Null pointers are rejected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_render_mesh(command: *const MeshCommand) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if command.is_null() || !command.is_aligned() {
            return Err("invalid mesh command pointer".into());
        }
        let request = unsafe { MeshRequest::copy(&*command) }?;
        let sink = SINK.read().map_err(|_| "mesh sink poisoned")?
            .ok_or("Bevy renderer is not initialized")?;
        sink(&request)
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => { eprintln!("bevy_krkr: {error}"); -1 }
        Err(_) => -1,
    }
}

const _: () = {
    assert!(std::mem::size_of::<MeshSurface>() == 2240);
    assert!(std::mem::offset_of!(MeshSurface, models) == 128);
    assert!(std::mem::offset_of!(MeshSurface, placement) == 2176);
    assert!(std::mem::offset_of!(MeshSurface, kind) == 2224);
    assert!(std::mem::size_of::<MeshCommand>() == 88);
    assert!(std::mem::offset_of!(MeshCommand, surfaces) == 24);
    assert!(std::mem::offset_of!(MeshCommand, opacity) == 48);
    assert!(std::mem::offset_of!(MeshCommand, tint) == 52);
    assert!(std::mem::offset_of!(MeshCommand, color) == 68);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_commands_before_dereferencing_surface_pointer() {
        assert_eq!(unsafe { krkr_render_mesh(std::ptr::null()) }, -1);
        let mut c: MeshCommand = unsafe { std::mem::zeroed() };
        c.target = 1;
        c.source = 2;
        c.surfaces = std::ptr::dangling();
        c.surface_count = u32::MAX;
        c.division_x = 1;
        c.division_y = 1;
        assert_eq!(unsafe { krkr_render_mesh(&c) }, -1);
        c.surface_count = 1;
        c.division_x = u32::MAX;
        assert_eq!(unsafe { krkr_render_mesh(&c) }, -1);
    }
}
