[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$workspaceManifest = Join-Path $repoRoot "Cargo.toml"
$content = Get-Content $workspaceManifest
$inWorkspacePackage = $false

foreach ($line in $content) {
    if ($line -match "^\[workspace\.package\]") {
        $inWorkspacePackage = $true
        continue
    }
    if ($inWorkspacePackage -and $line -match "^\[") {
        break
    }
    if ($inWorkspacePackage -and $line -match '^version\s*=\s*"([^"]+)"') {
        Write-Output $Matches[1]
        exit 0
    }
}

throw "workspace version not found"
