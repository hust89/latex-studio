# Build latex-ffi and stage it plus tectonic.exe inside the Flutter Windows bundle.
$ErrorActionPreference = "Stop"

$Root = Resolve-Path "$PSScriptRoot\.."
$Tectonic = $env:TECTONIC_BIN
if (-not $Tectonic) { $Tectonic = (Get-Command tectonic -ErrorAction SilentlyContinue).Source }
if (-not $Tectonic) { throw "tectonic not found; set TECTONIC_BIN" }

Push-Location $Root
cargo build --release -p latex-ffi

Push-Location flutter
flutter build windows --release
$Bundle = Join-Path $Root "flutter\build\windows\x64\runner\Release"

Copy-Item "$Root\target\release\latex_ffi.dll" $Bundle
Copy-Item $Tectonic (Join-Path $Bundle "tectonic.exe")

Pop-Location
Pop-Location
Write-Host "Staged: $Bundle"
