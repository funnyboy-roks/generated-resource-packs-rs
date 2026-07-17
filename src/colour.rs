use image::Rgba;

pub fn to_8bit(rgb: Rgba<i32>) -> Rgba<i32> {
    Rgba([
        (rgb.0[0] / 32) * 32,
        (rgb.0[1] / 32) * 32,
        (rgb.0[2] / 64) * 64,
        rgb.0[3],
    ])
}

pub fn rgb_to_hsv([r, g, b]: &[u8; 3]) -> [f32; 3] {
    let rp = *r as f32 / 255.;
    let gp = *g as f32 / 255.;
    let bp = *b as f32 / 255.;

    let c_max = rp.max(gp).max(bp);
    let c_min = rp.min(gp).min(bp);
    let delta = c_max - c_min;

    let h = if delta == 0. {
        0.
    } else if c_max == rp {
        60. * (((gp - bp) / delta) % 6.)
    } else if c_max == gp {
        60. * ((bp - rp) / delta + 2.)
    } else if c_max == bp {
        60. * ((rp - gp) / delta + 4.)
    } else {
        unreachable!()
    };

    let s = if c_max == 0. { 0. } else { delta / c_max };
    let v = c_max;

    [h, s, v]
}

// https://docs.rs/hsv/latest/hsv/fn.hsv_to_rgb.html
pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [u8; 3] {
    fn is_between(value: f32, min: f32, max: f32) -> bool {
        min <= value && value < max
    }

    let c = v * s;
    let h = h / 60.0;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if is_between(h, 0.0, 1.0) {
        (c, x, 0.0)
    } else if is_between(h, 1.0, 2.0) {
        (x, c, 0.0)
    } else if is_between(h, 2.0, 3.0) {
        (0.0, c, x)
    } else if is_between(h, 3.0, 4.0) {
        (0.0, x, c)
    } else if is_between(h, 4.0, 5.0) {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}
