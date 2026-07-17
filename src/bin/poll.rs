use std::{
    ops::Bound,
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, bail};
use clap::Parser;
use gen_rp_rs::{
    Manifest, Pack, Version, build_packs,
    colour::{hsv_to_rgb, rgb_to_hsv},
    modrinth::{self, CreateVersionReq, VersionStatus, VersionType},
};
use tempfile::TempDir;

fn upload_version(
    modrinth_token: &str,
    slug: &str,
    version: &Version,
    file: &str,
    out_dir: Arc<Path>,
) -> anyhow::Result<()> {
    let already_exists = modrinth::project_has_version(modrinth_token, slug, version)
        .with_context(|| {
            format!(
                "Checking if version for {} already exists on Modrinth",
                version
            )
        })?;

    if already_exists {
        println!("Version for {} already exists on Modrinth", version);
        return Ok(());
    }

    CreateVersionReq {
        name: &version.id,
        version_number: &version.id,
        changelog: &format!("Update pack for {}", version),
        game_versions: &[&version.id],
        version_type: match &*version.kind {
            "snapshot" => VersionType::Beta,
            "release" => VersionType::Release,
            _ => bail!("Unknown version kind: {}", version.kind),
        },
        status: VersionStatus::Listed,
        project_id: slug,
    }
    .send(modrinth_token, file, &out_dir.join(file))
    .context("Creating version")
}

const PACKS: &[Pack] = &[
    Pack {
        name: "Saturation",
        desc: "§6Saturates all textures\n§3By: funnyboy_roks",
        slug: "yTgcjxyL",
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
        slug: "mHNsfZ54",
        func: |image| image.grayscale(),
    },
];

fn publish_version(version: &Version) -> anyhow::Result<()> {
    let modrinth_token =
        std::env::var("MODRINTH_TOKEN").context("MODRINTH_TOKEN env var not set")?;

    let textures_dir = TempDir::new().context("Creating temporary directory for textures")?;
    let out_dir: Arc<Path> = Arc::from(Path::new("out"));

    build_packs(
        version,
        PACKS,
        Arc::from(textures_dir.path()),
        out_dir.clone(),
    )
    .context("Building resource packs")?;

    eprintln!("Uploading to Modrinth...");
    for pack in PACKS {
        upload_version(
            &modrinth_token,
            pack.slug,
            version,
            &format!("{}.zip", pack.name),
            out_dir.clone(),
        )
        .context("Uploading Saturation")?;
    }
    eprintln!("Done uploading.");

    Ok(())
}

fn update_existing() -> anyhow::Result<()> {
    let modrinth_token =
        std::env::var("MODRINTH_TOKEN").context("MODRINTH_TOKEN env var not set")?;
    let textures_dir = TempDir::new().context("Creating temporary directory for textures")?;
    let out_dir: Arc<Path> = Arc::from(Path::new("out"));

    let manifest = Manifest::get().context("Getting manifest")?;

    let latest_mc = manifest.latest_version();
    let version_map = manifest.get_version_map();

    for pack in PACKS {
        eprintln!("Checking {} for needed updates", pack.name);
        let latest_mr = modrinth::project_latest_version(&modrinth_token, pack.slug, &version_map)
            .context("Getting latest modrinth version")?;

        let latest_mr = if let Some(latest_mr) = latest_mr {
            eprintln!("Latest version: {}", latest_mr);
            latest_mr
        } else {
            println!("No versions");
            manifest.versions.first().unwrap()
        };

        if latest_mc <= latest_mr {
            eprintln!("Modrinth up to date");
        }

        eprintln!("Modrinth out of date");
        let mut between = manifest
            .versions
            .range((Bound::Excluded(latest_mr), Bound::Included(latest_mc)))
            .rev()
            .take(5) // upload just the latest 5 versions
            .collect::<Vec<_>>();
        between.reverse(); // ensure we build/upload in the correct order

        for v in between {
            eprintln!("Building for {}", v);
            build_packs(
                v,
                std::slice::from_ref(pack),
                Arc::from(textures_dir.path()),
                out_dir.clone(),
            )
            .context("Building resource packs")?;

            eprintln!("Uploading to Modrinth...");
            upload_version(
                &modrinth_token,
                pack.slug,
                v,
                &format!("{}.zip", pack.name),
                out_dir.clone(),
            )
            .context("Uploading Saturation")?;
            eprintln!("Done uploading.");
        }
    }

    Ok(())
}

#[derive(clap::Parser)]
struct Cli {
    #[clap(short, long, default_value_t = Duration::from_hours(24).into())]
    interval: humantime::Duration,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    update_existing().context("Updating existing packs")?;

    let mut last_version = None::<Version>;
    eprintln!("Polling for updates...");
    loop {
        let start = Instant::now();

        let version = Version::get_latest().context("Fetching latest version")?;
        eprintln!("Latest version: {}", version);

        if last_version.as_ref().is_none_or(|l| *l != version) {
            if let Some(last) = last_version {
                eprintln!("{} -> {}", last, version);
            };

            if let Err(e) = publish_version(&version) {
                eprintln!("Error Publishing version: {:?}", e);
            }

            last_version = Some(version);
        }

        let wait = Duration::from(cli.interval) - start.elapsed();
        eprintln!("Waiting {}", humantime::format_duration(wait));
        thread::sleep(wait);
    }
}
