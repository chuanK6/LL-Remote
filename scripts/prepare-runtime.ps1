$ErrorActionPreference = 'Stop'
$taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$taskVsRoot = (& $taskVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
$taskRedistVersion = Get-ChildItem -LiteralPath (Join-Path $taskVsRoot 'VC\Redist\MSVC') -Directory | Where-Object { $_.Name -match '^\d' } | Sort-Object Name -Descending | Select-Object -First 1
$taskCrtRoot = Get-ChildItem -LiteralPath (Join-Path $taskRedistVersion.FullName 'x64') -Directory | Where-Object { $_.Name -match '^Microsoft\.VC.*\.CRT$' } | Select-Object -First 1
if (-not $taskCrtRoot) { throw 'MSVC app-local redistributable libraries were not found.' }
$taskRuntimeRoot = Join-Path $taskWorkspace 'runtimes\VC\x64'
New-Item -ItemType Directory -Path $taskRuntimeRoot -Force | Out-Null
$taskLibraries = @(Get-ChildItem -LiteralPath $taskCrtRoot.FullName -Filter '*.dll' -File)
foreach ($taskLibrary in $taskLibraries) {
    foreach ($taskTargetRoot in @($taskRuntimeRoot,(Join-Path $taskWorkspace 'engines\Moonlight'),(Join-Path $taskWorkspace 'engines\Sunshine'))) {
        Copy-Item -LiteralPath $taskLibrary.FullName -Destination (Join-Path $taskTargetRoot $taskLibrary.Name) -Force
    }
}
$taskConfigPath = Join-Path $taskWorkspace 'src-tauri\tauri.conf.json'
$taskConfig = Get-Content -LiteralPath $taskConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($taskLibrary in $taskLibraries) {
    $taskConfig.bundle.resources | Add-Member -NotePropertyName ('../runtimes/VC/x64/' + $taskLibrary.Name) -NotePropertyValue $taskLibrary.Name -Force
}
$taskUtf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($taskConfigPath, ($taskConfig | ConvertTo-Json -Depth 12) + "`n", $taskUtf8)
$taskQtLicenses = Join-Path $taskWorkspace '.tools\Qt\6.12.0\msvc2022_64\licenses'
if (Test-Path -LiteralPath $taskQtLicenses) {
    $taskNoticeTarget = Join-Path $taskWorkspace 'notices\Qt'
    New-Item -ItemType Directory -Path $taskNoticeTarget -Force | Out-Null
    Copy-Item -Path (Join-Path $taskQtLicenses '*') -Destination $taskNoticeTarget -Recurse -Force
}
$taskQtNotice = Join-Path $taskWorkspace 'notices\Qt'
New-Item -ItemType Directory -Path $taskQtNotice -Force | Out-Null
foreach ($taskLicenseName in @('GPL-3.0-only.txt','LGPL-3.0-only.txt')) {
    Invoke-WebRequest -Uri ('https://raw.githubusercontent.com/qt/qtbase/v6.12.0/LICENSES/' + $taskLicenseName) -OutFile (Join-Path $taskQtNotice $taskLicenseName) -UseBasicParsing -TimeoutSec 30
}
Write-Output "Prepared $($taskLibraries.Count) app-local MSVC runtime libraries."
