use std::{collections::HashMap, path::Path};

use ::reqwest::{
    StatusCode,
    blocking::{self as reqwest, Client},
};
use anyhow::{Context, bail};
use lazy_static::lazy_static;
use reqwest::multipart::Form;
use serde::{Deserialize, Serialize};

use crate::Version;

const MODRINTH_API: &str = "https://api.modrinth.com/v2";

lazy_static! {
    static ref CLIENT: Client = Client::builder()
        .user_agent("funnyboy-roks/generated-resource-packs (fbr@fbr.dev)")
        .build()
        .expect("Failed to build client");
}

#[derive(Debug, Deserialize)]
pub struct CreateVersionRes {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Beta,
    Alpha,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Listed,
    Archived,
    Draft,
    Unlisted,
    Scheduled,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionRequestedStatus {
    Listed,
    Archived,
    Draft,
    Unlisted,
}

#[derive(Debug, Serialize)]
pub struct CreateVersionReq<'a> {
    pub name: &'a str,
    pub version_number: &'a str,
    pub changelog: &'a str,
    pub game_versions: &'a [&'a str],
    pub version_type: VersionType,
    pub status: VersionStatus,

    /// The ID of the project this version is for
    pub project_id: &'a str,
}

impl CreateVersionReq<'_> {
    pub fn send(
        self,
        modrinth_token: &str,
        file_name: impl Into<String>,
        file: &Path,
    ) -> anyhow::Result<()> {
        #[derive(Debug, Serialize)]
        struct AdditionalData<'a> {
            #[serde(flatten)]
            req: CreateVersionReq<'a>,
            loaders: &'a [&'a str],
            featured: bool,
            requested_status: Option<VersionRequestedStatus>,
            file_parts: &'a [&'a str],
            dependencies: &'a [&'a str],
        }

        let file_name = file_name.into();

        let data = AdditionalData {
            req: self,
            loaders: &["minecraft"],
            featured: false,
            requested_status: None,
            file_parts: &[&file_name],
            dependencies: &[],
        };

        let response = CLIENT
            .post(format!("{}/version", MODRINTH_API))
            .header("Authorization", modrinth_token)
            .multipart(
                Form::new()
                    .text(
                        "data",
                        serde_json::to_string(&dbg!(data))
                            .expect("This structure can't fail to serialize"),
                    )
                    .file(file_name, file)
                    .with_context(|| format!("Reading {}", file.display()))?,
            )
            .send()?;

        let json: serde_json::Value = match response.status() {
            StatusCode::OK => response.json().context("Parsing response json")?,
            status => {
                bail!(
                    "Failed creating version ({}): {:?}",
                    status,
                    response.json::<serde_json::Value>().ok()
                );
            }
        };

        dbg!(json);

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    game_versions: Vec<String>,
}

pub fn project_has_version(
    modrinth_token: &str,
    slug: &str,
    version: &Version,
) -> anyhow::Result<bool> {
    let req = CLIENT
        .get(format!("{}/project/{}/version", MODRINTH_API, slug))
        .query(&[("game_versions", &version.id)])
        .header("Authorization", modrinth_token);
    let response = req.send()?;

    let json: Vec<ModrinthVersion> = match response.status() {
        StatusCode::OK => response.json().context("Parsing response json")?,
        status => {
            bail!(
                "Failed getting version ({}): {:?}",
                status,
                response.json::<serde_json::Value>().ok()
            );
        }
    };

    if json.is_empty() {
        return Ok(false);
    }

    Ok(json.iter().any(|v| v.game_versions.contains(&version.id)))
}

pub fn project_latest_version<'a>(
    modrinth_token: &str,
    slug: &str,
    versions: &HashMap<&'a str, &'a Version>,
) -> anyhow::Result<Option<&'a Version>> {
    let req = CLIENT
        .get(format!("{}/project/{}/version", MODRINTH_API, slug))
        .header("Authorization", modrinth_token);
    let response = req.send()?;

    let json: Vec<ModrinthVersion> = match response.status() {
        StatusCode::OK => response.json().context("Parsing response json")?,
        status => {
            bail!(
                "Failed getting versions ({}): {:?}",
                status,
                response.json::<serde_json::Value>().ok()
            );
        }
    };

    Ok(json
        .iter()
        .flat_map(|v| v.game_versions.iter())
        .map(|v| &versions[v.as_str()])
        .max()
        .copied())
}
