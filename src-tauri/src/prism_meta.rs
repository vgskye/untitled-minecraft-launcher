use std::{collections::HashMap, path::PathBuf};

use anyhow::anyhow;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::api::http::{ClientBuilder, HttpRequestBuilder, ResponseType};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetaIndex {
    pub format_version: u8,
    pub packages: Vec<IndexPackage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexPackage {
    pub name: String,
    pub sha256: String,
    pub uid: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageIndex {
    pub format_version: u8,
    pub name: String,
    pub uid: String,
    pub versions: Vec<PackageVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageVersion {
    pub recommended: bool,
    #[serde(with = "time::serde::iso8601")]
    pub release_time: OffsetDateTime,
    pub requires: Vec<Dependency>,
    pub sha256: String,
    #[serde(rename = "type")]
    pub version_type: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub suggests: Option<String>,
    pub equals: Option<String>,
    pub uid: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub format_version: u8,
    pub order: i32,
    pub name: String,
    pub version: String,
    pub applet_class: Option<String>,
    #[serde(rename = "+tweakers")]
    pub tweakers: Option<Vec<String>>,
    #[serde(rename = "+traits")]
    pub traits: Option<Vec<String>>,
    #[serde(rename = "+jvmArgs")]
    pub jvm_args: Option<Vec<String>>,
    pub jar_mods: Option<Vec<Library>>,
    pub libraries: Option<Vec<Library>>,
    pub maven_files: Option<Vec<Library>>,
    pub main_jar: Option<Library>,
    pub requires: Vec<Dependency>,
    pub conflicts: Vec<Dependency>,
    pub volatile: bool,
    pub asset_index: AssetIndex,
    pub compatible_java_majors: Vec<u32>,
    pub main_class: Option<String>,
    pub minecraft_arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    name: String,
    url: Option<String>,
    extract: Option<ExtractOptions>,
    natives: Option<HashMap<String, String>>,
    rules: Option<Vec<LibraryRule>>,
    downloads: Option<LibraryDownloads>,
    #[serde(rename = "MMC-hint")]
    hint: Option<LibraryHint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryHint {
    AlwaysStale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractOptions {
    exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryRule {
    action: LibraryRuleAction,
    os: Option<LibraryRuleOs>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryRuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryRuleOs {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    artifact: Option<Download>,
    classifiers: Option<HashMap<String, Download>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    id: String,
    sha1: String,
    size: u64,
    total_size: u64,
    url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadedMetaIndex {
    pub index: MetaIndex,
    pub packages: HashMap<String, PackageIndex>,
}

const META_API_BASE: &str = "https://meta.prismlauncher.org/v1/";

pub async fn fetch_meta() -> anyhow::Result<DownloadedMetaIndex> {
    let client = ClientBuilder::new().build()?;
    let index = client
        .send(
            HttpRequestBuilder::new("GET", format!("{}index.json", META_API_BASE))?
                .response_type(ResponseType::Json),
        )
        .await?
        .read()
        .await?;
    let index: MetaIndex = serde_json::from_value(index.data)?;

    let mut packages = HashMap::new();

    for package in &index.packages {
        let downloaded_package = client
            .send(
                HttpRequestBuilder::new(
                    "GET",
                    format!("{}{}/index.json", META_API_BASE, package.uid),
                )?
                .response_type(ResponseType::Json),
            )
            .await?
            .read()
            .await?;
        let downloaded_package: PackageIndex = serde_json::from_value(downloaded_package.data)?;
        packages.insert(package.uid.clone(), downloaded_package);
    }

    Ok(DownloadedMetaIndex { index, packages })
}

const LIBRARY_BASE_URL: &str = "https://libraries.minecraft.net/";

lazy_static::lazy_static! {
    static ref LIBRARY_NAME_REGEX: Regex = Regex::new("(?P<group>[^:@]+):(?P<name>[^:@]+):(?P<version>[^:@]+)(?::(?P<classifier>[^:@]+))?(?:@(?P<extension>[^:@]+))?").unwrap();
}

fn name_to_path(name: &str, classifier: Option<&str>) -> Option<String> {
    let caps = LIBRARY_NAME_REGEX.captures(name)?;
    let ext = caps
        .name("extension")
        .map_or_else(|| "jar", |mat| mat.as_str());
    let classifier = classifier.or_else(|| caps.name("classifier").map(|mat| mat.as_str()));
    let filename = match classifier {
        Some(mat) => format!(
            "{}-{}-{}.{}",
            caps.name("name")?.as_str(),
            caps.name("version")?.as_str(),
            mat,
            ext
        ),
        None => format!(
            "{}-{}.{}",
            caps.name("name")?.as_str(),
            caps.name("version")?.as_str(),
            ext
        ),
    };
    let dir = caps.name("group")?.as_str().replace(".", "/");
    Some(format!(
        "{}/{}/{}/{}",
        dir,
        caps.name("name")?.as_str(),
        caps.name("version")?.as_str(),
        filename,
    ))
}

fn cur_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "x86",
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        "arm" => "arm32",
        _ => "unknown",
    }
}

fn cur_os() -> &'static str {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "osx",
        "windows" => "windows",
        _ => "unknown",
    }
}

fn os_arch() -> String {
    if cur_arch() == "x86" || cur_arch() == "x86_64" {
        cur_os().to_string()
    } else {
        format!("{}-{}", cur_os(), cur_arch())
    }
}

pub async fn download_library(
    base_path: PathBuf,
    library: Library,
) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(rules) = library.rules {
        let mut allowed = false;
        for rule in rules {
            if let Some(os) = rule.os {
                if os.name == os_arch() {
                    allowed = match rule.action {
                        LibraryRuleAction::Allow => true,
                        LibraryRuleAction::Disallow => false,
                    };
                }
            } else {
                allowed = match rule.action {
                    LibraryRuleAction::Allow => true,
                    LibraryRuleAction::Disallow => false,
                };
            }
        }
        if !allowed {
            // We don't need the library
            return Ok(vec![]);
        }
    }
    let mut downloaded = vec![];
    match library.downloads {
        Some(downloads) => {
            if let Some(artifact) = downloads.artifact {
                let mut path = base_path.clone();
                path.push(PathBuf::from(
                    name_to_path(&library.name, None).ok_or(anyhow!("Can't get path from name"))?,
                ));
                crate::storage::get_file(&path, &artifact.url, false, Some(&artifact.sha1)).await?;
                downloaded.push(path);
            }
            if let Some(natives) = library.natives {
                if let Some(native) = natives.get(&os_arch()) {
                    let artifacts = downloads
                        .classifiers
                        .ok_or(anyhow!("Can't get classifiers"))?;
                    let artifact = artifacts.get(native).ok_or(anyhow!("Can't get native"))?;
                    let mut path = base_path.clone();
                    path.push(PathBuf::from(
                        name_to_path(&library.name, Some(native))
                            .ok_or(anyhow!("Can't get path from name"))?,
                    ));
                    crate::storage::get_file(&path, &artifact.url, false, Some(&artifact.sha1))
                        .await?;
                    downloaded.push(path);
                }
            }
        }
        None => {
            let mut url = library.url.map_or(LIBRARY_BASE_URL.to_string(), |url| url);
            if url.ends_with('/') {
                url += &name_to_path(&library.name, None)
                    .ok_or(anyhow!("Can't get path from name"))?;
            }
            let mut path = base_path.clone();
            path.push(PathBuf::from(
                name_to_path(&library.name, None).ok_or(anyhow!("Can't get path from name"))?,
            ));
            crate::storage::get_file(
                &path,
                &url,
                library.hint == Some(LibraryHint::AlwaysStale),
                None,
            )
            .await?;
            downloaded.push(path);
        }
    }
    Ok(downloaded)
}
