//! `ds-sign` end to end: keys, signatures, skill packs.

use std::path::Path;
use std::process::{Command, Output};

fn ds_sign(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ds-sign"))
        .args(args)
        .current_dir(dir)
        .env_remove("DS_SIGN_KEY")
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn keys_signatures_and_skill_packs() {
    let dir = std::env::temp_dir().join(format!("ds-sign-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Schlüsselpaar; ein zweites keygen überschreibt nichts.
    let public = ok(&ds_sign(&dir, &["keygen", "test"])).trim().to_owned();
    assert_eq!(public.len(), 64);
    assert_eq!(
        std::fs::read_to_string(dir.join("test.pub"))
            .unwrap()
            .trim(),
        public
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("test.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(!ds_sign(&dir, &["keygen", "test"]).status.success());
    assert_eq!(
        ok(&ds_sign(&dir, &["public", "--key", "test.key"])).trim(),
        public
    );

    // Signieren und prüfen; eine geänderte Datei fällt durch.
    std::fs::write(dir.join("catalog.json"), "{}").unwrap();
    ok(&ds_sign(
        &dir,
        &["sign", "--key", "test.key", "catalog.json"],
    ));
    ok(&ds_sign(
        &dir,
        &["verify", "--public", &public, "catalog.json"],
    ));
    ok(&ds_sign(
        &dir,
        &["verify", "--public", "test.pub", "catalog.json"],
    ));
    std::fs::write(dir.join("catalog.json"), "{ }").unwrap();
    assert!(
        !ds_sign(&dir, &["verify", "--public", &public, "catalog.json"])
            .status
            .success()
    );

    // Schlüssel aus DS_SIGN_KEY (CI).
    let secret = std::fs::read_to_string(dir.join("test.key")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ds-sign"))
        .args(["sign", "catalog.json"])
        .current_dir(&dir)
        .env("DS_SIGN_KEY", secret.trim())
        .output()
        .unwrap();
    ok(&out);
    ok(&ds_sign(
        &dir,
        &["verify", "--public", &public, "catalog.json"],
    ));
    assert!(
        !ds_sign(&dir, &["sign", "catalog.json"]).status.success(),
        "ohne Schlüssel"
    );

    // Skill-Paket im Format, das Freya TDE prüft.
    let pack = dir.join("skills");
    let skill = "---\nname: demo\ndescription: Demo\n---\n\n# Demo\n";
    std::fs::create_dir_all(pack.join("demo")).unwrap();
    std::fs::write(pack.join("demo/SKILL.md"), skill).unwrap();
    ok(&ds_sign(
        &dir,
        &[
            "skill-pack",
            "skills",
            "--version",
            "2026.11.20",
            "--min-freya",
            "0.1",
            "--key",
            "test.key",
        ],
    ));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(pack.join("pack.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], 1);
    assert_eq!(manifest["version"], "2026.11.20");
    assert_eq!(manifest["min_freya"], "0.1");
    assert_eq!(
        manifest["skills"]["demo/SKILL.md"],
        ds_catalog::sign::sha256(skill.as_bytes())
    );
    ok(&ds_sign(
        &pack,
        &["verify", "--public", &public, "pack.json"],
    ));
    // Noch einmal über einem fertigen Paket: pack.json wird ersetzt.
    ok(&ds_sign(
        &dir,
        &[
            "skill-pack",
            "skills",
            "--version",
            "2026.11.21",
            "--min-freya",
            "0.1",
            "--key",
            "test.key",
        ],
    ));

    // Fremde Dateien im Paket verweigert.
    std::fs::write(pack.join("demo/run.sh"), "echo").unwrap();
    assert!(
        !ds_sign(
            &dir,
            &[
                "skill-pack",
                "skills",
                "--version",
                "1",
                "--min-freya",
                "0.1",
                "--key",
                "test.key"
            ]
        )
        .status
        .success()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Serves `files` (path → body) for the rest of the test.
fn serve(files: Vec<(String, Vec<u8>)>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap() == 0 || h.trim().is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let mut out = stream;
            match files.iter().find(|(p, _)| path.starts_with(p.as_str())) {
                Some((_, body)) => {
                    let _ = write!(
                        out,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = out.write_all(body);
                }
                None => {
                    let _ = write!(
                        out,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                }
            }
        }
    });
    addr
}

#[test]
fn catalog_from_github_releases() {
    let dir = std::env::temp_dir().join(format!("ds-sign-catalog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let windows = b"windows-zip".to_vec();
    // Platzhalter für die eigene Adresse; der Server kennt sie erst nach dem Start.
    let assets = |addr: &str| {
        serde_json::to_vec(&serde_json::json!([
            { "tag_name": "v0.5.0", "draft": false, "prerelease": true, "assets": [] },
            { "tag_name": "v0.4.0", "draft": false, "prerelease": false, "assets": [
                { "name": "ct-editor-0.4.0-linux-x86_64.tar.gz", "size": 3, "browser_download_url": "https://x/l.tar.gz", "digest": "sha256:ABCDEF0000000000000000000000000000000000000000000000000000000000" },
                { "name": "ct-editor-0.4.0-windows-x86_64.zip", "size": windows.len(), "browser_download_url": format!("{addr}/dl/w.zip") },
                { "name": "ct-editor-0.4.0-macos-arm64.zip", "size": 1, "browser_download_url": "https://x/m.zip", "digest": "sha256:00" },
                { "name": "ct-editor-0.4.0-skills.tar.gz", "size": 2, "browser_download_url": "https://x/s.tar.gz", "digest": format!("sha256:{}", "1".repeat(64)) },
                { "name": "README.md", "size": 1, "browser_download_url": "https://x/r" }
            ]},
            { "tag_name": "v0.3.0", "draft": false, "prerelease": false, "assets": [
                { "name": "ct-editor-0.3.0-linux-x86_64.tar.gz", "size": 1, "browser_download_url": "https://x/old", "digest": format!("sha256:{}", "2".repeat(64)) }
            ]},
            { "tag_name": "v9.9.9", "draft": true, "prerelease": false, "assets": [] }
        ]))
        .unwrap()
    };
    // Erst starten, dann die Antworten mit der Adresse füllen: zwei Server.
    let downloads = serve(vec![("/dl/w.zip".into(), windows.clone())]);
    let api = serve(vec![
        ("/repos/Freya-TDE/CT-E/releases".into(), assets(&downloads)),
        ("/repos/Freya-TDE/Leer/releases".into(), b"[]".to_vec()),
    ]);
    std::fs::write(
        dir.join("programs.toml"),
        r#"
[[program]]
id = "ct-editor"
name = "CT-Editor"
license = "MIT OR Apache-2.0"
repository = "Freya-TDE/CT-E"
formula = "freya-tde/tap/ct-editor"
releases = 2

[[program]]
id = "noch-nicht"
name = "Ohne Release"
license = "MIT"
repository = "Freya-TDE/Leer"
"#,
    )
    .unwrap();
    ok(&ds_sign(
        &dir,
        &[
            "catalog",
            "programs.toml",
            "--out",
            "catalog.json",
            "--api",
            &api,
            "--generated",
            "2026-11-17",
        ],
    ));
    let c: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("catalog.json")).unwrap()).unwrap();
    assert_eq!(c["generated"], "2026-11-17");
    assert_eq!(
        c["programs"].as_array().unwrap().len(),
        1,
        "Programm ohne Release fehlt"
    );
    let releases = c["programs"][0]["releases"].as_array().unwrap();
    assert_eq!(releases.len(), 2, "Vorabversion und Entwurf übersprungen");
    let r = &releases[0];
    assert_eq!(r["version"], "0.4.0");
    assert_eq!(
        r["platforms"]["linux-x86_64"]["sha256"],
        format!("abcdef{}", "0".repeat(58))
    );
    assert_eq!(
        r["platforms"]["windows-x86_64"]["sha256"],
        ds_catalog::sign::sha256(&windows),
        "ohne Digest heruntergeladen"
    );
    assert_eq!(r["platforms"]["macos-aarch64"]["kind"], "brew");
    assert_eq!(
        r["platforms"]["macos-x86_64"]["formula"],
        "freya-tde/tap/ct-editor"
    );
    assert_eq!(r["skill_pack"]["sha256"], "1".repeat(64));
    assert_eq!(releases[1]["version"], "0.3.0");
    let _ = std::fs::remove_dir_all(&dir);
}
