param([string]$NativeRoot = 'E:\kongping')
$ErrorActionPreference = 'Stop'
$taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskNativeRoot = (Resolve-Path -LiteralPath $NativeRoot).Path
$taskEnginesRoot = [System.IO.Path]::GetFullPath((Join-Path $taskWorkspace 'engines'))
if (-not $taskEnginesRoot.StartsWith($taskWorkspace + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Engine target must stay within the workspace.' }
$taskComponents = @(
    @{ Source = 'Moonlight'; Target = 'Moonlight'; Executable = 'Moonlight.exe' },
    @{ Source = 'Sunshine'; Target = 'Sunshine'; Executable = 'sunshine.exe' },
    @{ Source = 'natpierce-win-v1.06'; Target = 'Jiaoyuelian'; Executable = 'natpierce.exe' }
)
foreach ($taskComponent in $taskComponents) {
    $taskSourceRoot = (Resolve-Path -LiteralPath (Join-Path $taskNativeRoot $taskComponent.Source)).Path
    $taskTargetRoot = Join-Path $taskEnginesRoot $taskComponent.Target
    New-Item -ItemType Directory -Path $taskTargetRoot -Force | Out-Null
    $taskFiles = @(Get-ChildItem -LiteralPath $taskSourceRoot -Recurse -File | Where-Object {
        $taskRelative = $_.FullName.Substring($taskSourceRoot.Length + 1)
        $taskFirstPart = ($taskRelative -split '[\\/]')[0]
        $taskFirstPart -notin @('config', 'Data', 'logs', 'credentials', '.git') -and $_.Extension -notin @('.log', '.ini', '.pem', '.key', '.p12', '.pfx') -and $_.Name -notmatch '^sunshine\.log|^Moonlight-.*\.log'
    })
    foreach ($taskFile in $taskFiles) {
        if (($taskFile.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Linked resources must be reviewed before packaging.' }
        $taskRelative = $taskFile.FullName.Substring($taskSourceRoot.Length + 1)
        $taskTarget = Join-Path $taskTargetRoot $taskRelative
        New-Item -ItemType Directory -Path (Split-Path -Parent $taskTarget) -Force | Out-Null
        Copy-Item -LiteralPath $taskFile.FullName -Destination $taskTarget -Force
    }
    Write-Output "$($taskComponent.Target): copied $($taskFiles.Count) runtime files; user configuration excluded."
}
$taskLicense = Join-Path $taskWorkspace 'third-party\sources\moonlight\LICENSE'
if (Test-Path -LiteralPath $taskLicense) { Copy-Item -LiteralPath $taskLicense -Destination (Join-Path $taskWorkspace 'notices\Moonlight-GPLv3.txt') -Force }
$taskManifestPath = Join-Path $taskEnginesRoot 'manifest.json'
$taskManifest = Get-Content -LiteralPath $taskManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($taskName in @('moonlight', 'sunshine', 'jiaoyuelian')) {
    $taskExecutable = Join-Path $taskEnginesRoot $taskManifest.$taskName.executable
    $taskManifest.$taskName | Add-Member -NotePropertyName sha256 -NotePropertyValue (Get-FileHash -LiteralPath $taskExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -Force
}
$taskUtf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($taskManifestPath, ($taskManifest | ConvertTo-Json -Depth 8) + "`n", $taskUtf8)
