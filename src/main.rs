#![allow(clippy::uninlined_format_args)]
use std::{path::Path, sync::Arc, thread};

use anyhow::Context;
use clap::Parser;
use gen_rp_rs::{
    Version,
    colour::{hsv_to_rgb, rgb_to_hsv, to_8bit},
    extract_jar, generate_pack, modrinth,
};
use image::Rgba;
use prog::{Progress, ProgressGroup};
use tempfile::TempDir;
use walkdir::WalkDir;

// TODO: Pack version from version.json

#[derive(clap::Parser)]
struct Cli {
    #[clap(short, long)]
    slug: Option<String>,
    version: Option<String>,
}

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

    let mut threads = Vec::new();

    let num_files = WalkDir::new(&textures_dir).into_iter().count();
    let prog_group = ProgressGroup::builder()
        .progress_width(80)
        .style(prog::ProgressStyle {
            use_percent: true,
            ..Default::default()
        })
        .build();

    macro_rules! pack {
        ($name: literal, $desc: literal, $($fn: tt)+) => {{
            let prog_group = Arc::clone(&prog_group);
            let textures_dir = textures_dir.path().to_path_buf();
            threads.push(thread::spawn(move || {
                let mut p = Progress::builder(prog_group)
                    .label($name)
                    .init(0)
                    .max(num_files - 1)
                    .build()
                    .unwrap();
                let res = generate_pack($name, $desc, &mut p, &textures_dir, &out_dir, pack_format, $($fn)+);
                match res {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Error while generating pack \"{}\": {:?}", $name, e);
                    }
                }
                p
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
        "1-bit",
        "§6Convert all textures to 1-bit\n§3By: funnyboy_roks",
        |image| {
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

            image
                .pixels_mut()
                .zip(px.into_iter())
                .for_each(|(old, new)| {
                    old.0[0] = new.0[0].clamp(0, 255) as u8;
                    old.0[1] = new.0[1].clamp(0, 255) as u8;
                    old.0[2] = new.0[2].clamp(0, 255) as u8;
                    old.0[3] = new.0[3].clamp(0, 255) as u8;
                });

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

    // pack!(
    //     "K-Means",
    //     "§6K-Means or something\n§3By: funnyboy_roks",
    //     |image| {
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
    // );

    threads.into_iter().for_each(|t| {
        t.join().unwrap();
    });
    prog_group.draw();
    drop(textures_dir);

    let modrinth_token =
        std::env::var("MODRINTH_TOKEN").context("MODRINTH_TOKEN env var not set")?;

    if let Some(slug) = &cli.slug {
        eprintln!("Uploading to Modrinth...");
        modrinth::CreateVersionReq {
            name: &version.id,
            version_number: &version.id,
            changelog: &format!("Update pack for {}", version.id),
            game_versions: &[&version.id],
            version_type: match &*version.kind {
                "snapshot" => modrinth::VersionType::Beta,
                "release" => modrinth::VersionType::Release,
                _ => panic!("Unknown version kind: {}", version.kind),
            },
            status: modrinth::VersionStatus::Listed,
            project_id: slug,
        }
        .send(
            &modrinth_token,
            "Saturation.zip",
            &out_dir.join("Saturation.zip"),
        )
        .context("Creating release")?;
        eprintln!("Done uploading.");
    } else {
        eprintln!("Skipping modrinth upload.");
    }

    Ok(())
}
