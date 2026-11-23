param([switch]$Execute, [switch]$IncludeQtDebug)
$ErrorActionPreference = 'Stop'
$taskWorkspaceRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ((Get-Item -LiteralPath $taskWorkspaceRoot -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw 'The workspace root must be a real directory.' }
$taskRootPrefix = $taskWorkspaceRoot.TrimEnd('\') + '\'
$taskPlanFile = Join-Path $taskWorkspaceRoot 'docs\workspace-cleanup-plan.json'
$taskPlan = Get-Content -LiteralPath $taskPlanFile -Raw -Encoding UTF8 | ConvertFrom-Json
if (-not $taskWorkspaceRoot.Equals($taskPlan.workspace, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'The cleanup plan belongs to another workspace.' }
$taskOptionalPaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($taskEntry in $taskPlan.optionalQtDebug) { [void]$taskOptionalPaths.Add($taskEntry.path.Replace('/', '\')) }

function Assert-WorkspacePath([string]$Path) {
    $taskAbsolute = [System.IO.Path]::GetFullPath($Path)
    if (-not $taskAbsolute.StartsWith($taskRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) { throw "Path outside the workspace: $taskAbsolute" }
    $taskParent = [System.IO.Path]::GetDirectoryName($taskAbsolute)
    while ($taskParent -and -not $taskParent.Equals($taskWorkspaceRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        if ((Test-Path -LiteralPath $taskParent) -and ((Get-Item -LiteralPath $taskParent -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw "Parent is a directory link: $taskParent" }
        $taskParent = [System.IO.Path]::GetDirectoryName($taskParent)
    }
    return $taskAbsolute
}

function Resolve-CleanupTarget([string]$RelativePath) {
    if ([System.IO.Path]::IsPathRooted($RelativePath) -or ($RelativePath.Replace('/', '\').Split('\') -contains '..')) { throw 'Invalid cleanup path.' }
    $taskRelative = $RelativePath.Replace('/', '\')
    $taskCache = $taskRelative -eq '.work' -or $taskRelative.StartsWith('.work\', [System.StringComparison]::OrdinalIgnoreCase) -or $taskRelative.StartsWith('src-tauri\target\', [System.StringComparison]::OrdinalIgnoreCase) -or $taskRelative -in @('dist', 'test-results', 'aqtinstall.log')
    $taskDebug = $IncludeQtDebug -and $taskOptionalPaths.Contains($taskRelative)
    if (-not ($taskCache -or $taskDebug)) { throw "Path is not an approved cache or optional debug file: $taskRelative" }
    if ($taskRelative -ieq 'src-tauri\target\release\LLRemote.exe' -or $taskRelative.Split('\') -contains 'Data') { throw "Application data or the retained executable cannot be removed: $taskRelative" }
    $taskAbsolute = Assert-WorkspacePath (Join-Path $taskWorkspaceRoot $taskRelative)
    if ($taskDebug) {
        $taskName = [System.IO.Path]::GetFileNameWithoutExtension($taskAbsolute)
        $taskExtension = [System.IO.Path]::GetExtension($taskAbsolute)
        if (-not $taskName.EndsWith('d') -or $taskExtension -notin @('.dll', '.lib', '.pdb', '.exe')) { throw 'The optional file is not a Qt debug counterpart.' }
        $taskCounterpart = Join-Path ([System.IO.Path]::GetDirectoryName($taskAbsolute)) ($taskName.Substring(0, $taskName.Length - 1) + $taskExtension)
        if (-not (Test-Path -LiteralPath $taskCounterpart -PathType Leaf)) { throw "Release counterpart is missing: $taskCounterpart" }
    }
    return $taskAbsolute
}

function Find-ApplicationData([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $taskItem = Get-Item -LiteralPath $Path -Force
    if (($taskItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -or -not $taskItem.PSIsContainer) { return }
    if ($taskItem.Name -ieq 'Data') { throw "Preserve application Data before cleaning this cache: $Path" }
    foreach ($taskChild in Get-ChildItem -LiteralPath $Path -Directory -Force) { Find-ApplicationData $taskChild.FullName }
}

function Remove-CacheItem([string]$Path) {
    $taskAbsolute = Assert-WorkspacePath $Path
    if (-not (Test-Path -LiteralPath $taskAbsolute)) { return }
    $taskItem = Get-Item -LiteralPath $taskAbsolute -Force
    if ($taskItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
        # Delete the link itself. Never enumerate or recursively delete its target.
        if ($taskItem.PSIsContainer) { [System.IO.Directory]::Delete($taskAbsolute, $false) }
        else { [System.IO.File]::Delete($taskAbsolute) }
        return
    }
    if ($taskItem.PSIsContainer) {
        foreach ($taskChild in Get-ChildItem -LiteralPath $taskAbsolute -Force) { Remove-CacheItem $taskChild.FullName }
        [System.IO.Directory]::Delete($taskAbsolute, $false)
    } else { Remove-Item -LiteralPath $taskAbsolute -Force }
}

$taskSelected = @($taskPlan.recommended)
if ($IncludeQtDebug) { $taskSelected += @($taskPlan.optionalQtDebug) }
$taskResolved = @()
foreach ($taskEntry in $taskSelected) {
    $taskAbsolute = Resolve-CleanupTarget $taskEntry.path
    $taskResolved += [PSCustomObject]@{ RelativePath=$taskEntry.path; AbsolutePath=$taskAbsolute; ExpectedBytes=[long]$taskEntry.bytes; Exists=(Test-Path -LiteralPath $taskAbsolute) }
}
$taskExpectedBytes = [long](($taskResolved | Where-Object Exists | Measure-Object ExpectedBytes -Sum).Sum)
$taskResolved | Where-Object Exists | Select-Object RelativePath,@{Name='GiB';Expression={[Math]::Round($_.ExpectedBytes / 1GB, 3)}} | Format-Table -AutoSize
Write-Output ("Prepared cleanup: {0:N2} GiB. Execute = {1}." -f ($taskExpectedBytes / 1GB), [bool]$Execute)
if (-not $Execute) { Write-Output 'Preview only. No files were deleted or renamed.'; return }

# Complete all safety checks before the first removal or evidence copy.
foreach ($taskEntry in $taskResolved) {
    if ($taskEntry.Exists -and -not $taskEntry.RelativePath.StartsWith('.work', [System.StringComparison]::OrdinalIgnoreCase)) { Find-ApplicationData $taskEntry.AbsolutePath }
}
$taskBuildNames = @('cargo.exe','rustc.exe','node.exe','cl.exe','link.exe','jom.exe','nmake.exe','qmake.exe')
foreach ($taskProcess in Get-CimInstance Win32_Process) {
    foreach ($taskEntry in $taskResolved | Where-Object Exists) {
        if ($taskProcess.ExecutablePath -and ($taskProcess.ExecutablePath.Equals($taskEntry.AbsolutePath, [System.StringComparison]::OrdinalIgnoreCase) -or $taskProcess.ExecutablePath.StartsWith($taskEntry.AbsolutePath + '\', [System.StringComparison]::OrdinalIgnoreCase))) { throw "A process is running from a cleanup target: $($taskProcess.Name), PID $($taskProcess.ProcessId)." }
    }
    if ($taskProcess.Name -in $taskBuildNames -and $taskProcess.CommandLine -and $taskProcess.CommandLine.IndexOf($taskWorkspaceRoot, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "Workspace build or test process is active: $($taskProcess.Name), PID $($taskProcess.ProcessId)." }
}

$taskEvidenceDirectory = Join-Path $taskWorkspaceRoot 'docs\cleanup-evidence'
$taskWorkDirectory = Join-Path $taskWorkspaceRoot '.work'
if (Test-Path -LiteralPath $taskWorkDirectory) {
    New-Item -ItemType Directory -Path $taskEvidenceDirectory -Force | Out-Null
    foreach ($taskEvidence in Get-ChildItem -LiteralPath $taskWorkDirectory -File -Force | Where-Object { $_.Extension -eq '.json' -or $_.Name -in @('moonlight-build.log','diagnose-pairing.mjs') }) {
        Copy-Item -LiteralPath $taskEvidence.FullName -Destination $taskEvidenceDirectory -Force
    }
}
foreach ($taskEntry in $taskResolved | Where-Object Exists) {
    Write-Output ("Removing cache: " + $taskEntry.RelativePath)
    Remove-CacheItem $taskEntry.AbsolutePath
}
# Build scripts use this scratch root for logs before making their own subfolders.
if ($taskResolved.RelativePath -contains '.work') { New-Item -ItemType Directory -Path $taskWorkDirectory -Force | Out-Null }
$taskUtf8 = [System.Text.UTF8Encoding]::new($false)
$taskExecution = [ordered]@{ status='executed'; workspace=$taskWorkspaceRoot; expectedBytes=$taskExpectedBytes; includeQtDebug=[bool]$IncludeQtDebug; paths=@($taskResolved | Where-Object Exists | ForEach-Object RelativePath) }
[System.IO.File]::WriteAllText((Join-Path $taskWorkspaceRoot 'docs\workspace-cleanup-execution.json'), ($taskExecution | ConvertTo-Json -Depth 6), $taskUtf8)
Write-Output 'Cleanup complete. Formal release, source, runtime inputs, and application Data were preserved.'
