$ErrorActionPreference = 'Stop'
# Reuse the existing preview, path validation and junction-safe removal functions.
# Evidence was already copied and verified by prepare-current-cleanup.py; preserve
# the previous cleanup-evidence files rather than overwriting them.
. (Join-Path $PSScriptRoot '..\..\..\scripts\clean-workspace.ps1')
foreach ($taskEntry in $taskResolved | Where-Object Exists) {
    Find-ApplicationData $taskEntry.AbsolutePath
}
$taskBuildNames = @('cargo.exe','rustc.exe','node.exe','cl.exe','link.exe','jom.exe','nmake.exe','qmake.exe')
foreach ($taskProcess in Get-CimInstance Win32_Process) {
    foreach ($taskEntry in $taskResolved | Where-Object Exists) {
        if ($taskProcess.ExecutablePath -and ($taskProcess.ExecutablePath.Equals($taskEntry.AbsolutePath, [System.StringComparison]::OrdinalIgnoreCase) -or $taskProcess.ExecutablePath.StartsWith($taskEntry.AbsolutePath + '\', [System.StringComparison]::OrdinalIgnoreCase))) {
            throw "A cleanup target is in use: $($taskProcess.Name), PID $($taskProcess.ProcessId)."
        }
    }
    if ($taskProcess.Name -in $taskBuildNames -and $taskProcess.CommandLine -and $taskProcess.CommandLine.IndexOf($taskWorkspaceRoot, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        throw "Workspace build/test process is active: $($taskProcess.Name), PID $($taskProcess.ProcessId)."
    }
}
$taskDeleted = @()
foreach ($taskEntry in $taskResolved | Where-Object Exists) {
    Write-Output ('Removing cache: ' + $taskEntry.RelativePath)
    Remove-CacheItem $taskEntry.AbsolutePath
    $taskDeleted += $taskEntry.RelativePath
}
$taskExecution = [ordered]@{
    status='executed'; workspace=$taskWorkspaceRoot; date='2026-11-23'; expectedBytes=$taskExpectedBytes
    includeQtDebug=$false; paths=$taskDeleted
    evidenceManifest='docs/cleanup-history/2026-11-23/archived-evidence.json'
    currentDebugReleaseProgramsAndSymbolsPreserved=$true; applicationDataPreservedInPlace=$true
}
[System.IO.File]::WriteAllText((Join-Path $taskWorkspaceRoot 'docs\workspace-cleanup-execution.json'), ($taskExecution | ConvertTo-Json -Depth 6), [System.Text.UTF8Encoding]::new($false))
Write-Output ('Cleanup complete: ' + $taskDeleted.Count + ' targets; all Data and current development resources preserved.')
