param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

if ([string]::IsNullOrWhiteSpace($Version)) {
    $CargoToml = Get-Content -Path (Join-Path $Root "Cargo.toml") -Raw
    if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    } else {
        $Version = "0.1.3"
    }
}

$Dist = Join-Path $Root "dist"
$Stage = Join-Path $Dist "stage-windows-x86_64"

$GuiBin = Join-Path $Root "target\release\student-journal.exe"
$McpBin = Join-Path $Root "target\release\student-journal-mcp.exe"

if (-not (Test-Path $GuiBin) -or -not (Test-Path $McpBin)) {
    throw "Release binaries missing. Run: cargo build --release --workspace"
}

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
# Clean stale versioned Windows artifacts so SHA256SUMS.txt only covers this build
Get-ChildItem $Dist -File | Where-Object {
    $_.Name -match '^(Oxidium-.*-windows-x86_64.*\.(zip|exe)|oxidium-.*-sbom\.cdx\.json|SHA256SUMS\.txt)$'
} | Remove-Item -Force

if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null

# 1. Stage Portable Payload
Copy-Item $GuiBin $Stage
Copy-Item $McpBin $Stage
Copy-Item (Join-Path $Root "LICENSE.txt") $Stage
Copy-Item (Join-Path $Root "THIRD_PARTY_NOTICES.md") $Stage
Copy-Item (Join-Path $Root "docs\mcp\QUICKSTART.md") (Join-Path $Stage "MCP-QUICKSTART.md")
Copy-Item (Join-Path $Root "CHANGELOG.md") (Join-Path $Stage "RELEASE-NOTES.md")

$PortableZip = Join-Path $Dist "Oxidium-$Version-windows-x86_64-portable.zip"
if (Test-Path $PortableZip) { Remove-Item -Force $PortableZip }
Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $PortableZip -CompressionLevel Optimal
Write-Host "Created $PortableZip"

# 2. Build Native Per-User HKCU Self-Extracting Windows Installer
$SetupExe = Join-Path $Dist "Oxidium-$Version-windows-x86_64-setup.exe"
$env:OXIDIUM_PAYLOAD_ZIP = $PortableZip
$env:OXIDIUM_VERSION = $Version
$InstallerSrc = Join-Path $Root "scripts\windows_installer.rs"
& rustc --edition 2024 -C opt-level=3 -C strip=symbols $InstallerSrc -o $SetupExe
if ($LASTEXITCODE -ne 0) {
    throw "Failed to compile native Windows installer"
}
$SetupPdb = Join-Path $Dist "Oxidium-$Version-windows-x86_64-setup.pdb"
if (Test-Path $SetupPdb) { Remove-Item -Force $SetupPdb }
Write-Host "Created $SetupExe"

# 3. Also compile Inno Setup installer if ISCC is available
$iscc = Get-Command iscc -ErrorAction SilentlyContinue
if ($null -ne $iscc) {
    & $iscc.Source "/DMyAppVersion=$Version" (Join-Path $Root "scripts\oxidium.iss")
    if ($LASTEXITCODE -ne 0) {
        throw "ISCC failed"
    }
}

# 4. Generate CycloneDX 1.5 SBOM
$SbomPath = Join-Path $Dist "oxidium-$Version-sbom.cdx.json"
& python (Join-Path $Root "scripts\generate-sbom.py") $SbomPath
if ($LASTEXITCODE -ne 0) {
    throw "SBOM generation failed"
}

# 5. Generate SHA256SUMS.txt
$SumFile = Join-Path $Dist "SHA256SUMS.txt"
$Artifacts = Get-ChildItem $Dist -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | Sort-Object Name
$Lines = foreach ($f in $Artifacts) {
    $hash = (Get-FileHash -Algorithm SHA256 -Path $f.FullName).Hash.ToLowerInvariant()
    "$hash  $($f.Name)"
}
Set-Content -Path $SumFile -Value $Lines -Encoding UTF8
Write-Host "Created $SumFile"

Remove-Item -Recurse -Force $Stage
Get-ChildItem $Dist -File | Select-Object Name, Length
