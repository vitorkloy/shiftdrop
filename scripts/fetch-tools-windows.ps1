# Baixa as ferramentas embutidas no instalador Windows para src-tauri/resources/tools.
# Usado no CI (release.yml) e localmente antes de `npx tauri build`.
# Cada executável sai com as DLLs que importa (ffmpeg compartilhado, Ghostscript,
# e o runtime VC++ — em especial VCOMP140.DLL do OpenMP, que o magick portátil exige).
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

Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Collections.Generic;
using System.Text;
public static class SdPe {
  public static string[] Imports(string path) {
    var names = new List<string>();
    using (var fs = File.OpenRead(path)) {
      var br = new BinaryReader(fs);
      fs.Seek(0x3C, SeekOrigin.Begin);
      int pe = br.ReadInt32();
      fs.Seek(pe, SeekOrigin.Begin);
      if (br.ReadUInt32() != 0x4550) return new string[0];
      ushort magic;
      fs.Seek(pe + 24, SeekOrigin.Begin);
      magic = br.ReadUInt16();
      bool plus = magic == 0x20b;
      int opt = pe + 24;
      int dd = opt + (plus ? 112 : 96);
      fs.Seek(pe + 6, SeekOrigin.Begin);
      ushort sections = br.ReadUInt16();
      fs.Seek(pe + 20, SeekOrigin.Begin);
      ushort optSize = br.ReadUInt16();
      int secOff = pe + 24 + optSize;
      var secs = new List<uint[]>();
      for (int i = 0; i < sections; i++) {
        fs.Seek(secOff + i * 40 + 8, SeekOrigin.Begin);
        uint vsz = br.ReadUInt32();
        uint va = br.ReadUInt32();
        uint rawsz = br.ReadUInt32();
        uint raw = br.ReadUInt32();
        secs.Add(new uint[] { va, vsz, raw, rawsz });
      }
      Func<uint, long> off = rva => {
        foreach (var s in secs) {
          uint span = Math.Max(s[1], s[3]);
          if (rva >= s[0] && rva < s[0] + span) return s[2] + (rva - s[0]);
        }
        return -1;
      };
      Action<uint> readImport = rva => {
        long o = off(rva);
        if (o < 0) return;
        for (int n = 0; n < 256; n++) {
          fs.Seek(o + n * 20 + 12, SeekOrigin.Begin);
          uint nameRva = br.ReadUInt32();
          if (nameRva == 0) break;
          long no = off(nameRva);
          if (no < 0) continue;
          fs.Seek(no, SeekOrigin.Begin);
          var sb = new StringBuilder();
          for (;;) { int b = br.ReadByte(); if (b <= 0) break; sb.Append((char)b); }
          if (sb.Length > 0) names.Add(sb.ToString());
        }
      };
      fs.Seek(dd + 8, SeekOrigin.Begin);
      uint impRva = br.ReadUInt32();
      if (impRva != 0) readImport(impRva);
      fs.Seek(dd + 13 * 8, SeekOrigin.Begin);
      uint delayRva = br.ReadUInt32();
      if (delayRva != 0) {
        long o = off(delayRva);
        if (o >= 0) {
          for (int n = 0; n < 64; n++) {
            fs.Seek(o + n * 32, SeekOrigin.Begin);
            uint attr = br.ReadUInt32();
            uint nameRva = br.ReadUInt32();
            if (nameRva == 0) break;
            if ((attr & 1) != 0) continue;
            long no = off(nameRva);
            if (no < 0) continue;
            fs.Seek(no, SeekOrigin.Begin);
            var sb = new StringBuilder();
            for (;;) { int b = br.ReadByte(); if (b <= 0) break; sb.Append((char)b); }
            if (sb.Length > 0) names.Add(sb.ToString());
          }
        }
      }
    }
    return names.ToArray();
  }
  public static bool IsX64(string path) {
    using (var fs = File.OpenRead(path)) {
      var br = new BinaryReader(fs);
      fs.Seek(0x3C, SeekOrigin.Begin);
      int pe = br.ReadInt32();
      fs.Seek(pe + 4, SeekOrigin.Begin);
      return br.ReadUInt16() == 0x8664;
    }
  }
}
'@

$vcNames = @(
  'vcomp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll', 'vcruntime140_threads.dll',
  'msvcp140.dll', 'msvcp140_1.dll', 'msvcp140_2.dll', 'msvcp140_atomic_wait.dll',
  'msvcp140_codecvt_ids.dll', 'concrt140.dll'
)

function Get-VcDir {
  $cache = Join-Path $tmp 'vcruntime'
  if (Test-Path (Join-Path $cache 'vcomp140.dll')) { return $cache }
  New-Item -ItemType Directory -Force $cache | Out-Null
  $redist = Join-Path $tmp 'vc_redist.x64.exe'
  if (-not (Test-Path $redist)) {
    Get 'https://aka.ms/vs/17/release/vc_redist.x64.exe' $redist
  }
  $bytes = [IO.File]::ReadAllBytes($redist)
  $hits = New-Object System.Collections.Generic.List[int]
  for ($i = 0; $i -le $bytes.Length - 4; $i++) {
    if ($bytes[$i] -eq 0x4D -and $bytes[$i+1] -eq 0x53 -and $bytes[$i+2] -eq 0x43 -and $bytes[$i+3] -eq 0x46) {
      $hits.Add($i)
    }
  }
  $cabRoot = Join-Path $tmp 'vc-cabs'
  New-Item -ItemType Directory -Force $cabRoot | Out-Null
  $queue = New-Object System.Collections.Generic.Queue[string]
  for ($k = 0; $k -lt $hits.Count; $k++) {
    $start = $hits[$k]
    $end = if ($k + 1 -lt $hits.Count) { $hits[$k + 1] } else { $bytes.Length }
    if ($end - $start -lt 1000) { continue }
    $cab = Join-Path $cabRoot ("top{0}.cab" -f $k)
    $slice = New-Object byte[] ($end - $start)
    [Array]::Copy($bytes, $start, $slice, 0, $slice.Length)
    [IO.File]::WriteAllBytes($cab, $slice)
    $queue.Enqueue($cab)
  }
  $n = 0
  while ($queue.Count -gt 0 -and $n -lt 40) {
    $cab = $queue.Dequeue()
    $dest = Join-Path $cabRoot ("x{0}" -f $n)
    $n++
    New-Item -ItemType Directory -Force $dest | Out-Null
    $p = Start-Process -FilePath "$env:SystemRoot\System32\expand.exe" -ArgumentList @($cab, '-F:*', $dest) -Wait -PassThru -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { continue }
    Get-ChildItem $dest -File | ForEach-Object {
      $head = New-Object byte[] 4
      $fs = [IO.File]::OpenRead($_.FullName)
      [void]$fs.Read($head, 0, 4)
      $fs.Close()
      if ($head[0] -eq 0x4D -and $head[1] -eq 0x53 -and $head[2] -eq 0x43 -and $head[3] -eq 0x46) {
        $queue.Enqueue($_.FullName)
      }
    }
  }
  foreach ($name in $vcNames) {
    $stem = [IO.Path]::GetFileNameWithoutExtension($name)
    $found = Get-ChildItem $cabRoot -Recurse -File | Where-Object {
      $_.Name -like "$stem.dll*" -and $_.Name -notlike '*arm64*' -and $_.Name -like '*amd64*'
    } | Where-Object { [SdPe]::IsX64($_.FullName) } | Select-Object -First 1
    if ($found) { Copy-Item $found.FullName (Join-Path $cache $name) -Force }
  }
  if (-not (Test-Path (Join-Path $cache 'vcomp140.dll'))) {
    throw 'não achei VCOMP140.DLL (x64) no Visual C++ Redistributable oficial'
  }
  Set-Content -Encoding utf8 (Join-Path $cache 'LICENSE-vcredist.txt') @"
Microsoft Visual C++ 2015-2022 Runtime, cópia local (app-local).
DLLs extraídas do instalador oficial vc_redist.x64.exe
(https://aka.ms/vs/17/release/vc_redist.x64.exe).
A licença de redistribuição da Microsoft permite distribuir estes arquivos junto do aplicativo.
"@
  return $cache
}

function Repair-ToolDir([string]$dir) {
  if (-not (Test-Path $dir)) { return }
  $bins = @(Get-ChildItem $dir -File | Where-Object { $_.Extension -match '^\.(exe|dll)$' })
  $need = New-Object System.Collections.Generic.HashSet[string]
  $fatal = @()
  foreach ($b in $bins) {
    foreach ($imp in [SdPe]::Imports($b.FullName)) {
      if (Test-Path (Join-Path $dir $imp)) { continue }
      $low = $imp.ToLowerInvariant()
      if ($vcNames -contains $low) { [void]$need.Add($low); continue }
      $sys = Join-Path "$env:SystemRoot\System32" $imp
      if (Test-Path $sys) { continue }
      $fatal += "$($b.Name) importa $imp"
    }
  }
  if ($need.Count -gt 0) {
    $vc = Get-VcDir
    Copy-Item (Join-Path $vc 'LICENSE-vcredist.txt') (Join-Path $dir 'LICENSE-vcredist.txt') -Force -ErrorAction SilentlyContinue
    foreach ($low in $need) {
      $src = Join-Path $vc $low
      if (-not (Test-Path $src)) { $fatal += "runtime $low ausente no pacote oficial"; continue }
      Copy-Item $src (Join-Path $dir $low) -Force
      Write-Host "runtime $low -> $dir"
    }
  }
  if ($fatal.Count) { throw ("Dependência não empacotada em ${dir}:`n" + ($fatal -join "`n")) }
}

# ffmpeg + ffprobe (gyan.dev essentials, GPL) e as DLLs do mesmo bin\
if (-not (Test-Path "$root\ffmpeg.exe")) {
  Get 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' "$tmp\ff.zip"
  Expand-Archive "$tmp\ff.zip" "$tmp\ff" -Force
  $ff = Get-ChildItem "$tmp\ff" -Recurse -Filter ffmpeg.exe | Select-Object -First 1
  if (-not $ff) { throw 'ffmpeg.exe não veio no zip' }
  Copy-Item (Join-Path $ff.DirectoryName 'ffmpeg.exe'), (Join-Path $ff.DirectoryName 'ffprobe.exe') $root
  Get-ChildItem $ff.DirectoryName -Filter *.dll | Copy-Item -Destination $root
  Get-ChildItem "$tmp\ff" -Recurse -Filter LICENSE | Select-Object -First 1 | Copy-Item -Destination "$root\LICENSE-ffmpeg.txt"
}

# ImageMagick portable (Q16 x64). O .exe é estático nas libs próprias, mas importa VCOMP140.DLL.
if (-not (Test-Path "$root\magick\magick.exe")) {
  Get (Asset 'ImageMagick/ImageMagick' 'portable-Q16-x64\.7z$') "$tmp\im.7z"
  New-Item -ItemType Directory -Force "$root\magick" | Out-Null
  & "$env:SystemRoot\System32\tar.exe" -xf "$tmp\im.7z" -C "$root\magick"
  if (-not $?) { throw 'falha ao extrair ImageMagick' }
  $inner = Get-ChildItem "$root\magick" -Directory | Where-Object { Test-Path "$($_.FullName)\magick.exe" } | Select-Object -First 1
  if ($inner) { Get-ChildItem $inner.FullName | Move-Item -Destination "$root\magick" -Force; Remove-Item $inner.FullName -Recurse -Force }
  Get-ChildItem "$root\magick" -Filter *.exe | Where-Object Name -ne 'magick.exe' | Remove-Item -Force
}

# pandoc (exe e qualquer DLL que viaje no zip)
if (-not (Test-Path "$root\pandoc.exe")) {
  Get (Asset 'jgm/pandoc' 'windows-x86_64\.zip$') "$tmp\pd.zip"
  Expand-Archive "$tmp\pd.zip" "$tmp\pd" -Force
  $pd = Get-ChildItem "$tmp\pd" -Recurse -Filter pandoc.exe | Select-Object -First 1
  if (-not $pd) { throw 'pandoc.exe não veio no zip' }
  Copy-Item $pd.FullName $root
  Get-ChildItem $pd.DirectoryName -Filter *.dll | Copy-Item -Destination $root
  Get-ChildItem "$tmp\pd" -Recurse -Filter COPYING* | Select-Object -First 1 | Copy-Item -Destination "$root\LICENSE-pandoc.txt"
}

# Ghostscript (AGPL): binário, DLLs e a pasta lib (gs_init.ps). Sem isso o PDF não converte.
if (-not (Test-Path "$root\gs\bin\gswin64c.exe") -and -not (Test-Path "$root\gs\gswin64c.exe")) {
  New-Item -ItemType Directory -Force "$root\gs" | Out-Null
  $7z = (Get-Command 7z -ErrorAction SilentlyContinue).Source
  if (-not $7z -and (Test-Path 'C:\Program Files\7-Zip\7z.exe')) { $7z = 'C:\Program Files\7-Zip\7z.exe' }
  if ($7z) {
    Get (Asset 'ArtifexSoftware/ghostpdl-downloads' 'gs\d+w64\.exe$') "$tmp\gs.exe"
    & $7z x "$tmp\gs.exe" "-o$tmp\gs" -y | Out-Null
  } else {
    $inst = Get-ChildItem 'C:\Program Files\gs' -Directory -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
    if ($inst) {
      New-Item -ItemType Directory -Force "$tmp\gs" | Out-Null
      Copy-Item $inst.FullName "$tmp\gs\gs" -Recurse
    } else {
      Write-Warning 'Ghostscript não empacotado (sem 7-Zip). A conversão de imagem segue com o ImageMagick.'
    }
  }
  $gsBin = if (Test-Path "$tmp\gs") { Get-ChildItem "$tmp\gs" -Recurse -Filter gswin64c.exe -ErrorAction SilentlyContinue | Select-Object -First 1 } else { $null }
  if ($gsBin) {
    $gsHome = $gsBin.Directory.Parent.FullName
    New-Item -ItemType Directory -Force "$root\gs\bin" | Out-Null
    Copy-Item $gsBin.FullName "$root\gs\bin"
    Get-ChildItem $gsBin.DirectoryName -Filter *.dll | Copy-Item -Destination "$root\gs\bin"
    if (Test-Path "$gsHome\lib") { Copy-Item "$gsHome\lib" "$root\gs\lib" -Recurse -Force }
    Get-ChildItem "$tmp\gs" -Recurse -Filter LICENSE -File -ErrorAction SilentlyContinue | Select-Object -First 1 | Copy-Item -Destination "$root\gs\LICENSE-ghostscript.txt" -ErrorAction SilentlyContinue
  }
}

Repair-ToolDir (Join-Path $root 'magick')
if (-not (Test-Path "$root\magick\vcomp140.dll")) { throw 'magick.exe ficou sem VCOMP140.DLL ao lado' }
if (-not (Test-Path "$root\ffmpeg.exe")) { throw 'ffmpeg.exe não foi empacotado' }

Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
Get-ChildItem $root -Recurse -File | Measure-Object Length -Sum | ForEach-Object { 'tools: {0:N0} MB' -f ($_.Sum / 1MB) }
