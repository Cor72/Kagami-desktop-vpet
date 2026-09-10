param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [string]$Executable = (Join-Path $PSScriptRoot '../src-tauri/target/release/yachiyo-desktop.exe')
)
$ErrorActionPreference = 'Stop'
$auditDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if ((Test-Path -LiteralPath $auditDirectory) -and (Get-ChildItem -LiteralPath $auditDirectory -Force | Select-Object -First 1)) {
    throw '请选择新的空目录，避免覆盖或混合已有采样。'
}
New-Item -ItemType Directory -Path $auditDirectory -Force | Out-Null
$processor = Get-CimInstance Win32_Processor
$computer = Get-CimInstance Win32_ComputerSystem
$operatingSystem = Get-CimInstance Win32_OperatingSystem
$graphics = Get-CimInstance Win32_VideoController
[ordered]@{
    startedAt = [DateTimeOffset]::Now.ToString('o')
    cpu = @($processor | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors)
    logicalProcessors = [Environment]::ProcessorCount
    physicalMemoryBytes = $computer.TotalPhysicalMemory
    os = $operatingSystem.Caption
    osVersion = $operatingSystem.Version
    gpu = @($graphics | Select-Object Name, DriverVersion, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate)
    sampleIntervalSeconds = 5
    cpuDefinition = 'Delta total CPU seconds / wall seconds / logical processors * 100; parent plus descendant processes'
    workingSetDefinition = 'Sum of process working sets, shared pages may be counted more than once'
    privateBytesDefinition = 'Sum of process private committed bytes, not physical resident memory'
    gpuMemoryDefinition = 'Sum of Windows GPU Process Memory counters for this process tree'
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $auditDirectory 'machine.json') -Encoding utf8

$env:YACHIYO_AUDIT_DIR = $auditDirectory
$auditProcess = Start-Process -FilePath ([IO.Path]::GetFullPath($Executable)) -WorkingDirectory (Split-Path ([IO.Path]::GetFullPath($Executable))) -WindowStyle Hidden -PassThru
Remove-Item Env:YACHIYO_AUDIT_DIR
$previousCpu = @{}
$previousTime = [DateTimeOffset]::Now
$processPath = Join-Path $auditDirectory 'processes.jsonl'
$gpuError = $null
while (-not $auditProcess.HasExited) {
    $now = [DateTimeOffset]::Now
    $allProcesses = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($auditProcess.Id)
    do {
        $added = $false
        foreach ($item in $allProcesses) {
            if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) { $added = $true }
        }
    } while ($added)
    $cpuDelta = 0.0
    $processes = @()
    foreach ($processId in $ids) {
        $item = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if ($null -eq $item) { continue }
        $cpuSeconds = $item.TotalProcessorTime.TotalSeconds
        if ($previousCpu.ContainsKey($processId)) { $cpuDelta += [Math]::Max(0, $cpuSeconds - $previousCpu[$processId]) }
        $previousCpu[$processId] = $cpuSeconds
        $processes += [pscustomobject]@{ pid = $processId; name = $item.ProcessName; cpuSeconds = $cpuSeconds; workingSetBytes = $item.WorkingSet64; privateBytes = $item.PrivateMemorySize64 }
    }
    $gpuRows = @()
    try {
        $gpuRows = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory | Where-Object { $_.Name -match '^pid_(\d+)_' -and $ids.Contains([int]$Matches[1]) } | Select-Object Name, DedicatedUsage, SharedUsage, TotalCommitted)
    } catch { $gpuError = $_.Exception.Message }
    $phase = 'startup'
    $statusPath = Join-Path $auditDirectory 'status.json'
    if (Test-Path -LiteralPath $statusPath) {
        try { $phase = (Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json).phase } catch { $phase = 'status-write-in-progress' }
    }
    [ordered]@{
        timestampMs = $now.ToUnixTimeMilliseconds(); phase = $phase
        cpuPercent = $cpuDelta / [Math]::Max(0.001, ($now - $previousTime).TotalSeconds) / [Environment]::ProcessorCount * 100
        workingSetBytes = ($processes | Measure-Object -Property workingSetBytes -Sum).Sum
        privateBytes = ($processes | Measure-Object -Property privateBytes -Sum).Sum
        processes = $processes; gpuMemory = $gpuRows; gpuCounterError = $gpuError
    } | ConvertTo-Json -Depth 5 -Compress | Add-Content -LiteralPath $processPath -Encoding utf8
    $previousTime = $now
    Start-Sleep -Seconds 5
    $auditProcess.Refresh()
}
if (Test-Path -LiteralPath (Join-Path $auditDirectory 'error.txt')) { throw (Get-Content -LiteralPath (Join-Path $auditDirectory 'error.txt') -Raw) }
if (-not (Test-Path -LiteralPath (Join-Path $auditDirectory 'complete.json'))) { throw '验收进程提前退出，未完成全部阶段。' }
Write-Output "Performance audit complete: $auditDirectory"
