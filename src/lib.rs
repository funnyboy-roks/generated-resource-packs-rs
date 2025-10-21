#![allow(clippy::uninlined_format_args)]
use std::{
    fs::File,
    io::{BufWriter, Cursor, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context;
use image::{DynamicImage, ImageReader};
use prog::Progress;
use reqwest::blocking as reqwest;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, ZipWriter};

pub mod k_means;

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

pub fn fetch_jar() -> anyhow::Result<File> {
    let jar_path = Path::new("client.jar");
    if !std::fs::exists(jar_path)? {
        let res = reqwest::get(MANIFEST_URL)?;
        let json: Manifest = res.json()?;

        let version = json
            .versions
            .into_iter()
            .find(|v| v.kind == "release")
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

pub fn generate_pack(
    pack_name: impl AsRef<str>,
    description: impl AsRef<str>,
    progress: &mut Progress<usize>,
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
    let mut i = 0;
    for entry in WalkDir::new("textures") {
        let entry = entry?;
        if i % 32 == 0 {
            progress.update(i);
        }
        i += 1;
        if entry.path().is_dir() {
            progress.set_status(entry.path().display());
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

    progress.update(i);

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
    // println!(
    //     "Finished generating \"{}\" pack in {}ms",
    //     pack_name,
    //     elapsed.as_millis()
    // );
    progress.set_status(format!(
        "\x1b[32mDone!\x1b[0m in {:?}",
        Duration::from_millis(elapsed.as_millis() as u64)
    ));

    Ok(())
}
