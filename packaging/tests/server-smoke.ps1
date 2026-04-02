[CmdletBinding()]
param(
    [string]$Binary = ".\\gateway.exe",
    [int]$Port = 18080
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rivulet-smoke-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $tempRoot | Out-Null
$configPath = Join-Path $tempRoot "gateway.toml"
$process = $null

function Invoke-SmokeRequest {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Uri,
        [Parameter(Mandatory = $true)]
        [string]$HostHeader
    )

    try {
        return Invoke-WebRequest -Uri $Uri -Headers @{ Host = $HostHeader }
    }
    catch {
        $response = $_.Exception.Response
        if ($null -eq $response) {
            throw
        }

        return [pscustomobject]@{
            StatusCode = [int]$response.StatusCode
        }
    }
}

try {
    @"
[runtime]
worker_threads = 4
graceful_shutdown_secs = 5
downstream_read_timeout_ms = 5000
upstream_connect_timeout_ms = 1000
upstream_read_timeout_ms = 1000
upstream_retry_attempts = 1
upstream_idle_pool_size = 1
max_upstream_status_line_bytes = 8192
max_upstream_headers = 100
max_upstream_header_bytes = 65536
max_upstream_body_bytes = 8388608
max_request_line_bytes = 8192
max_request_headers = 100
max_request_body_bytes = 1048576

[[listeners]]
name = "smoke"
address = "127.0.0.1:$Port"
protocol = "http1"

[[upstreams]]
name = "missing-upstream"
load_balance = "round_robin"

[[upstreams.endpoints]]
address = "127.0.0.1:19090"
weight = 1

[[routes]]
name = "default"
listener = "smoke"
hosts = ["smoke.test"]
path_prefixes = ["/"]
methods = []
upstream = "missing-upstream"
filters = ["request-id"]
"@ | Set-Content -Encoding utf8 $configPath

    $stdoutPath = Join-Path $tempRoot "stdout.log"
    $stderrPath = Join-Path $tempRoot "stderr.log"
    $process = Start-Process -FilePath $Binary -ArgumentList $configPath -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    Start-Sleep -Seconds 1

    $ok = Invoke-SmokeRequest -Uri "http://127.0.0.1:$Port/ok" -HostHeader "smoke.test"
    $missing = Invoke-SmokeRequest -Uri "http://127.0.0.1:$Port/missing" -HostHeader "other.test"

    if ($ok.StatusCode -ne 502) {
        throw "expected 502 from missing upstream, got $($ok.StatusCode)"
    }
    if ($missing.StatusCode -ne 404) {
        throw "expected 404 from unmatched route, got $($missing.StatusCode)"
    }

    Write-Host "server smoke passed on port $Port"
}
finally {
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
    }
    if (Test-Path $tempRoot) {
        Remove-Item -Recurse -Force $tempRoot
    }
}
