# Mede CPU% e RAM (working set) do ShiftDrop + filhos (WebView2). Uso: measure.ps1 <rótulo>
param([string]$Label = 'run', [int]$IdleSecs = 60)
$exe = "$env:LOCALAPPDATA\ShiftDrop\shiftdrop.exe"
Get-Process shiftdrop -EA 0 | Stop-Process -Force; Start-Sleep 1
function Tree($root) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
  $ids = @($root); $grew = $true
  while ($grew) { $grew = $false; foreach ($p in $all) { if ($ids -contains $p.ParentProcessId -and $ids -notcontains $p.ProcessId) { $ids += $p.ProcessId; $grew = $true } } }
  $ids
}
function CpuMs($ids) { $t = 0.0; foreach ($p in (Get-Process -Id $ids -EA 0)) { $t += $p.TotalProcessorTime.TotalMilliseconds }; $t }
function Ws($ids) { $t = 0.0; foreach ($p in (Get-Process -Id $ids -EA 0)) { $t += $p.WorkingSet64 }; $t }
function Sample($root, $secs) {
  $ids = Tree $root; $c0 = CpuMs $ids; $t0 = Get-Date; $peak = 0.0
  for ($i = 0; $i -lt $secs; $i++) { Start-Sleep 1; $ids = Tree $root; $w = Ws $ids; if ($w -gt $peak) { $peak = $w } }
  $c1 = CpuMs $ids; $el = ((Get-Date) - $t0).TotalMilliseconds * [Environment]::ProcessorCount
  [pscustomobject]@{ procs = @(Get-Process -Id $ids -EA 0).Count; cpu_pct = [math]::Round(100 * [math]::Max(0, $c1 - $c0) / $el, 3); ram_mb = [math]::Round((Ws $ids) / 1MB, 1); peak_mb = [math]::Round($peak / 1MB, 1) }
}
$p = Start-Process $exe -PassThru; Start-Sleep 8; $p = Get-Process shiftdrop | Sort-Object StartTime | Select-Object -First 1
$idle = Sample $p.Id $IdleSecs
$f = "$env:USERPROFILE\Pictures\ShiftDrop-demo\cat6.JPEG"
Start-Sleep 2; $after = Sample $p.Id 10
Get-Process shiftdrop -EA 0 | Stop-Process -Force; Start-Sleep 1
# Arraste: instância própria do autoteste OLE (abre a roda e passeia pelos setores).
$s = Start-Process $exe -ArgumentList 'selftest-drag', "`"$f`"" -PassThru; Start-Sleep 2
$drag = Sample $s.Id 6
Get-Process shiftdrop -EA 0 | Stop-Process -Force
"$Label ocioso: procs=$($idle.procs) cpu=$($idle.cpu_pct)% ram=$($idle.ram_mb)MB"
"$Label arraste: procs=$($drag.procs) cpu=$($drag.cpu_pct)% pico=$($drag.peak_mb)MB"
"$Label ocioso+10s: cpu=$($after.cpu_pct)% ram=$($after.ram_mb)MB"
