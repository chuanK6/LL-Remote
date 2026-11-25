param([Parameter(Mandatory = $true)][string[]]$TemporaryPaths, [switch]$Execute)
$ErrorActionPreference = 'Stop'
$taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskWorkRoot = Join-Path $taskWorkspace '.work'
if (-not (Test-Path -LiteralPath $taskWorkRoot) -or ((Get-Item -LiteralPath $taskWorkRoot -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw 'Workspace .work must be a real directory.' }
$taskAllowedNames = @('package-current','fix-0.1.14','history-0.1.14','history-delete-0.1.14','wallpaper-reconnect-0.1.14','github-release-assets')
$taskAllowedPrefixes = @('archive-0.1.','previous-local-0.1.','github-release-','before-device-login-update-','before-overlay-update-')
$taskTargets = @()
foreach ($taskRelative in $TemporaryPaths) {
    $taskAbsolute = [System.IO.Path]::GetFullPath((Join-Path $taskWorkspace $taskRelative))
    if ([System.IO.Path]::GetDirectoryName($taskAbsolute) -ne $taskWorkRoot) { throw "Temporary delivery must be a direct child of workspace .work: $taskRelative" }
    $taskName = [System.IO.Path]::GetFileName($taskAbsolute)
    if ($taskName -notin $taskAllowedNames -and -not @($taskAllowedPrefixes | Where-Object { $taskName.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase) }).Count) { throw "Not a recognized delivery directory: $taskRelative" }
    if (-not (Test-Path -LiteralPath $taskAbsolute)) { continue }
    $taskItem = Get-Item -LiteralPath $taskAbsolute -Force
    if (-not $taskItem.PSIsContainer) { throw "Expected a delivery directory: $taskRelative" }
    $taskPending = [System.Collections.Generic.Stack[System.IO.DirectoryInfo]]::new()
    $taskPending.Push($taskItem)
    $taskBytes = [long]0
    $taskFiles = 0
    while ($taskPending.Count) {
        $taskDirectory = $taskPending.Pop()
        if ($taskDirectory.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw "Refuse to traverse a link: $($taskDirectory.FullName)" }
        foreach ($taskEntry in Get-ChildItem -LiteralPath $taskDirectory.FullName -Force) {
            if ($taskEntry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw "Delivery contains a link: $($taskEntry.FullName)" }
            if ($taskEntry.PSIsContainer) { $taskPending.Push($taskEntry); continue }
            if ($taskEntry.FullName.Substring($taskAbsolute.Length + 1) -match '(^|\\)Data\\') { throw "Delivery contains Data files; preserve user data: $taskRelative" }
            $taskBytes += $taskEntry.Length
            $taskFiles++
        }
    }
    $taskTargets += [pscustomobject]@{ Path = $taskAbsolute; Files = $taskFiles; Bytes = $taskBytes }
}
# Preflight every target before deleting any of them. Never stop a process here.
foreach ($taskProcess in Get-CimInstance Win32_Process -Filter "Name='LLRemote.exe' OR Name='LLRemote.EngineHost.exe' OR Name='sunshine.exe' OR Name='Moonlight.exe'") {
    foreach ($taskTarget in $taskTargets) {
        if ($taskProcess.ExecutablePath -and $taskProcess.ExecutablePath.StartsWith($taskTarget.Path + '\', [System.StringComparison]::OrdinalIgnoreCase)) { throw "Delivery directory is in use: $($taskTarget.Path)" }
    }
}
foreach ($taskTarget in $taskTargets) {
    if ($Execute) { Remove-Item -LiteralPath $taskTarget.Path -Recurse -Force }
    [pscustomobject]@{ Path = $taskTarget.Path; Files = $taskTarget.Files; Bytes = $taskTarget.Bytes; Deleted = [bool]$Execute }
}
