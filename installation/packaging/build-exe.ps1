# Construit l'exécutable Windows avec PyInstaller (sur un PC Windows, Python 3.12).
# Usage : powershell -File installation\packaging\build-exe.ps1 [-Version v0.2.13]
param([string]$Version = "")
$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
if (-not $Version) { $Version = (Get-Content "$root\VERSION" -Raw).Trim() }
$work = "$root\build\installation"
New-Item -ItemType Directory -Force $work | Out-Null
"VERSION = `"$Version`"" | Set-Content -Encoding utf8 "$root\installation\winlux_usb\_version.py"
python -m pip install --disable-pip-version-check -q pyinstaller pillow
python -c "from PIL import Image; Image.open(r'$root\assets\brand\winlux-logo-256.png').save(r'$work\winlux.ico', sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])"
$name = "winlux-usb-creator-$Version"
python -m PyInstaller --noconfirm --clean --onefile --windowed --uac-admin `
    --name $name --icon "$work\winlux.ico" `
    --paths "$root\installation" `
    --hidden-import winlux_usb.gui --hidden-import winlux_usb.disks_windows `
    --add-data "$root\assets\brand\winlux-logo-64.png;winlux_usb" `
    --distpath "$work\dist" --workpath "$work\pyi" --specpath "$work" `
    "$root\installation\winlux-usb-creator.py"
if ($LASTEXITCODE -ne 0) { throw "PyInstaller a échoué" }
Get-Item "$work\dist\$name.exe"
