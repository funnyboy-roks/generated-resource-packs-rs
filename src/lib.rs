#![allow(clippy::uninlined_format_args)]
use std::{
    collections::{BTreeSet, HashMap},
    ffi::OsStr,
    fmt::Display,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Cursor, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, bail, ensure};
use image::{DynamicImage, ImageReader};
use prog::{Progress, ProgressGroup};
use reqwest::blocking as reqwest;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub mod colour;
pub mod k_means;
pub mod modrinth;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
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

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.release_time.cmp(&other.release_time)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(&self.id)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub latest: Latest,
    pub versions: BTreeSet<Version>,
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
    pub client_mappings: Option<DownloadInfo>,
    pub server: DownloadInfo,
    pub server_mappings: Option<DownloadInfo>,
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
pub struct PackMcMetaPack<'a> {
    description: &'a str,
    pack_format: u32,
    supported_formats: Option<SupportedFormats>,
    min_format: u32,
    max_format: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackMcMeta<'a> {
    pub pack: PackMcMetaPack<'a>,
}

impl<'a> PackMcMeta<'a> {
    fn new(description: &'a str, pack_format: u32) -> Self {
        Self {
            pack: PackMcMetaPack {
                description,
                pack_format,
                supported_formats: (pack_format < 65).then_some(SupportedFormats {
                    min_inclusive: pack_format,
                    max_inclusive: pack_format,
                }),
                min_format: pack_format,
                max_format: pack_format,
            },
        }
    }
}

/// `pack_version` in `version.json` in the top-level of a client jar
/// See <https://minecraft.wiki/w/Version.json>
#[derive(Clone, Debug, Deserialize)]
pub struct PackVersion {
    pub resource_major: u32,
    pub resource_minor: u32,
    pub data_major: u32,
    pub data_minor: u32,
}

/// `version.json` in the top-level of a client jar
/// See <https://minecraft.wiki/w/Version.json>
#[derive(Clone, Debug, Deserialize)]
pub struct VersionJson {
    pub id: String,
    pub name: String,
    pub world_version: u32,
    pub series_id: String,
    pub protocol_version: u32,
    pub pack_version: PackVersion,
    pub build_time: String,
    pub java_component: String,
    pub stable: bool,
    pub use_editor: bool,
}

impl Manifest {
    pub fn get() -> anyhow::Result<Self> {
        reqwest::get(MANIFEST_URL)?
            .error_for_status()?
            .json()
            .context("Parsing response json")
    }

    pub fn latest_version(&self) -> &Version {
        self.versions
            .last()
            .expect("There should always be at least one version since 2009")
    }

    pub fn get_version_map(&self) -> HashMap<&str, &Version> {
        self.versions.iter().map(|v| (&*v.id, v)).collect()
    }
}

impl Version {
    pub fn get_latest() -> anyhow::Result<Self> {
        Ok(Manifest::get()?.versions.pop_last().unwrap())
    }

    pub fn get_by_id(id: &str) -> anyhow::Result<Self> {
        Manifest::get()?
            .versions
            .into_iter()
            .find(|v| v.id == id)
            .with_context(|| format!("Unknown version id: {}", id))
    }

    pub fn download_jar(&self, clients_dir: impl AsRef<Path>) -> anyhow::Result<File> {
        let clients_dir = clients_dir.as_ref();

        let jar_path = clients_dir.join(&self.id).with_added_extension("jar");
        if jar_path.try_exists()? {
            println!("{} already exists, skipping download.", jar_path.display());
            return Ok(File::open(&jar_path)?);
        }

        fs::create_dir_all(clients_dir)
            .with_context(|| format!("Creating {} directory", clients_dir.display()))?;

        let res = reqwest::get(&self.url)?;
        let meta: VersionMeta = res.json()?;
        println!("Getting version {}", self.id);

        let mut res = reqwest::get(&meta.downloads.client.url)?;
        let mut jar_file = File::create_new(&jar_path)?;

        io::copy(&mut res, &mut jar_file)
            .with_context(|| format!("Downloading client to {}", jar_path.display()))?;
        println!("Downloaded to {}", jar_path.display());
        drop(jar_file);

        Ok(File::open(&jar_path)?)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn generate_pack(
    pack_name: impl AsRef<str>,
    description: impl AsRef<str>,
    progress: &mut Progress<usize>,
    textures_dir: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
    pack_format: u32,
    f: fn(DynamicImage) -> DynamicImage,
) -> anyhow::Result<()> {
    let start = Instant::now();

    let pack_name = pack_name.as_ref();
    let description = description.as_ref();
    let textures_dir = textures_dir.as_ref();
    let out_dir = out_dir.as_ref();

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let zip_file = out_dir.join(pack_name).with_added_extension("zip");
    let zip_file =
        File::create(&zip_file).with_context(|| format!("Creating file {}", zip_file.display()))?;
    let mut writer = ZipWriter::new(BufWriter::new(zip_file));

    let mut image_buf = Vec::new();
    let mut i = 0;
    for entry in WalkDir::new(textures_dir) {
        if i % 32 == 0 {
            progress.update(i);
        }
        i += 1;

        let entry = entry?;

        if entry.path().is_dir() {
            let full_path_str = entry
                .path()
                .strip_prefix(textures_dir)
                .expect("path is in textures_dir")
                .to_str()
                .expect("all asset paths are valid utf-8");
            progress.set_status(
                full_path_str
                    .strip_prefix(textures_dir.to_str().expect("textures_dir is valid utf-8"))
                    .map(|s| s.trim_start_matches('/'))
                    .unwrap_or(full_path_str),
            );
            continue;
        }

        let path = if entry.file_name() == "pack.png" {
            PathBuf::from_iter(["pack.png"])
        } else {
            PathBuf::from_iter(["assets", "minecraft", "textures"]).join(
                entry
                    .path()
                    .strip_prefix(textures_dir)
                    .expect("Path is in textures_dir"),
            )
        };

        if entry.path().extension().is_some_and(|ext| ext == "png") {
            let image = ImageReader::open(entry.path())
                .with_context(|| format!("Reading image {}", entry.path().display()))?
                .decode()
                .context("Decoding image")?;

            let image = f(image);

            writer.start_file_from_path(path, options)?;
            let mut cursor = Cursor::new(&mut image_buf);
            image.write_to(&mut cursor, image::ImageFormat::Png)?;
            writer.write_all(&image_buf)?;
            image_buf.clear();
        } else {
            writer.start_file_from_path(path, options)?;
            io::copy(&mut File::open(entry.path())?, &mut writer)?;
        }
    }

    let pack_mcmeta = serde_json::to_string_pretty(&PackMcMeta::new(description, pack_format))?;

    writer.start_file("pack.mcmeta", options)?;
    writer.write_all(pack_mcmeta.as_bytes())?;

    writer.finish()?;

    progress.update(i);
    progress.set_status(format!(
        "\x1b[32mDone!\x1b[0m in {:?}",
        Duration::from_millis(start.elapsed().as_millis() as u64)
    ));

    Ok(())
}

pub fn extract_jar(jar: impl Read + Seek, textures_dir: impl AsRef<Path>) -> anyhow::Result<u32> {
    let textures_dir = textures_dir.as_ref();
    let mut dec = ZipArchive::new(BufReader::new(jar))?;
    let mut pack_format = None::<u32>;

    for i in 0..dec.len() {
        let mut file = dec.by_index(i)?;
        let path1 = file
            .enclosed_name()
            .with_context(|| format!("Malformed path in jar: {}", file.name()))?;

        if path1 == *"version.json" {
            let version_json: VersionJson = serde_json::from_reader(file)?;
            pack_format = Some(version_json.pack_version.resource_major);
            ensure!(
                version_json.pack_version.resource_minor == 0,
                "resource_minor must be 0" // we assume this
            );

            continue;
        }

        let allowed_extensions = ["png", "mcmeta"].map(OsStr::new);

        if path1
            .extension()
            .is_none_or(|ext| !allowed_extensions.contains(&ext))
        {
            continue;
        }

        let path = if path1 != Path::new("pack.png") {
            if !path1.starts_with("assets/minecraft/textures") {
                continue;
            }
            let mut path = textures_dir.to_path_buf();
            path.extend(path1.components().skip(3));
            path
        } else {
            textures_dir.join("pack.png")
        };

        let parent = path
            .parent()
            .with_context(|| format!("path contains no parent: {}", path.display()))?;
        fs::create_dir_all(parent).with_context(|| format!("Making dir {}", parent.display()))?;

        let mut out =
            File::create(&path).with_context(|| format!("Creating file {}", path.display()))?;

        io::copy(&mut file, &mut out).with_context(|| format!("Saving {}", path.display()))?;
    }

    let Some(pack_format) = pack_format else {
        bail!("Unable to determine pack format");
    };

    Ok(pack_format)
}

pub struct Pack<'a> {
    pub name: &'a str,
    pub desc: &'a str,
    pub slug: &'a str,
    pub func: fn(DynamicImage) -> DynamicImage,
}

pub fn build_packs(
    version: &Version,
    packs: &[Pack<'_>],
    textures_dir: Arc<Path>,
    out_dir: Arc<Path>,
) -> anyhow::Result<()> {
    fs::remove_dir_all(&textures_dir).context("Removing textures dir")?;
    fs::create_dir_all(&textures_dir).context("Creating textures dir")?;

    let jar_file = version.download_jar("clients")?;
    let pack_format = extract_jar(jar_file, &textures_dir).context("Extracting JAR")?;

    let mut threads = Vec::new();

    fs::create_dir_all(&out_dir)
        .with_context(|| format!("Creating directory {}", out_dir.display()))?;

    let num_files = WalkDir::new(&textures_dir).into_iter().count();
    let prog_group = ProgressGroup::builder()
        .width(130)
        .progress_width(80)
        .style(prog::ProgressStyle {
            use_percent: true,
            ..Default::default()
        })
        .build();

    for pack in packs {
        let prog_group = Arc::clone(&prog_group);
        let textures_dir = textures_dir.clone();
        let out_dir = out_dir.clone();
        let pack_name = String::from(pack.name);
        let pack_desc = String::from(pack.desc);
        let pack_func = pack.func;
        threads.push(thread::spawn(move || {
            let mut p = Progress::builder(prog_group)
                .label(&pack_name)
                .init(0)
                .max(num_files - 1)
                .build()
                .unwrap();
            let res = generate_pack(
                &pack_name,
                pack_desc,
                &mut p,
                &textures_dir,
                &out_dir,
                pack_format,
                pack_func,
            );
            match res {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("Error while generating pack \"{}\": {:?}", pack_name, e);
                }
            }
        }));
    }

    threads
        .into_iter()
        .try_for_each(JoinHandle::join)
        .expect("Waiting for threads to finish");

    prog_group.draw();

    Ok(())
}
