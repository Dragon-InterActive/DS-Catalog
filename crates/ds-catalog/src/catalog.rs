//! The program catalog: `catalog.json`, signed with Ed25519 in
//! `catalog.json.sig` (hex, over the exact bytes of the JSON).
//!
//! The catalog is a static file, so any web server can host it (GitHub
//! Pages first, an own server later). Nothing in it is used before the
//! signature is checked.
//!
//! ```json
//! {
//!   "format": 1,
//!   "generated": "2026-11-16",
//!   "programs": [{
//!     "id": "freya-tde", "name": "Freya TDE",
//!     "description": "…", "homepage": "https://…", "license": "MIT OR Apache-2.0",
//!     "requires": ["ct-editor"],
//!     "releases": [{
//!       "version": "0.1.0",
//!       "platforms": {
//!         "linux-x86_64": { "kind": "archive", "url": "https://…", "sha256": "…", "size": 123 },
//!         "macos-aarch64": { "kind": "brew", "formula": "freya-tde/tap/freya-tde" }
//!       },
//!       "skill_pack": { "url": "https://…", "sha256": "…", "size": 456 }
//!     }]
//!   }]
//! }
//! ```
//!
//! Readers skip fields they don't know (no `deny_unknown_fields`): a newer
//! catalog with additional fields still loads in an older launcher, which
//! must keep reading it to update itself.
//!
//! *Programmkatalog: statische, signierte JSON-Datei; unbekannte Felder werden
//! übersprungen, damit ältere Launcher neuere Kataloge lesen.*

use std::collections::{BTreeMap, BTreeSet};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{Error, codes, sign};

/// The catalog format this launcher understands.
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub format: u32,
    /// When the catalog was made (ISO date, informative).
    pub generated: String,
    pub programs: Vec<Program>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    /// Stable id (`a-z`, `0-9`, `-`), e.g. `freya-tde`.
    pub id: String,
    pub name: String,
    /// German description.
    pub description: String,
    /// English description (optional; readers fall back to `description`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description_en: String,
    pub homepage: String,
    /// SPDX expression of the program's license.
    pub license: String,
    /// Programs installed together with this one if they are missing (not
    /// the other way round), e.g. Freya TDE brings the CT-Editor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    pub releases: Vec<Release>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: Version,
    /// Artifact per platform key (`<os>-<arch>`, see [`crate::platform`]).
    pub platforms: BTreeMap<String, Artifact>,
    /// Signed skill pack for Freya TDE (only Freya TDE has one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_pack: Option<Download>,
}

/// How a program gets onto a platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Artifact {
    /// An archive (`.tar.gz`, `.zip`) the launcher downloads and unpacks.
    Archive(Download),
    /// A Homebrew formula; the launcher calls `brew` and touches nothing else.
    Brew { formula: String },
}

/// A file to download, with its checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    pub url: String,
    /// SHA-256, lower-case hex.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
}

impl Download {
    /// Checks downloaded bytes against size and checksum.
    pub fn check(&self, bytes: &[u8]) -> Result<(), Error> {
        let hash = sign::sha256(bytes);
        if bytes.len() as u64 != self.size || hash != self.sha256 {
            return Err(Error::new(
                codes::DOWNLOAD_HASH,
                format!(
                    "{}: {} Bytes mit SHA-256 {hash}, erwartet {} Bytes mit {}",
                    self.url,
                    bytes.len(),
                    self.size,
                    self.sha256
                ),
            ));
        }
        Ok(())
    }
}

impl Catalog {
    /// Checks the signature with the public `key`, then reads and validates
    /// the catalog. Nothing is parsed before the signature is checked.
    pub fn load_with_key(json: &[u8], signature: &str, key: &[u8; 32]) -> Result<Self, Error> {
        sign::verify(json, signature, key)?;
        let catalog: Self = serde_json::from_slice(json)
            .map_err(|e| Error::new(codes::CATALOG_INVALID, format!("catalog.json: {e}")))?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Checks the rules the schema cannot express.
    pub fn validate(&self) -> Result<(), Error> {
        let bad = |detail: String| Err(Error::new(codes::CATALOG_INVALID, detail));
        if self.format != FORMAT {
            return bad(format!(
                "Katalogformat {} wird nicht unterstützt (dieser Launcher: {FORMAT})",
                self.format
            ));
        }
        let mut ids = BTreeSet::new();
        for p in &self.programs {
            let valid_id = !p.id.is_empty()
                && p.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !valid_id {
                return bad(format!("ungültige Programm-Kennung „{}“", p.id));
            }
            if !ids.insert(&p.id) {
                return bad(format!("Programm „{}“ steht doppelt im Katalog", p.id));
            }
            let mut versions = BTreeSet::new();
            for r in &p.releases {
                if !versions.insert(&r.version) {
                    return bad(format!("{} {} steht doppelt im Katalog", p.id, r.version));
                }
                let downloads = r
                    .platforms
                    .iter()
                    .map(|(platform, artifact)| (platform.as_str(), artifact))
                    .collect::<Vec<_>>();
                for (platform, artifact) in downloads {
                    if !crate::platform::is_valid(platform) {
                        return bad(format!(
                            "{} {}: ungültige Plattform „{platform}“",
                            p.id, r.version
                        ));
                    }
                    match artifact {
                        Artifact::Archive(d) => check_download(d, &p.id, &r.version)?,
                        Artifact::Brew { formula } if formula.trim().is_empty() => {
                            return bad(format!("{} {}: Formel fehlt", p.id, r.version));
                        }
                        Artifact::Brew { .. } => {}
                    }
                }
                if let Some(d) = &r.skill_pack {
                    check_download(d, &p.id, &r.version)?;
                }
            }
        }
        for p in &self.programs {
            for r in &p.requires {
                if r == &p.id {
                    return bad(format!("„{}“ verlangt sich selbst", p.id));
                }
                if !ids.contains(r) {
                    return bad(format!(
                        "„{}“ verlangt „{r}“, das nicht im Katalog steht",
                        p.id
                    ));
                }
            }
        }
        for p in &self.programs {
            self.requirements(&p.id)?;
        }
        Ok(())
    }

    /// The programs `id` requires, directly or through others, in the order
    /// they have to be installed (a requirement before what needs it).
    pub fn requirements(&self, id: &str) -> Result<Vec<&Program>, Error> {
        fn visit<'a>(
            catalog: &'a Catalog,
            id: &str,
            path: &mut Vec<String>,
            out: &mut Vec<&'a Program>,
        ) -> Result<(), Error> {
            let Some(program) = catalog.program(id) else {
                return Ok(());
            };
            if path.iter().any(|p| p == id) {
                path.push(id.to_owned());
                return Err(Error::new(
                    codes::CATALOG_INVALID,
                    format!("Programme verlangen sich im Kreis: {}", path.join(" → ")),
                ));
            }
            path.push(id.to_owned());
            for r in &program.requires {
                visit(catalog, r, path, out)?;
                if let Some(req) = catalog.program(r)
                    && !out.iter().any(|o| o.id == req.id)
                {
                    out.push(req);
                }
            }
            path.pop();
            Ok(())
        }
        let mut out = Vec::new();
        visit(self, id, &mut Vec::new(), &mut out)?;
        Ok(out)
    }

    pub fn program(&self, id: &str) -> Option<&Program> {
        self.programs.iter().find(|p| p.id == id)
    }
}

impl Program {
    /// The newest release with an artifact for `platform`.
    pub fn latest(&self, platform: &str) -> Option<(&Release, &Artifact)> {
        self.releases
            .iter()
            .filter_map(|r| r.platforms.get(platform).map(|a| (r, a)))
            .max_by(|a, b| a.0.version.cmp(&b.0.version))
    }
}

/// `https://`, or `http://` only to this computer (tests, local mirrors).
fn check_download(d: &Download, id: &str, version: &Version) -> Result<(), Error> {
    let local = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|p| d.url.starts_with(p));
    if !d.url.starts_with("https://") && !local {
        return Err(Error::new(
            codes::CATALOG_INVALID,
            format!("{id} {version}: Download nicht über https: {}", d.url),
        ));
    }
    let hex = d.sha256.len() == 64
        && d.sha256
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !hex {
        return Err(Error::new(
            codes::CATALOG_INVALID,
            format!("{id} {version}: sha256 ist kein SHA-256 in Kleinbuchstaben-Hex"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: [u8; 32] = [9; 32];

    fn catalog_json() -> serde_json::Value {
        let sha = sign::sha256(b"paket");
        serde_json::json!({
            "format": 1,
            "generated": "2026-11-16",
            "programs": [{
                "id": "freya-tde", "name": "Freya TDE", "description": "RAD für Rust",
                "homepage": "https://freya-tde.dev", "license": "MIT OR Apache-2.0",
                "releases": [
                    { "version": "0.1.0", "platforms": {
                        "linux-x86_64": { "kind": "archive", "url": "https://example.org/a.tar.gz", "sha256": sha, "size": 5 }
                    }},
                    { "version": "0.2.0", "platforms": {
                        "linux-x86_64": { "kind": "archive", "url": "https://example.org/b.tar.gz", "sha256": sha, "size": 5 },
                        "macos-aarch64": { "kind": "brew", "formula": "freya-tde/tap/freya-tde" }
                    }, "skill_pack": { "url": "https://example.org/skills.tar.gz", "sha256": sha, "size": 5 }},
                    { "version": "0.10.0", "platforms": {
                        "macos-aarch64": { "kind": "brew", "formula": "freya-tde/tap/freya-tde" }
                    }}
                ]
            }]
        })
    }

    fn signed(value: &serde_json::Value) -> (Vec<u8>, String) {
        let json = serde_json::to_vec_pretty(value).unwrap();
        let sig = sign::sign(&json, &SECRET);
        (json, sig)
    }

    #[test]
    fn loads_signed_catalog() {
        let (json, sig) = signed(&catalog_json());
        let c = Catalog::load_with_key(&json, &sig, &sign::public_key(&SECRET)).unwrap();
        let freya = c.program("freya-tde").unwrap();
        // Semver, nicht Text: 0.10.0 > 0.2.0.
        let (r, a) = freya.latest("macos-aarch64").unwrap();
        assert_eq!(r.version.to_string(), "0.10.0");
        assert!(matches!(a, Artifact::Brew { .. }));
        let (r, a) = freya.latest("linux-x86_64").unwrap();
        assert_eq!(r.version.to_string(), "0.2.0");
        let Artifact::Archive(d) = a else { panic!() };
        assert!(d.check(b"paket").is_ok());
        assert_eq!(d.check(b"paket!").unwrap_err().code, codes::DOWNLOAD_HASH);
        assert!(freya.latest("windows-x86_64").is_none());
    }

    #[test]
    fn refuses_bad_catalogs() {
        let key = sign::public_key(&SECRET);
        let code = |v: &serde_json::Value| {
            let (json, sig) = signed(v);
            Catalog::load_with_key(&json, &sig, &key).unwrap_err().code
        };
        // Fremder Schlüssel und veränderte Daten.
        let (json, sig) = signed(&catalog_json());
        assert_eq!(
            Catalog::load_with_key(&json, &sig, &sign::public_key(&[1; 32]))
                .unwrap_err()
                .code,
            codes::SIGNATURE
        );
        let mut changed = json.clone();
        changed.push(b' ');
        assert_eq!(
            Catalog::load_with_key(&changed, &sig, &key)
                .unwrap_err()
                .code,
            codes::SIGNATURE
        );

        let edit = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v = catalog_json();
            f(&mut v);
            code(&v)
        };
        assert_eq!(edit(&|v| v["format"] = 2.into()), codes::CATALOG_INVALID);
        assert_eq!(
            edit(&|v| v["programs"][0]["id"] = "Freya".into()),
            codes::CATALOG_INVALID
        );
        assert_eq!(
            edit(&|v| {
                let p = v["programs"][0].clone();
                v["programs"].as_array_mut().unwrap().push(p);
            }),
            codes::CATALOG_INVALID
        );
        assert_eq!(
            edit(
                &|v| v["programs"][0]["releases"][0]["platforms"]["linux-x86_64"]["url"] =
                    "http://example.org/a".into()
            ),
            codes::CATALOG_INVALID
        );
        assert_eq!(
            edit(
                &|v| v["programs"][0]["releases"][0]["platforms"]["linux-x86_64"]["sha256"] =
                    "ABC".into()
            ),
            codes::CATALOG_INVALID
        );
        assert_eq!(
            edit(&|v| v["programs"][0]["releases"][1]["version"] = "0.1.0".into()),
            codes::CATALOG_INVALID
        );
        assert_eq!(
            edit(&|v| v["programs"][0]["releases"][0]["platforms"]["Linux"] =
                v["programs"][0]["releases"][0]["platforms"]["linux-x86_64"].clone()),
            codes::CATALOG_INVALID
        );
    }

    /// Ein älterer Launcher muss neuere Kataloge lesen (sonst aktualisiert er
    /// sich nicht mehr): unbekannte Felder werden übersprungen.
    #[test]
    fn skips_unknown_fields() {
        let mut v = catalog_json();
        v["extra"] = 1.into();
        v["programs"][0]["future"] = "x".into();
        v["programs"][0]["releases"][0]["notes"] = "y".into();
        v["programs"][0]["releases"][0]["platforms"]["linux-x86_64"]["mirror"] = "z".into();
        let (json, sig) = signed(&v);
        assert!(Catalog::load_with_key(&json, &sig, &sign::public_key(&SECRET)).is_ok());
    }

    fn with_programs(programs: &[(&str, &[&str])]) -> serde_json::Value {
        let mut v = catalog_json();
        let template = v["programs"][0].clone();
        v["programs"] = programs
            .iter()
            .map(|(id, requires)| {
                let mut p = template.clone();
                p["id"] = (*id).into();
                p["requires"] = requires
                    .iter()
                    .map(|r| serde_json::Value::from(*r))
                    .collect();
                p
            })
            .collect();
        v
    }

    #[test]
    fn requirements_in_install_order() {
        let key = sign::public_key(&SECRET);
        let load = |v: &serde_json::Value| {
            let (json, sig) = signed(v);
            Catalog::load_with_key(&json, &sig, &key)
        };
        let c = load(&with_programs(&[
            ("freya-tde", &["ct-editor", "helper"]),
            ("ct-editor", &["helper"]),
            ("helper", &[]),
        ]))
        .unwrap();
        let ids = |id| -> Vec<String> {
            c.requirements(id)
                .unwrap()
                .iter()
                .map(|p| p.id.clone())
                .collect()
        };
        assert_eq!(ids("freya-tde"), ["helper", "ct-editor"]);
        assert_eq!(ids("ct-editor"), ["helper"]);
        assert!(ids("helper").is_empty());
        // Ohne Angabe: nichts verlangt, und das Feld wird nicht geschrieben.
        assert!(c.program("helper").unwrap().requires.is_empty());
        let json = serde_json::to_string(c.program("helper").unwrap()).unwrap();
        assert!(!json.contains("requires"));

        for bad in [
            with_programs(&[("freya-tde", &["fehlt"])]),
            with_programs(&[("freya-tde", &["freya-tde"])]),
            with_programs(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]),
        ] {
            assert_eq!(load(&bad).unwrap_err().code, codes::CATALOG_INVALID);
        }
    }
}
