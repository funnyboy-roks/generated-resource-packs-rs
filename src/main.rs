#![allow(clippy::uninlined_format_args)]
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    thread,
};

use anyhow::Context;
use gen_rp_rs::{
    fetch_jar, generate_pack,
    k_means::{closest, k_means},
};
use image::{GenericImageView, Rgb};
use zip::ZipArchive;

fn rgb_to_hsv([r, g, b]: &[u8; 3]) -> [f32; 3] {
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

fn main() -> anyhow::Result<()> {
    let mut jar_file = fetch_jar()?;

    let mut dec = ZipArchive::new(BufReader::new(&mut jar_file))?;

    for i in 0..dec.len() {
        let mut file = dec.by_index(i)?;
        let path1 = file
            .enclosed_name()
            .with_context(|| format!("Malformed path in jar: {}", file.name()))?;

        if path1
            .file_name()
            .is_none_or(|n| !n.to_string_lossy().ends_with(".png"))
        {
            continue;
        }

        let path = if path1 != Path::new("pack.png") {
            path1.components().skip(2).collect::<PathBuf>()
        } else {
            PathBuf::from_iter(["textures", "pack.png"])
        };

        let parent = path
            .parent()
            .with_context(|| format!("path contains no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Making dir {}", parent.display()))?;

        let mut out =
            File::create(&path).with_context(|| format!("Creating file {}", path.display()))?;

        std::io::copy(&mut file, &mut out).with_context(|| format!("Saving {}", path.display()))?;
    }

    let zip = true;
    let mut threads = Vec::new();

    macro_rules! pack {
        ($name: literal, $desc: literal, $($fn: tt)+) => {{
            threads.push(thread::spawn(move || {
                let res = generate_pack($name, $desc, zip, $($fn)+);
                match res {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Error while generating pack \"{}\": {:?}", $name, e);
                    }
                }
            }));
        }};
    }

    pack!(
        "Greyscale",
        "§7All Textures are Greyscale\n§3By: funnyboy_roks",
        |image| image.grayscale()
    );

    pack!(
        "Invert",
        "§6All Textures are Inverted\n§3By: funnyboy_roks",
        |mut image| {
            image.invert();
            image
        }
    );

    pack!(
        "Saturation",
        "§6Saturates all textures\n§3By: funnyboy_roks",
        |image| {
            let mut image = image.into_rgba8();

            let (width, height) = image.dimensions();
            for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
                let px = image.get_pixel_mut(x, y);

                let mut hsv = rgb_to_hsv(&[px[0], px[1], px[2]]);
                hsv[1] = (hsv[1] * 2.).min(1.);
                let rgb = hsv_to_rgb(hsv);

                px.0[..3].copy_from_slice(&rgb);
            }

            image.into()
        },
    );

    pack!(
        "Average",
        "§6Averages all textures\n§3By: funnyboy_roks",
        |image| {
            let mut image = image.into_rgba8();

            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut i = 0u32;

            let (width, height) = image.dimensions();

            for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
                let px = image.get_pixel(x, y);

                if px[3] > 0 {
                    r += px[0] as u32;
                    g += px[1] as u32;
                    b += px[2] as u32;
                    i += 1;
                }
            }

            if i == 0 {
                return image.into();
            }

            let r = (r / i) as u8;
            let g = (g / i) as u8;
            let b = (b / i) as u8;

            for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
                let px = image.get_pixel_mut(x, y);

                if px[3] > 0 {
                    px[0] = r;
                    px[1] = g;
                    px[2] = b;
                }
            }

            image.into()
        },
    );

    pack!(
        "8bit",
        "§6All textures are 8-bit\n§3By: funnyboy_roks",
        |image| {
            let mut image = image.into_rgba8();

            let (width, height) = image.dimensions();
            for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
                let px = image.get_pixel_mut(x, y);

                px[0] = (px[0] / 32) * 32;
                px[1] = (px[1] / 32) * 32;
                px[2] = (px[2] / 64) * 64;
            }

            image.into()
        },
    );

    pack!(
        "K-Means",
        "§6K-Means or something\n§3By: funnyboy_roks",
        |image| {
            let clusters = k_means(
                4,
                &image
                    .pixels()
                    .map(|(_, _, x)| Rgb::<u8>([x[0], x[1], x[2]]))
                    .collect::<Vec<_>>(),
            );
            let mut image = image.into_rgba8();

            for px in image.pixels_mut() {
                if px[3] > 0 {
                    let next = closest(Rgb::<u8>([px[0], px[1], px[2]]), &clusters);
                    px.0[..3].copy_from_slice(&next.0);
                }
            }

            image.into()
        },
    );

    for t in threads {
        t.join().unwrap();
    }

    Ok(())
}
