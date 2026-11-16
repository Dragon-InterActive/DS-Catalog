//! Builds `catalog.json` from `programs.toml` and the GitHub releases of
//! the programs.
//!
//! ```toml
//! [[program]]
//! id = "ct-editor"
//! name = "CT-Editor"
//! description = "Visueller Editor für Cargo.toml-Dateien"
//! homepage = "https://github.com/Freya-TDE/CT-E"
//! license = "MIT OR Apache-2.0"
//! repository = "Freya-TDE/CT-E"          # GitHub releases
//! tag-prefix = "v"                       # default "v"
//! formula = "freya-tde/tap/ct-editor"    # macOS via Homebrew (optional)
//! releases = 1                           # newest releases to list (default 1)
//! ```
//!
//! Release assets are found by name: `<id>-<version>-<platform>.tar.gz` or
//! `.zip` (platform `<os>-<arch>`, `arm64` counts as `aarch64`) and
//! `<id>-<version>-skills.tar.gz` for a skill pack. With a formula, macOS
//! assets are ignored and macOS gets `brew` entries. Checksums come from
//! the asset's `digest`, else the asset is downloaded and hashed.
//!
//! *Katalog aus `programs.toml` und den GitHub-Releases bauen.*

use std::collections::BTreeMap;
use std::io::Read;

use ds_catalog::catalog::{Artifact, Catalog, Download, FORMAT, Program, Release};
use ds_catalog::{platform, sign};
use semver::Version;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(rename = "program", default)]
    pub programs: Vec<ProgramSource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProgramSource {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    pub license: String,
    /// `owner/name` on GitHub.
    pub repository: String,
    #[serde(default = "default_prefix")]
    pub tag_prefix: String,
    pub formula: Option<String>,
    #[serde(default = "default_releases")]
    pub releases: usize,
}

fn default_prefix() -> String {
    "v".into()
}

fn default_releases() -> usize {
    1
}

/// The GitHub API answer for one release (only what is used).
#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// Talks to the GitHub API (`api`, e.g. `https://api.github.com`).
pub struct GitHub {
    pub api: String,
    pub token: Option<String>,
}

impl GitHub {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let mut request = ureq::get(url)
            .header("User-Agent", "ds-sign")
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let response = request.call().map_err(|e| format!("{url}: {e}"))?;
        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{url}: {e}"))?;
        Ok(bytes)
    }

    fn releases(&self, repository: &str) -> Result<Vec<GhRelease>, String> {
        let url = format!("{}/repos/{repository}/releases?per_page=100", self.api);
        serde_json::from_slice(&self.get(&url, 32 << 20)?).map_err(|e| format!("{url}: {e}"))
    }

    fn sha256(&self, asset: &GhAsset) -> Result<String, String> {
        if let Some(hex) = asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
        {
            return Ok(hex.to_ascii_lowercase());
        }
        let bytes = self.get(&asset.browser_download_url, asset.size + 1)?;
        Ok(sign::sha256(&bytes))
    }
}

/// What an asset name means for program `id` in `version`.
#[derive(Debug, PartialEq, Eq)]
enum AssetKind {
    Platform(String),
    Skills,
}

fn classify(name: &str, id: &str, version: &Version) -> Option<AssetKind> {
    let rest = name.strip_prefix(&format!("{id}-{version}-"))?;
    if rest == "skills.tar.gz" {
        return Some(AssetKind::Skills);
    }
    let platform = rest
        .strip_suffix(".tar.gz")
        .or_else(|| rest.strip_suffix(".zip"))?;
    let platform = platform.replace("-arm64", "-aarch64");
    platform::is_valid(&platform).then_some(AssetKind::Platform(platform))
}

/// Builds the catalog; `generated` is the date written into it.
pub fn build(source: &Source, github: &GitHub, generated: &str) -> Result<Catalog, String> {
    let mut programs = Vec::new();
    for p in &source.programs {
        let mut found: Vec<(Version, GhRelease)> = github
            .releases(&p.repository)?
            .into_iter()
            .filter(|r| !r.draft && !r.prerelease)
            .filter_map(|r| {
                let version = r.tag_name.strip_prefix(&p.tag_prefix)?.parse().ok()?;
                Some((version, r))
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0));
        let mut releases = Vec::new();
        for (version, release) in found {
            if releases.len() == p.releases {
                break;
            }
            let mut platforms = BTreeMap::new();
            let mut skill_pack = None;
            for asset in &release.assets {
                let kind = classify(&asset.name, &p.id, &version);
                let is_mac =
                    matches!(&kind, Some(AssetKind::Platform(pl)) if pl.starts_with("macos-"));
                if kind.is_none() || (is_mac && p.formula.is_some()) {
                    continue;
                }
                let download = Download {
                    url: asset.browser_download_url.clone(),
                    sha256: github.sha256(asset)?,
                    size: asset.size,
                };
                match kind {
                    Some(AssetKind::Platform(pl)) => {
                        platforms.insert(pl, Artifact::Archive(download));
                    }
                    Some(AssetKind::Skills) => skill_pack = Some(download),
                    None => {}
                }
            }
            if let Some(formula) = &p.formula {
                for mac in ["macos-aarch64", "macos-x86_64"] {
                    platforms.insert(
                        mac.into(),
                        Artifact::Brew {
                            formula: formula.clone(),
                        },
                    );
                }
            }
            if platforms.is_empty() {
                continue;
            }
            releases.push(Release {
                version,
                platforms,
                skill_pack,
            });
        }
        programs.push(Program {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            homepage: p.homepage.clone(),
            license: p.license.clone(),
            releases,
        });
    }
    let catalog = Catalog {
        format: FORMAT,
        generated: generated.to_owned(),
        programs,
    };
    catalog.validate().map_err(|e| e.to_string())?;
    Ok(catalog)
}

/// Today as `YYYY-MM-DD` (UTC).
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Tage seit 1970 → Kalenderdatum (Howard Hinnant, civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names() {
        let v: Version = "0.4.0".parse().unwrap();
        assert_eq!(
            classify("ct-editor-0.4.0-linux-x86_64.tar.gz", "ct-editor", &v),
            Some(AssetKind::Platform("linux-x86_64".into()))
        );
        assert_eq!(
            classify("ct-editor-0.4.0-macos-arm64.zip", "ct-editor", &v),
            Some(AssetKind::Platform("macos-aarch64".into()))
        );
        assert_eq!(
            classify("freya-tde-0.4.0-skills.tar.gz", "freya-tde", &v),
            Some(AssetKind::Skills)
        );
        assert_eq!(
            classify("ct-editor-0.3.0-linux-x86_64.tar.gz", "ct-editor", &v),
            None
        );
        assert_eq!(
            classify("ct-editor-0.4.0-linux-x86_64.deb", "ct-editor", &v),
            None
        );
        assert_eq!(classify("README.md", "ct-editor", &v), None);
        assert_eq!(today().len(), 10);
    }
}
