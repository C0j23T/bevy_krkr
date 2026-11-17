pub(super) fn supported(style: u32) -> bool {
    matches!(style, 0..=5 | 24..=29 | 48..=50)
}

pub(super) fn foreground(style: u32, x: u32, y: u32) -> bool {
    match style {
        0 => y == 0,
        1 => x == 0,
        2 => (x + y) % 8 == 0,
        3 => x == y,
        4 => x == 0 || y == 0,
        5 => x == y || (x + y) % 8 == 0,
        24 => x % 4 == 0,
        25 => y % 4 == 0,
        26 => x % 2 == 0,
        27 => y % 2 == 0,
        28 => x % 4 < 2,
        29 => y % 4 < 2,
        48 => x % 4 == 0 || y % 4 == 0,
        49 => (x / 2 + y / 2) % 2 == 0,
        50 => (x / 4 + y / 4) % 2 == 0,
        _ => false,
    }
}
