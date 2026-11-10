use krkr_shim::ffi::{
    KRKR_PNG_RGBA, KrkrPngInfo, SaveOptions, krkr_png_decode, krkr_png_free, krkr_png_info,
    krkr_save_image_cancel, krkr_save_image_encode, krkr_save_image_poll, krkr_save_image_start,
};
use std::ffi::c_void;
fn options(format: u32) -> SaveOptions {
    SaveOptions {
        format,
        level: -1,
        meta: KrkrPngInfo::default(),
        tags: std::ptr::null(),
        tags_len: 0,
    }
}
fn encode(pixels: &[u8], w: u32, h: u32, options: &SaveOptions) -> Option<Vec<u8>> {
    let mut bytes = std::ptr::null_mut::<c_void>();
    let mut len = 0;
    if unsafe {
        krkr_save_image_encode(
            pixels.as_ptr().cast(),
            pixels.len(),
            w,
            h,
            options,
            &mut bytes,
            &mut len,
        )
    } != 0
    {
        return None;
    }
    let out = unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), len) }.to_vec();
    unsafe { krkr_png_free(bytes, len) };
    Some(out)
}
#[test]
fn png_levels_metadata_and_pixels() {
    let pixels = [21, 45, 91, 127, 0, 30, 40, 0];
    let mut options = options(0);
    options.meta.has_offset = 1;
    options.meta.offset_x = -3;
    options.meta.offset_y = 9;
    options.meta.offset_unit = 1;
    options.meta.has_resolution = 1;
    options.meta.resolution_x = 72;
    options.meta.resolution_y = 144;
    options.meta.resolution_unit = 1;
    options.meta.has_vpage = 1;
    options.meta.vpage_width = 640;
    options.meta.vpage_height = 480;
    for level in -1..=9 {
        options.level = level;
        let bytes = encode(&pixels, 2, 1, &options).unwrap();
        let mut meta = KrkrPngInfo::default();
        assert_eq!(
            unsafe { krkr_png_info(bytes.as_ptr().cast(), bytes.len(), &mut meta) },
            0
        );
        assert_eq!((meta.offset_x, meta.offset_y, meta.offset_unit), (-3, 9, 1));
        assert_eq!(
            (meta.resolution_x, meta.resolution_y, meta.resolution_unit),
            (72, 144, 1)
        );
        assert_eq!((meta.vpage_width, meta.vpage_height), (640, 480));
        let mut out = [0u8; 8];
        assert_eq!(
            unsafe {
                krkr_png_decode(
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    KRKR_PNG_RGBA,
                    -1,
                    out.as_mut_ptr().cast(),
                    8,
                )
            },
            8
        );
        assert_eq!(pixels, out);
    }
    options.level = 10;
    assert!(encode(&pixels, 2, 1, &options).is_none());
    assert!(
        encode(
            &pixels,
            3,
            1,
            &SaveOptions {
                level: 0,
                ..options
            }
        )
        .is_none()
    );
}
#[test]
fn province_palette_and_transparency() {
    let bytes = encode(&[0, 255, 5], 3, 1, &options(2)).unwrap();
    let mut out = [0u8; 12];
    assert_eq!(
        unsafe {
            krkr_png_decode(
                bytes.as_ptr().cast(),
                bytes.len(),
                KRKR_PNG_RGBA,
                -1,
                out.as_mut_ptr().cast(),
                12,
            )
        },
        12
    );
    assert_eq!(&out[..8], &[0, 0, 0, 0, 255, 255, 255, 255]);
    assert_eq!(&out[8..], &[64, 0, 128, 255]);
}
#[test]
fn async_snapshot_and_stop() {
    let mut pixels = vec![33u8; 64 * 64 * 4];
    let opt = options(0);
    let id = unsafe { krkr_save_image_start(pixels.as_ptr().cast(), pixels.len(), 64, 64, &opt) };
    assert_ne!(id, 0);
    pixels.fill(0);
    let mut progress = 0;
    let mut bytes = std::ptr::null_mut();
    let mut len = 0;
    loop {
        let status = unsafe { krkr_save_image_poll(id, &mut progress, &mut bytes, &mut len) };
        if status != 0 {
            assert_eq!(status, 1);
            break;
        }
        std::thread::yield_now();
    }
    let mut out = vec![0u8; pixels.len()];
    assert_eq!(
        unsafe {
            krkr_png_decode(
                bytes,
                len,
                KRKR_PNG_RGBA,
                -1,
                out.as_mut_ptr().cast(),
                out.len(),
            )
        },
        out.len()
    );
    assert!(out.iter().all(|&v| v == 33));
    unsafe { krkr_png_free(bytes, len) };
    let id = unsafe { krkr_save_image_start(pixels.as_ptr().cast(), pixels.len(), 64, 64, &opt) };
    krkr_save_image_cancel(id, 1);
    assert_eq!(
        unsafe { krkr_save_image_poll(id, &mut progress, &mut bytes, &mut len) },
        -1
    );
}

#[test]
fn tlg_header_and_block_table() {
    let pixels = vec![0u8; 9 * 7 * 4];
    let bytes = encode(&pixels, 9, 7, &options(1)).unwrap();
    assert_eq!(&bytes[..11], b"TLG5.0\0raw\x1a");
    assert_eq!(bytes[11], 4);
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 9);
    let sizes: usize = bytes[24..32]
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .sum();
    assert_eq!(sizes, bytes.len() - 32);
    assert!(sizes < pixels.len());
}
