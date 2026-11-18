#!/bin/sh
# DS-Launcher installieren (Linux, macOS) – Install the DS-Launcher (Linux, macOS).
#
#   curl -fsSL https://catalog.dragon-software.net/install.sh | sh
#
# Linux: lädt das neueste Release, prüft die SHA-256-Prüfsumme, entpackt es in
# einen temporären Ordner und lässt den Launcher sich selbst installieren
# (/opt/dragon-software, Startmenü; Rechteabfrage über pkexec). Der Launcher
# prüft danach jeden Katalog mit seiner eingebauten Ed25519-Signatur.
# macOS: über Homebrew (brew install freya-tde/tap/ds-launcher).
#
# Linux: downloads the latest release, checks its SHA-256, unpacks it into a
# temporary folder and lets the launcher install itself (/opt/dragon-software,
# start menu; pkexec asks for rights). macOS: via Homebrew.
set -eu

# DS_LAUNCHER_BASE: andere Download-Adresse (Tests, Spiegel) / other download address.
base="${DS_LAUNCHER_BASE:-https://github.com/Dragon-InterActive/DS-Catalog/releases/latest/download}"

say() { printf '%s\n' "$*"; }
fail() {
    printf 'ds-launcher: %s\n' "$*" >&2
    exit 1
}

case "$(uname -s)" in
Darwin)
    command -v brew >/dev/null 2>&1 ||
        fail "Homebrew fehlt / missing – https://brew.sh, danach erneut / then run again"
    brew install freya-tde/tap/ds-launcher
    say "Fertig / done: ds-launcher starten / start ds-launcher."
    exit 0
    ;;
Linux) ;;
*) fail "nicht unterstützt / not supported: $(uname -s)" ;;
esac

case "$(uname -m)" in
x86_64 | amd64) platform=linux-x86_64 ;;
*) fail "nicht unterstützt / not supported: $(uname -m)" ;;
esac

for tool in curl tar sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool fehlt / missing"
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
file="ds-launcher-$platform.tar.gz"

say "Lade / downloading $file …"
curl -fsSL -o "$work/$file" "$base/$file"
curl -fsSL -o "$work/$file.sha256" "$base/$file.sha256"
expected=$(cut -d' ' -f1 <"$work/$file.sha256")
actual=$(sha256sum "$work/$file" | cut -d' ' -f1)
[ "$expected" = "$actual" ] ||
    fail "Prüfsumme stimmt nicht / checksum mismatch ($actual, erwartet / expected $expected)"

mkdir "$work/launcher"
tar xzf "$work/$file" -C "$work/launcher" --strip-components=1

say "Installiere / installing …"
"$work/launcher/bin/ds-launcher" install ds-launcher

installed=/opt/dragon-software/ds-launcher/bin/ds-launcher
if [ -x "$installed" ] && { [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; }; then
    # Im Hintergrund starten, losgelöst vom Terminal / start detached.
    (setsid "$installed" >/dev/null 2>&1 &) 2>/dev/null || ("$installed" >/dev/null 2>&1 &)
fi
say "Fertig / done: der DS-Launcher steht im Startmenü / the DS-Launcher is in the start menu."
