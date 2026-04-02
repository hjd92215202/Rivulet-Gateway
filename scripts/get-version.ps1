[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$workspaceManifest = Join-Path $repoRoot "Cargo.toml"
$content = Get-Content $workspaceManifest
$inWorkspacePackage = $false

function Normalize-Version {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    # Strip a leading v so tag-based release assets and package versions stay aligned.
    if ($Value.StartsWith("v")) {
        return $Value.Substring(1)
    }

    return $Value
}

if (-not [string]::IsNullOrWhiteSpace($env:RIVULET_RELEASE_VERSION)) {
    Write-Output (Normalize-Version -Value $env:RIVULET_RELEASE_VERSION)
    exit 0
}

if ($env:GITHUB_REF_TYPE -eq "tag" -and -not [string]::IsNullOrWhiteSpace($env:GITHUB_REF_NAME)) {
    Write-Output (Normalize-Version -Value $env:GITHUB_REF_NAME)
    exit 0
}

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
