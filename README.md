# DS-Catalog

Der signierte Programmkatalog von Dragon Software für den DS-Launcher, veröffentlicht unter
<https://catalog.dragon-software.net/catalog.json> (GitHub Pages), dazu das Signierwerkzeug `ds-sign`.

*The signed program catalog of Dragon Software for the DS-Launcher, published at
<https://catalog.dragon-software.net/catalog.json> (GitHub Pages), plus the signing tool `ds-sign`.*

## Inhalt / Contents

| | |
|---|---|
| `programs.toml` | Katalog-Quelle: Programme und ihre Repositories. / Catalog source: programs and their repositories. |
| `crates/ds-catalog` | Bibliothek: Katalogschema mit Prüfung, Ed25519-Signaturen, Plattformen. Der DS-Launcher nutzt sie. / Library: catalog schema with checks, Ed25519 signatures, platforms; used by the DS-Launcher. |
| `crates/ds-sign` | Werkzeug: Schlüssel, Signaturen, Katalog bauen, Skill-Pakete für Freya TDE. / Tool: keys, signatures, building the catalog, skill packs for Freya TDE. |
| `.github/workflows/catalog.yml` | Baut, signiert und veröffentlicht den Katalog. / Builds, signs and publishes the catalog. |

Releases dieses Repositorys mit dem Tag `ds-launcher-v<Version>` enthalten die Pakete des DS-Launchers.

*Releases of this repository tagged `ds-launcher-v<version>` carry the DS-Launcher packages.*

## Katalog bauen / Building the catalog

```bash
cargo run -p ds-sign -- catalog programs.toml --out catalog.json   # aus den GitHub-Releases
DS_SIGN_KEY=… cargo run -p ds-sign -- sign catalog.json             # catalog.json.sig
```

Release-Dateien heißen `<id>-<version>-<plattform>.tar.gz` bzw. `.zip` (z. B. `ct-editor-0.4.0-linux-x86_64.tar.gz`),
Skill-Pakete `<id>-<version>-skills.tar.gz`. Linux-Archive haben einen obersten Ordner mit `bin/<id>` und `share/…`.
Programme mit Homebrew-Formel bekommen für macOS einen `brew`-Eintrag. Neue Releases melden die Programme per
`repository_dispatch` (Typ `release`).

*Release assets are named `<id>-<version>-<platform>.tar.gz` or `.zip`, skill packs `<id>-<version>-skills.tar.gz`.
Linux archives have one top folder with `bin/<id>` and `share/…`. Programs with a Homebrew formula get a `brew`
entry for macOS. Programs announce new releases via `repository_dispatch` (type `release`).*

## ds-sign installieren / Installing ds-sign

```bash
cargo install --locked --git https://github.com/Dragon-InterActive/DS-Catalog ds-sign
```

## Lizenz / License

MIT OR Apache-2.0, © 2026 Dragon Software. Vollständig mit Claude Code (Claude Opus 5.5) umgesetzt. /
*Built entirely with Claude Code (Claude Opus 5.5).*
