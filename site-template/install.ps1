# DS-Launcher installieren (Windows) – Install the DS-Launcher (Windows).
#
#   irm https://catalog.dragon-software.net/install.ps1 | iex
#
# Lädt das neueste Release, prüft die SHA-256-Prüfsumme, entpackt es in einen
# temporären Ordner und lässt den Launcher sich selbst installieren
# (Programme\Dragon Software, Startmenü; Rechteabfrage über UAC). Der Launcher
# prüft danach jeden Katalog mit seiner eingebauten Ed25519-Signatur.
# PowerShell setzt beim Herunterladen keine Download-Markierung: SmartScreen
# fragt deshalb nicht nach.
#
# Downloads the latest release, checks its SHA-256, unpacks it into a temporary
# folder and lets the launcher install itself (Program Files\Dragon Software,
# start menu; UAC asks for rights).

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# DS_LAUNCHER_BASE: andere Download-Adresse (Tests, Spiegel) / other download address.
$base = if ($env:DS_LAUNCHER_BASE) { $env:DS_LAUNCHER_BASE } else {
    'https://github.com/Dragon-InterActive/DS-Catalog/releases/latest/download'
}
$file = 'ds-launcher-windows-x86_64.zip'

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'ds-launcher: nur 64-Bit-Windows / 64-bit Windows only'
}

$work = Join-Path ([IO.Path]::GetTempPath()) ('ds-launcher-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $zip = Join-Path $work $file
    Write-Host "Lade / downloading $file ..."
    Invoke-WebRequest -Uri "$base/$file" -OutFile $zip -UseBasicParsing
    Invoke-WebRequest -Uri "$base/$file.sha256" -OutFile "$zip.sha256" -UseBasicParsing
    $expected = ((Get-Content -Raw "$zip.sha256").Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLower()
    if ($expected -ne $actual) {
        throw "ds-launcher: Prüfsumme stimmt nicht / checksum mismatch ($actual, erwartet / expected $expected)"
    }

    $dir = Join-Path $work 'launcher'
    Expand-Archive -Path $zip -DestinationPath $dir
    $exe = Get-ChildItem -Path $dir -Recurse -Filter 'ds-launcher.exe' | Select-Object -First 1
    if (-not $exe) { throw 'ds-launcher: ds-launcher.exe fehlt im Archiv / missing in the archive' }

    Write-Host 'Installiere / installing ...'
    # Fensterprogramm: ohne -Wait würde PowerShell nicht auf das Ende warten.
    $run = Start-Process -FilePath $exe.FullName -ArgumentList 'install', 'ds-launcher' `
        -NoNewWindow -Wait -PassThru
    if ($run.ExitCode -ne 0) {
        throw "ds-launcher: Installation fehlgeschlagen / failed (Code $($run.ExitCode))"
    }

    $installed = Join-Path $env:ProgramFiles 'Dragon Software\DS-Launcher\bin\ds-launcher.exe'
    if (Test-Path $installed) { Start-Process -FilePath $installed }
    Write-Host 'Fertig / done: der DS-Launcher steht im Startmenü / the DS-Launcher is in the start menu.'
}
finally {
    Remove-Item -Recurse -Force -Path $work -ErrorAction SilentlyContinue
}
