#[path = "layer_ex_draw_record_format.rs"]
mod format;

use super::{PooledTexture, RENDERER_INSTANCE};
use format::{Bitmap, Clip, Record, RecordedDraw, multiply};
use vello_gpu::{kurbo::Affine, peniko::Fill};
use krkr_shim::layer_ex_draw::DrawOperation;
use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone)]
struct StoredRecord {
    content: Record,
    textures: HashMap<u64, PooledTexture>,
}

static RECORDS: LazyLock<Mutex<HashMap<u64, Arc<StoredRecord>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_RECORD: AtomicU64 = AtomicU64::new(1);
static NEXT_IMAGE: AtomicU64 = AtomicU64::new(1 << 63);

#[repr(C)]
pub struct RecordInfo {
    pub width: u32,
    pub height: u32,
    pub background_argb: u32,
    pub operation_count: u32,
    pub bounds: [f64; 4],
    pub dpi: [f64; 2],
}

fn ffi(action: impl FnOnce() -> Result<(), String>) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("bevy_krkr: {error}");
            -1
        }
        Err(_) => -1,
    }
}
fn get(id: u64) -> Result<Arc<StoredRecord>, String> {
    RECORDS
        .lock()
        .map_err(|e| e.to_string())?
        .get(&id)
        .cloned()
        .ok_or_else(|| "unknown LayerExDraw record".into())
}
unsafe fn output<T>(pointer: *mut T, value: T) -> Result<(), String> {
    if pointer.is_null() || !pointer.is_aligned() {
        return Err("invalid record output pointer".into());
    }
    unsafe {
        pointer.write(value);
    }
    Ok(())
}
unsafe fn array<T: Copy, const N: usize>(pointer: *const T) -> Result<[T; N], String> {
    if pointer.is_null() || !pointer.is_aligned() {
        return Err("invalid record array pointer".into());
    }
    Ok(unsafe { *(pointer.cast::<[T; N]>()) })
}
unsafe fn insert(record: StoredRecord, result: *mut u64) -> Result<(), String> {
    if result.is_null() || !result.is_aligned() {
        return Err("invalid record handle pointer".into());
    }
    let id = NEXT_RECORD.fetch_add(1, Ordering::Relaxed);
    RECORDS
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id, Arc::new(record));
    unsafe {
        result.write(id);
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_create(
    width: u32,
    height: u32,
    background: u32,
    result: *mut u64,
) -> i32 {
    ffi(|| unsafe {
        insert(
            StoredRecord {
                content: Record::new(width, height, background)?,
                textures: HashMap::new(),
            },
            result,
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_append(
    id: u64,
    operation: *const DrawOperation,
    smoothing: u32,
) -> i32 {
    ffi(|| {
        if operation.is_null() || !operation.is_aligned() || smoothing > 6 {
            return Err("invalid recorded operation".into());
        }
        let mut request = unsafe { krkr_shim::layer_ex_draw::DrawRequest::copy(&*operation) }?;
        let mut records = RECORDS.lock().map_err(|e| e.to_string())?;
        let record = records.get_mut(&id).ok_or("unknown LayerExDraw record")?;
        let record = Arc::make_mut(record);
        if record.content.operations.len() >= 100_000 {
            return Err("record operation limit".into());
        }
        if request.paints.iter().any(|p| p.brush_type == 2) {
            let mut guard = RENDERER_INSTANCE.lock().map_err(|e| e.to_string())?;
            let renderer = guard.as_mut().ok_or("Bevy renderer is not initialized")?;
            for paint in &request.paints {
                if paint.brush_type == 2 {
                    renderer.texture(paint.image)?;
                }
            }
            let mut copies = HashMap::new();
            for paint in &mut request.paints {
                if paint.brush_type != 2 {
                    continue;
                }
                let image = if let Some(id) = copies.get(&paint.image) {
                    *id
                } else {
                    let source = renderer.texture(paint.image)?;
                    let image = renderer.texture_pool.acquire(
                        &renderer.device,
                        source.texture.width(),
                        source.texture.height(),
                    );
                    renderer.encoder().copy_texture_to_texture(
                        source.texture.as_image_copy(),
                        image.texture.as_image_copy(),
                        source.texture.size(),
                    );
                    let id = NEXT_IMAGE.fetch_add(1, Ordering::Relaxed);
                    copies.insert(paint.image, id);
                    record.textures.insert(id, image);
                    id
                };
                paint.image = image;
            }
        }
        record.content.operations.push(RecordedDraw { draw:request, clips:Vec::new() });
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_append_image(id:u64, image:u64,
    source:*const f64, transform:*const f64) -> i32 {
    ffi(|| {
        let source = unsafe { array(source)? };
        let transform = unsafe { array(transform)? };
        if !valid_source(source) || transform.iter().any(|n:&f64| !n.is_finite() || n.abs()>1e9) {
            return Err("invalid embedded metafile coordinates".into());
        }
        let image = get(image)?;
        let mut records = RECORDS.lock().map_err(|e| e.to_string())?;
        let dest = Arc::make_mut(records.get_mut(&id).ok_or("unknown LayerExDraw record")?);
        let resources:HashMap<_,_> = image.textures.keys().chain(image.content.images.keys())
            .map(|id| (*id,NEXT_IMAGE.fetch_add(1,Ordering::Relaxed))).collect();
        dest.content.embed(&image.content,source,transform,&[],&resources)?;
        dest.textures.extend(image.textures.iter().map(|(id,tex)|(resources[id],tex.clone())));
        dest.content.images.extend(image.content.images.iter().map(|(id,bmp)|(resources[id],bmp.clone())));
        Ok(())
    })
}

fn valid_source(rect:[f64;4]) -> bool {
    rect.iter().all(|n|n.is_finite() && n.abs() <= 1e9) && rect[2] > 0. && rect[3] > 0.
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_decode(
    bytes: *const c_void,
    length: usize,
    result: *mut u64,
) -> i32 {
    ffi(|| {
        if bytes.is_null() || length > 256 * 1024 * 1024 {
            return Err("invalid metafile input".into());
        }
        let content = format::decode(unsafe { std::slice::from_raw_parts(bytes.cast(), length) })?;
        unsafe {
            insert(
                StoredRecord {
                    content,
                    textures: HashMap::new(),
                },
                result,
            )
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_encode(
    id: u64,
    bytes: *mut *mut c_void,
    length: *mut usize,
) -> i32 {
    ffi(|| {
        if bytes.is_null() || !bytes.is_aligned() || length.is_null() || !length.is_aligned() {
            return Err("invalid metafile output".into());
        }
        let stored = get(id)?;
        let mut record = stored.content.clone();
        if !stored.textures.is_empty() {
            let mut guard = RENDERER_INSTANCE.lock().map_err(|e| e.to_string())?;
            let renderer = guard.as_mut().ok_or("Bevy renderer is not initialized")?;
            for (&id, image) in &stored.textures {
                let (width, height) = (image.texture.width(), image.texture.height());
                let mut rgba = vec![0; width as usize * height as usize * 4];
                renderer.textures.insert(id, image.clone());
                let result = unsafe { renderer.read(id, 4, width as usize * 4, rgba.as_mut_ptr()) };
                renderer.textures.remove(&id);
                result?;
                record.images.insert(
                    id,
                    Bitmap {
                        width,
                        height,
                        rgba:rgba.into(),
                    },
                );
            }
        }
        let encoded = format::encode(&record)?.into_boxed_slice();
        let size = encoded.len();
        unsafe {
            bytes.write(Box::into_raw(encoded).cast::<u8>().cast());
            length.write(size);
        }
        Ok(())
    })
}

fn replay(
    id: u64,
    target: u64,
    clip: [i32; 4],
    view: [f64; 6],
    alpha: u32,
    source: Option<[f64; 4]>,
) -> Result<(), String> {
    if alpha > 2
        || target == 0
        || clip[2] < clip[0]
        || clip[3] < clip[1]
        || view.iter().any(|v| !v.is_finite() || v.abs() > 1e9)
        || source.is_some_and(|r| !valid_source(r))
    {
        return Err("invalid record replay arguments".into());
    }
    let stored = get(id)?;
    let mut guard = RENDERER_INSTANCE.lock().map_err(|e| e.to_string())?;
    let renderer = guard.as_mut().ok_or("Bevy renderer is not initialized")?;
    let mut resources=stored.textures.clone();
    for (&id, image) in &stored.content.images {
        let key=image.rgba.as_ptr() as usize;
        let texture=if let Some((_,texture))=renderer.record_images.get(&key) {
            texture.clone()
        } else {
            if image.width.max(image.height)>renderer.device.limits().max_texture_dimension_2d {
                return Err("metafile bitmap exceeds GPU texture limit".into());
            }
            let texture=renderer.texture_pool.acquire(&renderer.device,image.width,image.height);
            renderer.flush();
            renderer.queue.write_texture(
                texture.texture.as_image_copy(),&image.rgba,
                wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(image.width*4),rows_per_image:Some(image.height)},
                texture.texture.size(),
            );
            renderer.record_images.insert(key,(Arc::downgrade(&image.rgba),texture.clone()));
            texture
        };
        resources.insert(id,texture);
    }
    let mut remap=HashMap::new();
    for (id,image) in resources {
        let temporary=NEXT_IMAGE.fetch_add(1,Ordering::Relaxed);
        renderer.textures.insert(temporary,image);
        remap.insert(id,temporary);
    }
    let result = (|| {
        let crop = source.map(|rect| Clip {path:format::rectangle(rect).into(), fill:0, transform:format::IDENTITY});
        let background = stored.content.background_draw();
        for original in background.iter().chain(&stored.content.operations) {
            let mut request = original.draw.clone();
            request.target = target;
            request.alpha_mode = alpha;
            request.transform = multiply(request.transform, view);
            request.clip = clip;
            for paint in &mut request.paints {
                if paint.brush_type == 2 {
                    paint.image = *remap
                        .get(&paint.image)
                        .ok_or("record image is unavailable")?;
                }
            }
            let clips:Vec<_> = original.clips.iter().chain(crop.iter()).map(|clip| {
                let matrix = Affine::new(multiply(clip.transform,view));
                (matrix * super::layer_ex_draw::bez_path(&clip.path), if clip.fill == 0 { Fill::EvenOdd } else { Fill::NonZero })
            }).collect();
            renderer.draw_layer_ex_clipped(&request, &clips)?;
        }
        Ok(())
    })();
    for id in remap.values() {renderer.textures.remove(id);}
    result
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_replay(
    id: u64,
    target: u64,
    clip: *const i32,
    view: *const f64,
    alpha: u32,
) -> i32 {
    ffi(|| {
        replay(
            id,
            target,
            unsafe { array(clip)? },
            unsafe { array(view)? },
            alpha,
            None,
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_draw(
    id: u64,
    target: u64,
    clip: *const i32,
    view: *const f64,
    alpha: u32,
    source: *const f64,
) -> i32 {
    ffi(|| {
        replay(
            id,
            target,
            unsafe { array(clip)? },
            unsafe { array(view)? },
            alpha,
            Some(unsafe { array(source)? }),
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_clone(id: u64, result: *mut u64) -> i32 {
    ffi(|| unsafe { insert((*get(id)?).clone(), result) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_query(id: u64, result: *mut RecordInfo) -> i32 {
    ffi(|| {
        let stored = get(id)?;
        let r = &stored.content;
        unsafe {
            output(
                result,
                RecordInfo {
                    width: r.width,
                    height: r.height,
                    background_argb: r.background,
                    operation_count: r.operations.len() as u32,
                    bounds: r.bounds,
                    dpi:r.dpi,
                },
            )
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn krkr_layer_ex_record_release(id: u64) {
    let _ = ffi(|| {
        RECORDS.lock().map_err(|e| e.to_string())?.remove(&id);
        Ok(())
    });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn krkr_layer_ex_record_free(bytes: *mut c_void, length: usize) {
    if !bytes.is_null() {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                bytes.cast::<u8>(),
                length,
            )));
        }
    }
}

const _: () = assert!(std::mem::size_of::<RecordInfo>() == 64);

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::KrkrRenderer;
    use bevy::{
        app::PluginsState,
        prelude::*,
        render::renderer::{RenderDevice, RenderQueue},
        window::ExitCondition,
    };
    use krkr_shim::{
        layer_ex_draw::{DrawPaint, PathElement},
        render::RenderCommand,
    };

    #[test]
    fn record_snapshot_survives_replay_and_texture_pool_reuse() {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>(),
        );
        while app.plugins_state() != PluginsState::Ready {
            bevy::tasks::tick_global_task_pools_on_main_thread();
            std::thread::yield_now();
        }
        app.finish();
        app.cleanup();
        super::super::initialize(
            app.world().resource::<RenderDevice>().clone(),
            app.world().resource::<RenderQueue>().clone(),
            None,
        );
        let command = |operation, target| RenderCommand {
            operation,
            target,
            format: 4,
            rect: [0, 0, 8, 8],
            pitch: 32,
            ..unsafe { std::mem::zeroed() }
        };
        let upload = |renderer: &mut KrkrRenderer, id, color: [u8; 4]| {
            let rgba = color.repeat(64);
            let mut c = command(2, id);
            c.data = rgba.as_ptr().cast_mut().cast();
            unsafe { renderer.execute(&c) }.unwrap();
        };
        {
            let mut guard = RENDERER_INSTANCE.lock().unwrap();
            let renderer = guard.as_mut().unwrap();
            unsafe { renderer.execute(&command(0, 1001)) }.unwrap();
            unsafe { renderer.execute(&command(0, 1002)) }.unwrap();
            upload(renderer, 1001, [240, 30, 70, 255]);
        }
        let path = [
            (0, [0., 0., 0., 0., 0., 0.]),
            (1, [8., 0., 0., 0., 0., 0.]),
            (1, [8., 8., 0., 0., 0., 0.]),
            (1, [0., 8., 0., 0., 0., 0.]),
            (3, [0.; 6]),
        ]
        .map(|(verb, points)| PathElement {
            verb,
            points,
            reserved: 0,
        });
        let paint = DrawPaint {
            brush_type: 2,
            image: 1001,
            coordinates: [0., 0., 8., 8.],
            paint_transform: format::IDENTITY,
            width: 1.,
            miter_limit: 10.,
            wrap: 4,
            ..unsafe { std::mem::zeroed() }
        };
        let operation = DrawOperation {
            target: 1002,
            clip: [0, 0, 8, 8],
            transform: format::IDENTITY,
            path: path.as_ptr(),
            path_count: path.len() as u32,
            fill_rule: 0,
            paints: &paint,
            paint_count: 1,
            alpha_mode: 0,
            flags: 1,
            reserved: 0,
            bounds: [0.; 4],
        };
        let mut record = 0;
        let mut copy = 0;
        assert_eq!(
            unsafe { krkr_layer_ex_record_create(8, 8, 0, &mut record) },
            0
        );
        assert_eq!(
            unsafe { krkr_layer_ex_record_append(record, &operation, 3) },
            0
        );
        assert_eq!(unsafe { krkr_layer_ex_record_clone(record, &mut copy) }, 0);
        krkr_layer_ex_record_release(record);
        {
            let mut guard = RENDERER_INSTANCE.lock().unwrap();
            let renderer = guard.as_mut().unwrap();
            upload(renderer, 1001, [0, 0, 255, 255]);
            unsafe { renderer.execute(&command(1, 1001)) }.unwrap();
        }
        for frame in 0..12 {
            replay(copy, 1002, [0, 0, 8, 8], format::IDENTITY, 0, None).unwrap();
            let mut guard = RENDERER_INSTANCE.lock().unwrap();
            let renderer = guard.as_mut().unwrap();
            let mut rgba = [0u8; 256];
            unsafe { renderer.read(1002, 4, 32, rgba.as_mut_ptr()) }.unwrap();
            assert_eq!(
                &rgba[(3 * 8 + 3) * 4..][..4],
                &[240, 30, 70, 255],
                "snapshot changed after pool reuse, frame {frame}"
            );
            renderer.update();
            let mut reused = Vec::new();
            for _ in 0..32 {
                let image = renderer.texture_pool.acquire(&renderer.device, 8, 8);
                renderer.queue.write_texture(
                    image.texture.as_image_copy(),
                    &[0, 0, 255, 255].repeat(64),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(32),
                        rows_per_image: Some(8),
                    },
                    image.texture.size(),
                );
                reused.push(image);
            }
            for image in reused {
                renderer.texture_pool.retire(image);
            }
        }
        krkr_layer_ex_record_release(copy);

        let mut content = Record::new(8, 8, 0xff0000ff).unwrap();
        let mut solid = paint;
        solid.brush_type = 0;
        solid.colors = [0; 2];
        let mut operation = operation;
        operation.paints = &solid;
        operation.transform[4] = 2.5;
        let mut draw = unsafe { krkr_shim::layer_ex_draw::DrawRequest::copy(&operation) }.unwrap();
        draw.flags = 2;
        content.operations.push(RecordedDraw { draw, clips: Vec::new() });
        let encoded = format::encode(&content).unwrap();
        let mut source_copy = 0;
        assert_eq!(
            unsafe { krkr_layer_ex_record_decode(encoded.as_ptr().cast(), encoded.len(), &mut source_copy) },
            0
        );
        for (alpha_mode, edge, inside) in [
            (0, [0, 0, 255, 127], [0; 4]),
            (1, [0, 0, 127, 127], [0; 4]),
            (2, [0, 0, 127, 255], [0, 0, 0, 255]),
        ] {
            replay(source_copy, 1002, [0, 0, 8, 8], format::IDENTITY, alpha_mode, None).unwrap();
            let mut guard = RENDERER_INSTANCE.lock().unwrap();
            let renderer = guard.as_mut().unwrap();
            let mut rgba = [0u8; 256];
            unsafe { renderer.read(1002, 4, 32, rgba.as_mut_ptr()) }.unwrap();
            let pixel = |x: usize| &rgba[(3 * 8 + x) * 4..][..4];
            assert_eq!(pixel(1), &[0, 0, 255, 255]);
            assert_eq!(pixel(3), &inside);
            assert!(pixel(2).iter().zip(edge).all(|(&a, b)| a.abs_diff(b) <= 1), "{:?}", pixel(2));
        }
        krkr_layer_ex_record_release(source_copy);

        let mut guard = RENDERER_INSTANCE.lock().unwrap();
        unsafe { guard.as_mut().unwrap().execute(&command(1, 1002)) }.unwrap();
        guard.as_mut().unwrap().flush();
        *guard = None;
    }
}
