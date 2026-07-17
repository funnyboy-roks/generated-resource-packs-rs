use std::{path::Path, sync::Arc, thread::JoinHandle};

use anyhow::Context;
use clap::Parser;
use gen_rp_rs::{
    Pack, Version,
    colour::{hsv_to_rgb, rgb_to_hsv, to_8bit},
    extract_jar, generate_pack,
};
use image::Rgba;
use prog::{Progress, ProgressGroup};
use tempfile::TempDir;
use walkdir::WalkDir;

#[derive(clap::Parser)]
struct Cli {
    #[clap(short, long)]
    slug: Option<String>,
    version: Option<String>,
}

const PACKS: &[Pack] = &[
    Pack {
        name: "Saturation",
        desc: "§6Saturates all textures\n§3By: funnyboy_roks",
        slug: "unused",
        func: |image| {
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
    },
    Pack {
        name: "Greyscale",
        desc: "§7All Textures are Greyscale\n§3By: funnyboy_roks",
        slug: "unused",
        func: |image| image.grayscale(),
    },
    Pack {
        name: "Invert",
        desc: "§6All Textures are Inverted\n§3By: funnyboy_roks",
        slug: "unused",
        func: |mut image| {
            image.invert();
            image
        },
    },
    Pack {
        name: "1-bit",
        desc: "§6Convert all textures to 1-bit\n§3By: funnyboy_roks",
        slug: "unused",
        func: |image| {
            let mut image = image.into_rgba8();

            let (width, height) = image.dimensions();
            let (width, height) = (width as usize, height as usize);
            let mut px: Vec<_> = image
                .pixels()
                .map(|p| Rgba::<i32>([p.0[0] as _, p.0[1] as _, p.0[2] as _, p.0[3] as _]))
                .collect();

            for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
                let old = px[y * width + x];
                let new = to_8bit(old);
                px[y * width + x] = new;
                let quant = [
                    old.0[0] - new.0[0],
                    old.0[1] - new.0[1],
                    old.0[2] - new.0[2],
                    old.0[3] - new.0[3],
                ];
                // dbg!(old, new, diff);

                let mut add = |dx, dy, numerator, denominator| {
                    let x = x.checked_add_signed(dx)?;
                    if x >= width {
                        return None;
                    };
                    let y = y.checked_add_signed(dy)?;
                    if y >= height {
                        return None;
                    };
                    let a = &mut px[y * width + x];
                    a.0[0] += quant[0] * numerator / denominator;
                    a.0[1] += quant[1] * numerator / denominator;
                    a.0[2] += quant[2] * numerator / denominator;
                    a.0[3] += quant[3] * numerator / denominator;
                    Some(())
                };

                add(1, 0, 7, 16);
                add(-1, 1, 3, 16);
                add(0, 1, 5, 16);
                add(1, 1, 1, 16);
            }

            image.pixels_mut().zip(px).for_each(|(old, new)| {
                old.0[0] = new.0[0].clamp(0, 255) as u8;
                old.0[1] = new.0[1].clamp(0, 255) as u8;
                old.0[2] = new.0[2].clamp(0, 255) as u8;
                old.0[3] = new.0[3].clamp(0, 255) as u8;
            });

            image.into()
        },
    },
    Pack {
        name: "Average",
        desc: "§6Averages all textures\n§3By: funnyboy_roks",
        slug: "unused",
        func: |image| {
            let mut image = image.into_rgba8();

            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
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
    },
    Pack {
        name: "8bit",
        desc: "§6All textures are 8-bit\n§3By: funnyboy_roks",
        slug: "unused",
        func: |image| {
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
    },
    // Pack {
    //     name: "K-Means",
    //     desc: "§6K-Means or something\n§3By: funnyboy_roks",
    //     slug: "unused",
    //     func: |image| {
    //         let clusters = k_means(
    //             4,
    //             &image
    //                 .pixels()
    //                 .map(|(_, _, x)| Rgb::<u8>([x[0], x[1], x[2]]))
    //                 .collect::<Vec<_>>(),
    //         );
    //         let mut image = image.into_rgba8();

    //         for px in image.pixels_mut() {
    //             if px[3] > 0 {
    //                 let next = closest(Rgb::<u8>([px[0], px[1], px[2]]), &clusters);
    //                 px.0[..3].copy_from_slice(&next.0);
    //             }
    //         }

    //         image.into()
    //     },
    // },
];

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let version = if let Some(id) = cli.version {
        Version::get_by_id(&id).context("Fetching version")?
    } else {
        Version::get_latest().context("Getting latest version")?
    };

    let jar_file = version.download_jar("clients")?;

    let textures_dir = TempDir::new().context("Creating temporary directory for textures")?;
    let out_dir = Path::new("out");

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Creating dir: {}", out_dir.display()))?;

    let pack_format = extract_jar(jar_file, dbg!(&textures_dir)).context("Extracting JAR")?;

    let num_files = WalkDir::new(&textures_dir).into_iter().count();
    let prog_group = ProgressGroup::builder()
        .progress_width(80)
        .style(prog::ProgressStyle {
            use_percent: true,
            ..Default::default()
        })
        .build();

    let textures_dir_arc = Arc::new(textures_dir.path().to_path_buf());
    let threads = PACKS.iter().map(|p| {
        let prog_group = prog_group.clone();
        let textures_dir = textures_dir_arc.clone();
        std::thread::spawn(move || {
            let mut prog = Progress::builder(prog_group)
                .label(p.name)
                .init(0)
                .max(num_files - 1)
                .build()
                .unwrap();
            let res = generate_pack(
                p.name,
                p.desc,
                &mut prog,
                &*textures_dir,
                out_dir,
                pack_format,
                p.func,
            );
            match res {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("Error while generating pack \"{}\": {:?}", p.name, e);
                }
            }
        })
    });

    threads.into_iter().try_for_each(JoinHandle::join).unwrap();
    prog_group.draw();

    Ok(())
}
