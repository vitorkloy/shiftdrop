# Baixa as ferramentas embutidas no instalador Windows para src-tauri/resources/tools.
# Usado no CI (release.yml) e localmente antes de `npx tauri build`.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root = Join-Path $PSScriptRoot '..\src-tauri\resources\tools'
$tmp = Join-Path $env:TEMP 'sd-fetch'
New-Item -ItemType Directory -Force $root, $tmp | Out-Null
$gh = @{ 'User-Agent' = 'shiftdrop-ci' }
if ($env:GITHUB_TOKEN) { $gh['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
function Asset($repo, $pattern) {
  $r = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $gh
  ($r.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1).browser_download_url
}
function Get($url, $out) { Write-Host "-> $url"; Invoke-WebRequest $url -OutFile $out -UseBasicParsing }

# ffmpeg + ffprobe (gyan.dev essentials, GPL)
if (-not (Test-Path "$root\ffmpeg.exe")) {
  Get 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' "$tmp\ff.zip"
  Expand-Archive "$tmp\ff.zip" "$tmp\ff" -Force
  Get-ChildItem "$tmp\ff" -Recurse -Include ffmpeg.exe, ffprobe.exe | Copy-Item -Destination $root
  Get-ChildItem "$tmp\ff" -Recurse -Filter LICENSE | Select-Object -First 1 | Copy-Item -Destination "$root\LICENSE-ffmpeg.txt"
}

# ImageMagick portable (Q16 x64)
if (-not (Test-Path "$root\magick\magick.exe")) {
  Get (Asset 'ImageMagick/ImageMagick' 'portable-Q16-x64\.7z$') "$tmp\im.7z"
  New-Item -ItemType Directory -Force "$root\magick" | Out-Null
  & "$env:SystemRoot\System32\tar.exe" -xf "$tmp\im.7z" -C "$root\magick"
  if (-not $?) { throw 'falha ao extrair ImageMagick' }
  $inner = Get-ChildItem "$root\magick" -Directory | Where-Object { Test-Path "$($_.FullName)\magick.exe" } | Select-Object -First 1
  if ($inner) { Get-ChildItem $inner.FullName | Move-Item -Destination "$root\magick" -Force; Remove-Item $inner.FullName -Recurse -Force }
  # O portable é estático: cada .exe repete a biblioteca inteira. Só o magick.exe é usado.
  Get-ChildItem "$root\magick" -Filter *.exe | Where-Object Name -ne 'magick.exe' | Remove-Item -Force
}

# pandoc
if (-not (Test-Path "$root\pandoc.exe")) {
  Get (Asset 'jgm/pandoc' 'windows-x86_64\.zip$') "$tmp\pd.zip"
  Expand-Archive "$tmp\pd.zip" "$tmp\pd" -Force
  Get-ChildItem "$tmp\pd" -Recurse -Filter pandoc.exe | Copy-Item -Destination $root
  Get-ChildItem "$tmp\pd" -Recurse -Filter COPYING* | Select-Object -First 1 | Copy-Item -Destination "$root\LICENSE-pandoc.txt"
}

# Ghostscript (AGPL): extrai o instalador NSIS com 7-Zip; sem 7z, copia o instalado.
if (-not (Test-Path "$root\gs\gswin64c.exe")) {
  New-Item -ItemType Directory -Force "$root\gs" | Out-Null
  $7z = (Get-Command 7z -ErrorAction SilentlyContinue).Source
  if (-not $7z -and (Test-Path 'C:\Program Files\7-Zip\7z.exe')) { $7z = 'C:\Program Files\7-Zip\7z.exe' }
  if ($7z) {
    Get (Asset 'ArtifexSoftware/ghostpdl-downloads' 'gs\d+w64\.exe$') "$tmp\gs.exe"
    & $7z x "$tmp\gs.exe" "-o$tmp\gs" -y | Out-Null
    Copy-Item "$tmp\gs\bin\gswin64c.exe", "$tmp\gs\bin\gsdll64.dll" "$root\gs"
  } else {
    $inst = Get-ChildItem 'C:\Program Files\gs' -Directory | Sort-Object Name | Select-Object -Last 1
    Copy-Item "$($inst.FullName)\bin\gswin64c.exe", "$($inst.FullName)\bin\gsdll64.dll" "$root\gs"
  }
}
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
Get-ChildItem $root -Recurse -File | Measure-Object Length -Sum | ForEach-Object { 'tools: {0:N0} MB' -f ($_.Sum / 1MB) }
