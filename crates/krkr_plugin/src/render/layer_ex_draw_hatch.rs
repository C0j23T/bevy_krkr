pub(super) fn supported(style: u32) -> bool {
    style < 53
}

fn bayer(x: u32, y: u32) -> u32 {
    let mut value = 0;
    for bit in 0..3 {
        let a = (x >> bit) & 1;
        let b = (y >> bit) & 1;
        value |= ((a ^ b) * 2 + b) << ((2 - bit) * 2);
    }
    value
}

pub(super) fn foreground(style: u32, x: u32, y: u32) -> bool {
    let down = (x + 8 - y) % 8;
    let up = (x + y) % 8;
    match style {
        0 => y == 0,
        1 => x == 0,
        2 => up == 0,
        3 => down == 0,
        4 => x == 0 || y == 0,
        5 => down == 0 || up == 0,
        6..=17 => {
            let percent = [5, 10, 20, 25, 30, 40, 50, 60, 70, 75, 80, 90][(style - 6) as usize];
            bayer(x, y) * 100 < percent * 64
        }
        18 => down % 4 == 0,
        19 => up % 4 == 0,
        20 => down % 4 < 2,
        21 => up % 4 < 2,
        22 => down < 3,
        23 => up < 3,
        24 => x % 4 == 0,
        25 => y % 4 == 0,
        26 => x % 2 == 0,
        27 => y % 2 == 0,
        28 => x % 4 < 2,
        29 => y % 4 < 2,
        30 => down == 0 && y % 4 < 2,
        31 => up == 0 && y % 4 < 2,
        32 => y % 4 == 0 && (x / 4 + y / 4) % 2 == 0,
        33 => x % 4 == 0 && (x / 4 + y / 4) % 2 == 0,
        34 => (x * 13 + y * 7) % 17 == 0,
        35 => ((x / 2) * 13 + (y / 2) * 7) % 7 == 0,
        36 => y % 4 == x.min(8 - x) % 4,
        37 => y % 4 == [0, 0, 1, 2, 2, 2, 1, 0][x as usize],
        38 => down == 0 || up == 0 && y < 4,
        39 => y % 4 == 0 || (x + (y / 4) * 4) % 8 == 0,
        40 => x % 4 == 0 && y % 4 < 3 || y % 4 == 0 && x % 4 > 1,
        41 => x < 4 && y < 4 || (x + y) % 2 == 0 && (x >= 4 || y >= 4),
        42 => (x % 4 == 0 && y % 4 == 0) || (x % 4 == 1 && y % 4 == 1),
        43 => (x == 0 && y % 2 == 0) || (y == 0 && x % 2 == 0),
        44 => (down == 0 || up == 0) && x % 2 == 0,
        45 => down == 0 || x % 4 == 0 && y % 4 == 0,
        46 => up % 4 < 2 || down % 4 < 2,
        47 => ((x % 4) as i32 - 2).pow(2) + ((y % 4) as i32 - 2).pow(2) <= 4,
        48 => x % 4 == 0 || y % 4 == 0,
        49 => (x / 2 + y / 2) % 2 == 0,
        50 => (x / 4 + y / 4) % 2 == 0,
        51 => (x as i32 - 4).abs() + (y as i32 - 4).abs() == 4,
        52 => (x as i32 - 4).abs() + (y as i32 - 4).abs() <= 3,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_contains_foreground_and_background() {
        for style in 0..53 {
            let count = (0..8)
                .flat_map(|y| (0..8).map(move |x| (x, y)))
                .filter(|&(x, y)| foreground(style, x, y))
                .count();
            assert!(count > 0 && count < 64, "style {style}: {count}");
        }
    }
}
