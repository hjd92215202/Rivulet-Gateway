[CmdletBinding()]
param(
    [string]$Target = "",
    [ValidateSet("auto", "dir", "zip", "tar.gz", "rpm")]
    [string]$Format = "auto",
    [ValidateSet("release", "debug")]
    [string]$Profile = "release",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:RepoRoot = Split-Path -Parent $PSScriptRoot
$script:WorkspaceManifest = Join-Path $script:RepoRoot "Cargo.toml"

function New-WindowsLayout {
    param(
        [string]$PackageRoot,
        [string]$BinaryPath,
        [string]$ArtifactBinaryName
    )

    New-Item -ItemType Directory -Force $PackageRoot | Out-Null
    New-Item -ItemType Directory -Force (Join-Path $PackageRoot "config") | Out-Null
    Copy-Item $BinaryPath (Join-Path $PackageRoot $ArtifactBinaryName)
    Copy-Item (Join-Path $script:RepoRoot "packaging\examples\gateway.toml") (Join-Path $PackageRoot "config\gateway.toml")
    Copy-Item (Join-Path $script:RepoRoot "README.md") (Join-Path $PackageRoot "README.md")
}

function New-LinuxLayout {
    param(
        [string]$PackageRoot,
        [string]$BinaryPath,
        [string]$ArtifactBinaryName
    )

    $binDir = Join-Path $PackageRoot "usr\bin"
    $configDir = Join-Path $PackageRoot "etc\gateway"
    $serviceDir = Join-Path $PackageRoot "usr\lib\systemd\system"
    $docDir = Join-Path $PackageRoot "usr\share\doc\rivulet-gateway"

    New-Item -ItemType Directory -Force $binDir, $configDir, $serviceDir, $docDir | Out-Null
    Copy-Item $BinaryPath (Join-Path $binDir $ArtifactBinaryName)
    Copy-Item (Join-Path $script:RepoRoot "packaging\examples\gateway.toml") (Join-Path $configDir "gateway.toml")
    Copy-Item (Join-Path $script:RepoRoot "packaging\linux\gateway.service") (Join-Path $serviceDir "rivulet-gateway.service")
    Copy-Item (Join-Path $script:RepoRoot "README.md") (Join-Path $docDir "README.md")
}

function Resolve-PackageFormat {
    param(
        [string]$Target,
        [string]$Format
    )

    if ($Format -ne "auto") {
        return $Format
    }

    if ($Target -like "*windows*") {
        return "zip"
    }

    return "tar.gz"
}

function Get-HostTarget {
    $hostLine = (& rustc -vV | Select-String "^host: ").ToString()
    if ([string]::IsNullOrWhiteSpace($hostLine)) {
        throw "unable to resolve rust host triple"
    }

    return $hostLine.Split(":")[1].Trim()
}

$script:Version = & (Join-Path $script:RepoRoot "scripts\get-version.ps1")

if ([string]::IsNullOrWhiteSpace($Target)) {
    $Target = Get-HostTarget
}

$resolvedFormat = Resolve-PackageFormat -Target $Target -Format $Format
$isWindowsTarget = $Target -like "*windows*"
$binaryName = if ($isWindowsTarget) { "gateway-main.exe" } else { "gateway-main" }
$artifactBinaryName = if ($isWindowsTarget) { "gateway.exe" } else { "gateway" }
$buildArgs = @("build", "-p", "gateway-main")

if ($Profile -eq "release") {
    $buildArgs += "--release"
}
if (-not [string]::IsNullOrWhiteSpace($Target)) {
    $buildArgs += @("--target", $Target)
}

if (-not $SkipBuild) {
    Write-Host "building Rivulet Gateway"
    Write-Host "target=$Target format=$resolvedFormat profile=$Profile"
    & cargo @buildArgs
} else {
    Write-Host "packaging existing build"
    Write-Host "target=$Target format=$resolvedFormat profile=$Profile"
}

$binaryPath = if ([string]::IsNullOrWhiteSpace($Target)) {
    Join-Path $script:RepoRoot "target\$Profile\$binaryName"
} else {
    Join-Path $script:RepoRoot "target\$Target\$Profile\$binaryName"
}

if (-not (Test-Path $binaryPath)) {
    throw "built binary not found: $binaryPath"
}

$distRoot = Join-Path $script:RepoRoot "dist\$Target"
$packageBaseName = "rivulet-gateway-$script:Version"
$packageRoot = if ($resolvedFormat -eq "dir") {
    Join-Path $distRoot $packageBaseName
} else {
    Join-Path $distRoot "$packageBaseName-staging"
}
if (Test-Path $packageRoot) {
    Remove-Item -Recurse -Force $packageRoot
}

if ($isWindowsTarget) {
    New-WindowsLayout -PackageRoot $packageRoot -BinaryPath $binaryPath -ArtifactBinaryName $artifactBinaryName
} else {
    New-LinuxLayout -PackageRoot $packageRoot -BinaryPath $binaryPath -ArtifactBinaryName $artifactBinaryName
}

switch ($resolvedFormat) {
    "dir" {
        Write-Host "package root: $packageRoot"
    }
    "zip" {
        $zipPath = Join-Path $distRoot "$packageBaseName.zip"
        if (Test-Path $zipPath) {
            Remove-Item -Force $zipPath
        }
        Compress-Archive -Path (Join-Path $packageRoot "*") -DestinationPath $zipPath
        Write-Host "archive: $zipPath"
        Remove-Item -Recurse -Force $packageRoot
    }
    "tar.gz" {
        $tarPath = Join-Path $distRoot "$packageBaseName.tar.gz"
        if (Test-Path $tarPath) {
            Remove-Item -Force $tarPath
        }
        tar -czf $tarPath -C $distRoot (Split-Path -Leaf $packageRoot)
        Write-Host "archive: $tarPath"
        Remove-Item -Recurse -Force $packageRoot
    }
    "rpm" {
        throw "rpm packaging should be built on Linux with scripts/package.sh --format rpm"
    }
}
