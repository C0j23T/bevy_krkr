use krkr_shim::ffi::{krkr_image_operation, krkr_octet_vector_sum};
#[test]
fn crop_diff_and_channels() {
    let mut pixels = vec![0u8; 4 * 3 * 4];
    pixels[20..24].copy_from_slice(&[20, 30, 40, 127]);
    let mut args = [0i64; 6];
    let mut values = [0i64; 7];
    let mut bytes = std::ptr::null_mut();
    let mut len = 0;
    let status = unsafe {
        krkr_image_operation(
            0,
            pixels.as_mut_ptr().cast(),
            pixels.len(),
            4,
            3,
            std::ptr::null(),
            0,
            0,
            0,
            args.as_ptr(),
            values.as_mut_ptr(),
            &mut bytes,
            &mut len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(&values[..5], &[1, 1, 1, 1, 1]);
    let base = pixels.clone();
    args[0] = -1;
    args[1] = 0xff112233;
    pixels[23] = 128;
    assert_eq!(
        unsafe {
            krkr_image_operation(
                3,
                pixels.as_mut_ptr().cast(),
                pixels.len(),
                4,
                3,
                base.as_ptr().cast(),
                base.len(),
                4,
                3,
                args.as_ptr(),
                values.as_mut_ptr(),
                &mut bytes,
                &mut len,
            )
        },
        0
    );
    assert_eq!(values[4], 1);
    assert_eq!(&pixels[20..24], &[0x33, 0x22, 0x11, 255]);
}
#[test]
fn vector_sums_and_mismatched_sizes() {
    let a = [1u8, 2, 3];
    let b = [3u8, 2, 1];
    let mut v = [0i64; 7];
    assert_eq!(
        unsafe {
            krkr_octet_vector_sum(a.as_ptr().cast(), 3, b.as_ptr().cast(), 3, v.as_mut_ptr())
        },
        0
    );
    assert_eq!(v, [10, 8, 14, 14, 6, 6, 2]);
    assert_eq!(
        unsafe {
            krkr_octet_vector_sum(a.as_ptr().cast(), 3, b.as_ptr().cast(), 2, v.as_mut_ptr())
        },
        -1
    );
}
