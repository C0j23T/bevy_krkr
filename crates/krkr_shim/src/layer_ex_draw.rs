use std::sync::RwLock;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PathElement {
    pub verb: u32,
    pub reserved: u32,
    pub points: [f64; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ColorStop {
    pub position: f64,
    pub color: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BrushPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DrawPaint {
    pub brush_type: u32,
    pub stroke: u32,
    pub colors: [u32; 2],
    pub wrap: u32,
    pub hatch: u32,
    pub image: u64,
    pub coordinates: [f64; 4],
    pub paint_transform: [f64; 6],
    pub offset: [f64; 2],
    pub width: f64,
    pub miter_limit: f64,
    pub dash_offset: f64,
    pub start_cap: u32,
    pub end_cap: u32,
    pub dash_cap: u32,
    pub join: u32,
    pub alignment: u32,
    pub stop_count: u32,
    pub stops: *const ColorStop,
    pub dash_count: u32,
    pub reserved: u32,
    pub dashes: *const f64,
    pub tile: [f64; 4],
    pub center: [f64; 2],
    pub focus: [f64; 2],
    pub brush_point_count: u32,
    pub surround_count: u32,
    pub brush_points: *const BrushPoint,
    pub surround_colors: *const u32,
    pub blend_count: u32,
    pub brush_flags: u32,
    pub blend_positions: *const f64,
    pub blend_factors: *const f64,
    pub compound_count: u32,
    pub reserved2: u32,
    pub compound: *const f64,
    pub start_arrow: [f64; 4],
    pub end_arrow: [f64; 4],
}

#[repr(C)]
pub struct DrawOperation {
    pub target: u64,
    pub clip: [i32; 4],
    pub transform: [f64; 6],
    pub path: *const PathElement,
    pub path_count: u32,
    pub fill_rule: u32,
    pub paints: *const DrawPaint,
    pub paint_count: u32,
    pub alpha_mode: u32,
    pub flags: u32,
    pub reserved: u32,
    pub bounds: [f64; 4],
}

#[derive(Clone, Debug)]
pub struct Paint {
    pub brush_type: u32,
    pub stroke: bool,
    pub colors: [u32; 2],
    pub wrap: u32,
    pub hatch: u32,
    pub image: u64,
    pub coordinates: [f64; 4],
    pub transform: [f64; 6],
    pub offset: [f64; 2],
    pub width: f64,
    pub miter_limit: f64,
    pub dash_offset: f64,
    pub start_cap: u32,
    pub end_cap: u32,
    pub dash_cap: u32,
    pub join: u32,
    pub alignment: u32,
    pub stops: Vec<ColorStop>,
    pub dashes: Vec<f64>,
    pub tile: [f64; 4],
    pub center: [f64; 2],
    pub focus: [f64; 2],
    pub brush_points: Vec<BrushPoint>,
    pub surround_colors: Vec<u32>,
    pub brush_flags: u32,
    pub blend_positions: Vec<f64>,
    pub blend_factors: Vec<f64>,
    pub compound: Vec<f64>,
    pub start_arrow: [f64; 4],
    pub end_arrow: [f64; 4],
}

#[derive(Clone, Debug)]
pub struct DrawRequest {
    pub target: u64,
    pub clip: [i32; 4],
    pub transform: [f64; 6],
    pub path: Vec<PathElement>,
    pub fill_rule: u32,
    pub paints: Vec<Paint>,
    pub alpha_mode: u32,
    pub flags: u32,
}

pub type DrawSink = fn(&DrawRequest) -> Result<[f64; 4], String>;
static SINK: RwLock<Option<DrawSink>> = RwLock::new(None);

pub fn set_draw_sink(sink: DrawSink) {
    *SINK.write().unwrap() = Some(sink);
}

fn finite(values: impl IntoIterator<Item = f64>) -> bool {
    values
        .into_iter()
        .all(|value| value.is_finite() && value.abs() <= 1e9)
}

unsafe fn copy_slice<T: Copy>(
    pointer: *const T,
    count: u32,
    maximum: u32,
) -> Result<Vec<T>, String> {
    if count > maximum || (count != 0 && (pointer.is_null() || !pointer.is_aligned())) {
        return Err("invalid LayerExDraw array".into());
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    Ok(unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec())
}

impl DrawRequest {
    /// # Safety
    /// Every nonempty array in `operation`, including paint arrays, must point to
    /// initialized, aligned, readable elements until this function returns.
    pub unsafe fn copy(operation: &DrawOperation) -> Result<Self, String> {
        if operation.target == 0
            || operation.fill_rule > 1
            || operation.alpha_mode > 2
            || operation.flags & !1 != 0
            || !finite(operation.transform)
            || operation.clip[2] < operation.clip[0]
            || operation.clip[3] < operation.clip[1]
        {
            return Err("invalid LayerExDraw operation".into());
        }
        let path = unsafe { copy_slice(operation.path, operation.path_count, 1_000_000) }?;
        let raw_paints = unsafe { copy_slice(operation.paints, operation.paint_count, 1024) }?;
        let mut open = false;
        for element in &path {
            let used = match element.verb {
                0 | 1 => 2,
                2 => 6,
                3 => 0,
                _ => return Err("invalid path verb".into()),
            };
            if element.verb != 0 && !open {
                return Err("path must start with move".into());
            }
            if element.verb == 0 {
                open = true;
            }
            if !finite(element.points[..used].iter().copied()) {
                return Err("invalid path coordinates".into());
            }
        }
        let mut paints = Vec::with_capacity(raw_paints.len());
        for paint in raw_paints {
            if paint.brush_type > 4
                || paint.stroke > 1
                || paint.wrap > 4
                || paint.alignment > 1
                || !finite(
                    paint
                        .coordinates
                        .into_iter()
                        .chain(paint.paint_transform)
                        .chain(paint.offset)
                        .chain([paint.width, paint.miter_limit, paint.dash_offset]),
                )
                || paint.width < 0.0
                || paint.miter_limit < 0.0
            {
                return Err("invalid LayerExDraw paint".into());
            }
            let stops = unsafe { copy_slice(paint.stops, paint.stop_count, 4096) }?;
            let dashes = unsafe { copy_slice(paint.dashes, paint.dash_count, 4096) }?;
            if !finite(dashes.iter().copied())
                || dashes.iter().any(|&v| v <= 0.0)
                || stops
                    .iter()
                    .any(|v| !v.position.is_finite() || !(0.0..=1.0).contains(&v.position))
                || stops.windows(2).any(|v| v[0].position > v[1].position)
                || (!stops.is_empty()
                    && (stops.len() < 2
                        || stops[0].position != 0.0
                        || stops.last().unwrap().position != 1.0))
            {
                return Err("invalid LayerExDraw stops or dashes".into());
            }
            let brush_points =
                unsafe { copy_slice(paint.brush_points, paint.brush_point_count, 65536) }?;
            let surround_colors =
                unsafe { copy_slice(paint.surround_colors, paint.surround_count, 65536) }?;
            let blend_positions =
                unsafe { copy_slice(paint.blend_positions, paint.blend_count, 4096) }?;
            let blend_factors =
                unsafe { copy_slice(paint.blend_factors, paint.blend_count, 4096) }?;
            let compound = unsafe { copy_slice(paint.compound, paint.compound_count, 4096) }?;
            if !finite(
                paint
                    .tile
                    .into_iter()
                    .chain(paint.center)
                    .chain(paint.focus)
                    .chain(paint.start_arrow)
                    .chain(paint.end_arrow),
            ) || paint.focus.iter().any(|value| !(0.0..=1.0).contains(value))
                || brush_points.iter().any(|point| !finite([point.x, point.y]))
                || paint.brush_flags & !1 != 0
                || blend_positions
                    .iter()
                    .chain(&blend_factors)
                    .chain(&compound)
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                || blend_positions.windows(2).any(|pair| pair[0] > pair[1])
                || compound.len() % 2 != 0
                || compound.windows(2).any(|pair| pair[0] > pair[1])
                || (!blend_positions.is_empty()
                    && (blend_positions.len() < 2
                        || blend_positions[0] != 0.0
                        || *blend_positions.last().unwrap() != 1.0))
            {
                return Err("invalid LayerExDraw brush geometry or factors".into());
            }
            paints.push(Paint {
                brush_type: paint.brush_type,
                stroke: paint.stroke == 1,
                colors: paint.colors,
                wrap: paint.wrap,
                hatch: paint.hatch,
                image: paint.image,
                coordinates: paint.coordinates,
                transform: paint.paint_transform,
                offset: paint.offset,
                width: paint.width,
                miter_limit: paint.miter_limit,
                dash_offset: paint.dash_offset,
                start_cap: paint.start_cap,
                end_cap: paint.end_cap,
                dash_cap: paint.dash_cap,
                join: paint.join,
                alignment: paint.alignment,
                stops,
                dashes,
                tile: paint.tile,
                center: paint.center,
                focus: paint.focus,
                brush_points,
                surround_colors,
                brush_flags: paint.brush_flags,
                blend_positions,
                blend_factors,
                compound,
                start_arrow: paint.start_arrow,
                end_arrow: paint.end_arrow,
            });
        }
        Ok(Self {
            target: operation.target,
            clip: operation.clip,
            transform: operation.transform,
            path,
            fill_rule: operation.fill_rule,
            paints,
            alpha_mode: operation.alpha_mode,
            flags: operation.flags,
        })
    }
}

/// # Safety
/// All arrays must be initialized and readable, and `operation` writable, until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_draw(operation: *mut DrawOperation) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if operation.is_null() || !operation.is_aligned() {
            return Err("invalid LayerExDraw pointer".into());
        }
        let request = unsafe { DrawRequest::copy(&*operation) }?;
        let sink = SINK
            .read()
            .map_err(|_| "LayerExDraw sink poisoned")?
            .ok_or("Bevy renderer is not initialized")?;
        let bounds = sink(&request)?;
        unsafe {
            (*operation).bounds = bounds;
        }
        Ok::<_, String>(())
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("bevy_krkr: {error}");
            -1
        }
        Err(_) => -1,
    }
}

const _: () = {
    assert!(std::mem::size_of::<PathElement>() == 56);
    assert!(std::mem::size_of::<ColorStop>() == 16);
    assert!(std::mem::size_of::<DrawPaint>() == 392);
    assert!(std::mem::size_of::<DrawOperation>() == 144);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_arrays_and_nonfinite_coordinates() {
        assert_eq!(unsafe { krkr_layer_ex_draw(std::ptr::null_mut()) }, -1);
        let mut operation: DrawOperation = unsafe { std::mem::zeroed() };
        operation.target = 1;
        operation.path_count = u32::MAX;
        operation.path = std::ptr::dangling();
        assert!(unsafe { DrawRequest::copy(&operation) }.is_err());
        operation.path_count = 0;
        operation.transform[0] = f64::NAN;
        assert!(unsafe { DrawRequest::copy(&operation) }.is_err());
    }
}
