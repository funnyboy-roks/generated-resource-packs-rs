#![allow(clippy::uninlined_format_args)]
use std::{
    fs::File,
    io::{BufReader, BufWriter, Cursor, Write},
    path::{Path, PathBuf},
    thread,
    time::Instant,
};

use anyhow::Context;
use image::{DynamicImage, ImageReader};
use reqwest::blocking as reqwest;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    pub sha1: String,
    pub compliance_level: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<Version>,
}

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: u64,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Downloads {
    pub client: DownloadInfo,
    pub client_mappings: DownloadInfo,
    pub server: DownloadInfo,
    pub server_mappings: DownloadInfo,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub asset_index: AssetIndex,
    pub downloads: Downloads,
}

#[derive(Clone, Debug, Serialize)]
pub struct SupportedFormats {
    min_inclusive: u32,
    max_inclusive: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Pack<'a> {
    description: &'a str,
    pack_format: u32,
    supported_formats: SupportedFormats,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackMcMeta<'a> {
    pub pack: Pack<'a>,
}

fn fetch_jar() -> anyhow::Result<File> {
    let jar_path = Path::new("client.jar");
    if !std::fs::exists(jar_path)? {
        let res = reqwest::get(MANIFEST_URL)?;
        let json: Manifest = res.json()?;

        let version = json
            .versions
            .first()
            .context("No versions found at manifest URL")?;

        let res = reqwest::get(&version.url)?;
        let meta: VersionMeta = res.json()?;
        println!("Getting version {}", version.id);

        let mut res = reqwest::get(&meta.downloads.client.url)?;
        let mut jar_file = File::create_new(jar_path)?;

        std::io::copy(&mut res, &mut jar_file).context("downloading client.jar")?;
        println!("Downloaded to {}", jar_path.display());
        drop(jar_file);
    } else {
        println!("{} already exists, skipping download.", jar_path.display());
    }

    Ok(File::open(jar_path)?)
}

fn generate_pack(
    pack_name: impl AsRef<str>,
    description: impl AsRef<str>,
    zip: bool,
    f: fn(DynamicImage) -> DynamicImage,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let pack_name = pack_name.as_ref();
    let description = description.as_ref();
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut writer = if zip {
        let path = format!("{}.zip", pack_name);
        let f = File::create(&path).with_context(|| format!("Creating file {}", &path))?;
        let f = BufWriter::new(f);
        Some(ZipWriter::new(f))
    } else {
        None
    };

    let mut image_buf = Vec::new();
    for entry in WalkDir::new("textures") {
        let entry = entry?;
        if entry.path().is_dir() {
            continue;
        }
        anyhow::ensure!(entry.file_name().as_encoded_bytes().ends_with(b".png"));

        let image = ImageReader::open(entry.path())
            .with_context(|| format!("Reading image {}", entry.path().display()))?
            .decode()
            .context("Decoding image")?;

        let image = f(image);

        let path = if entry.file_name().as_encoded_bytes() == b"pack.png" {
            if writer.is_some() {
                PathBuf::from_iter(["pack.png"])
            } else {
                PathBuf::from_iter([pack_name, "pack.png"])
            }
        } else {
            let mut path = if writer.is_some() {
                PathBuf::from_iter(&["assets", "minecraft"])
            } else {
                PathBuf::from_iter(&[pack_name, "assets", "minecraft"])
            };
            path.push(entry.path());
            path
        };

        if let Some(ref mut writer) = writer {
            writer.start_file_from_path(path, options)?;
            let mut cursor = Cursor::new(&mut image_buf);
            image.write_to(&mut cursor, image::ImageFormat::Png)?;
            writer.write_all(&image_buf)?;
            image_buf.clear();
        } else {
            let parent = path
                .parent()
                .with_context(|| format!("path contains no parent: {}", path.display()))?;
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Making dir {}", parent.display()))?;

            image
                .save(&path)
                .with_context(|| format!("Saving image: {}", path.display()))?;
        }
    }

    let pack_mcmeta = serde_json::to_string_pretty(&PackMcMeta {
        pack: Pack {
            description,
            pack_format: 64,
            supported_formats: SupportedFormats {
                min_inclusive: 3,
                max_inclusive: 64,
            },
        },
    })?;
    if let Some(ref mut writer) = writer {
        writer.start_file("pack.mcmeta", options)?;
        writer.write_all(pack_mcmeta.as_bytes())?;
    } else {
        std::fs::write(PathBuf::from_iter([pack_name, "pack.mcmeta"]), pack_mcmeta)
            .context("Writing pack.mcmeta")?;
    }

    let elapsed = start.elapsed();

    if let Some(writer) = writer {
        writer.finish()?;
    }
    println!(
        "Finished generating \"{}\" pack in {}ms",
        pack_name,
        elapsed.as_millis()
    );

    Ok(())
}

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
        "greyscale",
        "§7All Textures are Greyscale\n§3By: funnyboy_roks",
        |image| image.grayscale()
    );

    pack!(
        "saturation",
        "§7Saturates all textures\n§3By: funnyboy_roks",
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

    for t in threads {
        t.join().unwrap();
    }

    Ok(())
}
