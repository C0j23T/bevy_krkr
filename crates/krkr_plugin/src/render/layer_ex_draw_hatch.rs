const COVERAGE: &[u8; 53 * 64] = include_bytes!("layer_ex_draw_hatch.bin");

pub(super) fn supported(style: u32) -> bool {
    style < 53
}

pub(super) fn pixel(style: u32, x: u32, y: u32, colors: [u32; 2]) -> [u8; 4] {
    let coverage = u32::from(COVERAGE[(style * 64 + y * 8 + x) as usize]);
    let channels = colors.map(|argb| {
        let [a, r, g, b] = argb.to_be_bytes().map(u32::from);
        [r * a, g * a, b * a, a * 255]
    });
    std::array::from_fn(|i| {
        let value = channels[0][i] * coverage + channels[1][i] * (255 - coverage);
        ((value + 32512) / 65025) as u8
    })
}
