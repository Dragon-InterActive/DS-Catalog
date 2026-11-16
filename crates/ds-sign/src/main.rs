//! `ds-sign`: Ed25519 keys and signatures for the DS-Launcher catalog and
//! Freya TDE's skill packs.
//!
//! - `ds-sign keygen <name>` writes `<name>.key` (secret, hex, mode 0600)
//!   and `<name>.pub` (public, hex) and prints the public key.
//! - `ds-sign sign [--key <file>] <file>…` writes `<file>.sig` next to each
//!   file. Without `--key` the secret key comes from `DS_SIGN_KEY` (hex,
//!   for CI secrets).
//! - `ds-sign verify --public <hex|file> <file>…` checks `<file>.sig`.
//! - `ds-sign skill-pack <dir> --version <v> --min-freya <v> [--key <file>]`
//!   writes and signs `pack.json` for the skill folders in `<dir>`.
//! - `ds-sign catalog <programs.toml> --out <catalog.json>` builds the
//!   catalog from the programs' GitHub releases (token from `GITHUB_TOKEN`);
//!   sign it afterwards with `ds-sign sign`.
//!
//! *Signierwerkzeug: Schlüssel erzeugen, Dateien signieren und prüfen,
//! Skill-Pakete für Freya TDE erstellen.*

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod catalog_build;

use clap::{Parser, Subcommand};
use ds_catalog::sign;

#[derive(Parser)]
#[command(
    name = "ds-sign",
    version,
    about = "Ed25519-Signaturen für DS-Launcher und Freya TDE"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Neues Schlüsselpaar: <name>.key (geheim) und <name>.pub.
    Keygen { name: PathBuf },
    /// Öffentlichen Schlüssel zu einem geheimen ausgeben.
    Public {
        #[arg(long)]
        key: Option<PathBuf>,
    },
    /// Dateien signieren (<datei>.sig).
    Sign {
        #[arg(long)]
        key: Option<PathBuf>,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Signaturen prüfen.
    Verify {
        /// Öffentlicher Schlüssel als Hex oder Datei (.pub).
        #[arg(long)]
        public: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// catalog.json aus programs.toml und den GitHub-Releases bauen.
    Catalog {
        source: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Adresse der GitHub-API.
        #[arg(long, default_value = "https://api.github.com")]
        api: String,
        /// Datum im Katalog statt heute (JJJJ-MM-TT).
        #[arg(long)]
        generated: Option<String>,
    },
    /// pack.json für ein Skill-Paket schreiben und signieren.
    SkillPack {
        dir: PathBuf,
        #[arg(long)]
        version: String,
        #[arg(long)]
        min_freya: String,
        #[arg(long)]
        key: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ds-sign: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Keygen { name } => keygen(&name),
        Command::Catalog {
            source,
            out,
            api,
            generated,
        } => {
            let text = std::fs::read_to_string(&source)
                .map_err(|e| format!("{}: {e}", source.display()))?;
            let source_data: catalog_build::Source =
                toml::from_str(&text).map_err(|e| format!("{}: {e}", source.display()))?;
            let github = catalog_build::GitHub {
                api,
                token: std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()),
            };
            let generated = generated.unwrap_or_else(catalog_build::today);
            let catalog = catalog_build::build(&source_data, &github, &generated)?;
            let json = serde_json::to_string_pretty(&catalog).expect("JSON") + "\n";
            write(&out, &json)?;
            println!("{}", out.display());
            Ok(())
        }
        Command::Public { key } => {
            println!(
                "{}",
                sign::to_hex(&sign::public_key(&secret_key(key.as_deref())?))
            );
            Ok(())
        }
        Command::Sign { key, files } => {
            let secret = secret_key(key.as_deref())?;
            for file in files {
                let data = read(&file)?;
                let sig = with_ext(&file, "sig");
                write(&sig, &(sign::sign(&data, &secret) + "\n"))?;
                println!("{}", sig.display());
            }
            Ok(())
        }
        Command::Verify { public, files } => {
            let key = public_key(&public)?;
            for file in files {
                let sig = with_ext(&file, "sig");
                let signature =
                    std::fs::read_to_string(&sig).map_err(|e| format!("{}: {e}", sig.display()))?;
                sign::verify(&read(&file)?, &signature, &key)
                    .map_err(|e| format!("{}: {e}", file.display()))?;
                println!("{}: ok", file.display());
            }
            Ok(())
        }
        Command::SkillPack {
            dir,
            version,
            min_freya,
            key,
        } => {
            let secret = secret_key(key.as_deref())?;
            let manifest = skill_pack_manifest(&dir, &version, &min_freya)?;
            let path = dir.join("pack.json");
            write(&path, &manifest)?;
            write(
                &dir.join("pack.json.sig"),
                &(sign::sign(manifest.as_bytes(), &secret) + "\n"),
            )?;
            println!("{}", path.display());
            Ok(())
        }
    }
}

fn keygen(name: &Path) -> Result<(), String> {
    let secret_path = with_ext(name, "key");
    let public_path = with_ext(name, "pub");
    for p in [&secret_path, &public_path] {
        if p.exists() {
            return Err(format!(
                "{} gibt es schon; nichts überschrieben",
                p.display()
            ));
        }
    }
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| format!("Zufallszahlen: {e}"))?;
    let public = sign::to_hex(&sign::public_key(&secret));
    write_secret(&secret_path, &(sign::to_hex(&secret) + "\n"))?;
    write(&public_path, &(public.clone() + "\n"))?;
    println!("{public}");
    eprintln!(
        "geheim: {} (sicher aufbewahren, nie einchecken), öffentlich: {}",
        secret_path.display(),
        public_path.display()
    );
    Ok(())
}

/// The secret key from `--key <file>` or `DS_SIGN_KEY`.
fn secret_key(file: Option<&Path>) -> Result<[u8; 32], String> {
    let (text, source) = match file {
        Some(f) => (
            std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?,
            f.display().to_string(),
        ),
        None => (
            std::env::var("DS_SIGN_KEY")
                .map_err(|_| "kein Schlüssel: --key <datei> oder DS_SIGN_KEY".to_owned())?,
            "DS_SIGN_KEY".to_owned(),
        ),
    };
    sign::key_from_hex(&text)
        .ok_or_else(|| format!("{source}: kein geheimer Schlüssel (64 Hex-Zeichen)"))
}

/// A public key as hex or from a file.
fn public_key(text: &str) -> Result<[u8; 32], String> {
    sign::key_from_hex(text)
        .or_else(|| {
            std::fs::read_to_string(text)
                .ok()
                .and_then(|t| sign::key_from_hex(&t))
        })
        .ok_or_else(|| {
            format!("{text}: kein öffentlicher Schlüssel (64 Hex-Zeichen oder .pub-Datei)")
        })
}

/// `pack.json` for the skill folders in `dir`: each subfolder holds exactly
/// one `SKILL.md`; other files are refused (Freya TDE would refuse the pack).
fn skill_pack_manifest(dir: &Path, version: &str, min_freya: &str) -> Result<String, String> {
    let mut skills = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() {
            if matches!(name.as_str(), "pack.json" | "pack.json.sig") {
                continue;
            }
            return Err(format!("{}: nur Skill-Ordner erlaubt", path.display()));
        }
        let files: Vec<_> = std::fs::read_dir(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        if files.len() != 1 || files[0] != "SKILL.md" {
            return Err(format!(
                "{}: genau eine Datei SKILL.md erwartet",
                path.display()
            ));
        }
        let text =
            std::fs::read(path.join("SKILL.md")).map_err(|e| format!("{name}/SKILL.md: {e}"))?;
        skills.insert(format!("{name}/SKILL.md"), sign::sha256(&text));
    }
    if skills.is_empty() {
        return Err(format!("{}: keine Skill-Ordner", dir.display()));
    }
    let manifest = serde_json::json!({
        "format": 1,
        "version": version,
        "min_freya": min_freya,
        "skills": skills,
    });
    Ok(serde_json::to_string_pretty(&manifest).expect("JSON") + "\n")
}

fn with_ext(path: &Path, ext: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes a secret file readable only by the owner (Unix 0600).
fn write_secret(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}
