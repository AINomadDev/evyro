# Install the `evyro` CLI on Windows from a GitHub release.
#
#   irm https://raw.githubusercontent.com/AINomadDev/evyro/main/scripts/install.ps1 | iex
#
# Options (environment variables):
#   EVYRO_VERSION   a version such as 0.2.0 (default: the latest release)
#   EVYRO_BIN_DIR   where to put evyro.exe (default: %LOCALAPPDATA%\Programs\evyro)
#
# The archive is checked against the release's SHA256SUMS before anything is
# installed. The folder is added to your user PATH only.
$ErrorActionPreference = 'Stop'
$repo = 'AINomadDev/evyro'
$binDir = if ($env:EVYRO_BIN_DIR) { $env:EVYRO_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\evyro' }
$target = 'x86_64-pc-windows-msvc'

if ($env:EVYRO_VERSION) {
  $version = $env:EVYRO_VERSION.TrimStart('v')
  $base = "https://github.com/$repo/releases/download/v$version"
} else {
  $version = 'latest'
  $base = "https://github.com/$repo/releases/latest/download"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("evyro-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Write-Host "evyro-install: downloading the $version release for $target"
  $sums = Join-Path $tmp 'SHA256SUMS'
  Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sums
  $line = Get-Content $sums | Where-Object { $_ -match "evyro-\S*-$target\.zip$" } | Select-Object -First 1
  if (-not $line) { throw "this release has no CLI for $target" }
  $parts = $line -split '\s+'
  $want = $parts[0].ToLower()
  $archive = $parts[1].TrimStart('*')
  $zip = Join-Path $tmp $archive
  Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive" -OutFile $zip
  $got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
  if ($want -ne $got) { throw "checksum mismatch for $archive (expected $want, got $got); nothing was installed" }
  Write-Host 'evyro-install: checksum ok'
  Expand-Archive -Path $zip -DestinationPath $tmp -Force
  $exe = Get-ChildItem $tmp -Recurse -Filter evyro.exe | Select-Object -First 1
  if (-not $exe) { throw 'the archive has no evyro.exe' }
  New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  Copy-Item $exe.FullName (Join-Path $binDir 'evyro.exe') -Force
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not ($userPath -split ';' | Where-Object { $_ -eq $binDir })) {
    [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ';' + $binDir), 'User')
    Write-Host "evyro-install: added $binDir to your user PATH; open a new terminal to use it"
  }
  Write-Host "evyro-install: installed $binDir\evyro.exe"
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
