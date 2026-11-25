param([switch]$Portable, [string]$OutputRoot)
$ErrorActionPreference = 'Stop'
$taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskWorkspace 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
$taskReleaseRoot = if ($OutputRoot) { [System.IO.Path]::GetFullPath((Join-Path $taskWorkspace $OutputRoot)) } else { Join-Path $taskWorkspace 'release' }
if (-not $taskReleaseRoot.StartsWith($taskWorkspace + '\', [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Package output must stay within the workspace.' }
$taskWorkRoot = Join-Path $taskWorkspace '.work'
if ($taskReleaseRoot -ne (Join-Path $taskWorkspace 'release') -and $taskReleaseRoot -ne (Join-Path $taskWorkRoot 'package-current')) { throw 'Use release for final output or .work/package-current for temporary packaging. Retired task directories must not receive new output.' }
if ((Test-Path -LiteralPath $taskReleaseRoot) -and ((Get-Item -LiteralPath $taskReleaseRoot).Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw 'Package output must be a real directory.' }
$taskStage = Join-Path $taskReleaseRoot ('LLRemote-Portable-x64-' + $taskVersion)
$taskRequired = @('src-tauri\target\release\LLRemote.exe','LLRemote.EngineHost.exe','engines\Moonlight\Moonlight.exe','engines\Sunshine\sunshine.exe','engines\Jiaoyuelian\natpierce.dll')
foreach ($taskRequiredPath in $taskRequired) { if (-not (Test-Path -LiteralPath (Join-Path $taskWorkspace $taskRequiredPath))) { throw "Required complete-package resource missing: $taskRequiredPath" } }
$taskManifest = Get-Content -LiteralPath (Join-Path $taskWorkspace 'engines\manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($taskManifest.moonlight.extensionProtocol -ne 1) { throw 'The portable package requires the built and verified Moonlight extension.' }
if ($taskManifest.sunshine.runtimeManifest -ne 'utf8') { throw 'Prepare the Sunshine UTF-8 process manifest before packaging.' }
$taskReleaseConfig = Get-Content -LiteralPath (Join-Path $taskWorkspace 'src-tauri\tauri.release.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($taskReleaseConfig.bundle.windows.webviewInstallMode.type -ne 'downloadBootstrapper') { throw 'The package must use the shared system WebView2 runtime.' }
if (Test-Path -LiteralPath $taskStage) {
    $taskResolvedStage = (Get-Item -LiteralPath $taskStage).FullName
    if ($taskResolvedStage -ne [System.IO.Path]::GetFullPath($taskStage) -or -not $taskResolvedStage.StartsWith($taskReleaseRoot + '\', [System.StringComparison]::OrdinalIgnoreCase) -or ((Get-Item -LiteralPath $taskStage).Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw 'Unsafe package staging directory.' }
    $taskExistingData = Join-Path $taskStage 'Data'
    if ((Test-Path -LiteralPath $taskExistingData) -and @(Get-ChildItem -LiteralPath $taskExistingData -File -Recurse -Force).Count -gt 0) { throw 'Clean packaging must not include existing user Data. Choose a new output version or preserve the directory before packaging.' }
    foreach ($taskProcess in Get-CimInstance Win32_Process -Filter "Name='LLRemote.exe' OR Name='LLRemote.EngineHost.exe' OR Name='sunshine.exe' OR Name='Moonlight.exe'") {
        if ($taskProcess.ExecutablePath -and $taskProcess.ExecutablePath.StartsWith($taskResolvedStage + '\', [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Exit the application in this package directory before updating its files.' }
    }
}
New-Item -ItemType Directory -Path $taskStage -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $taskWorkspace 'src-tauri\target\release\LLRemote.exe') -Destination (Join-Path $taskStage 'LLRemote.exe') -Force
Copy-Item -LiteralPath (Join-Path $taskWorkspace 'LLRemote.EngineHost.exe') -Destination $taskStage -Force
foreach ($taskDirectory in @('engines','notices','component-tools')) {
    $taskTarget = Join-Path $taskStage $taskDirectory
    New-Item -ItemType Directory -Path $taskTarget -Force | Out-Null
    if ($taskDirectory -eq 'engines') {
        $taskSourceRoot = Join-Path $taskWorkspace 'engines'
        foreach ($taskSourceFile in Get-ChildItem -LiteralPath $taskSourceRoot -File -Recurse -Force) {
            $taskRelative = $taskSourceFile.FullName.Substring($taskSourceRoot.Length + 1)
            if ($taskRelative -match '(^|\\)(config|Data|logs|cache)\\' -or $taskSourceFile.Extension -in @('.pem','.key','.p12','.pfx','.log','.ini') -or $taskSourceFile.Name -in @('config','sunshine_state.json','preferences.json','devices.json')) { continue }
            $taskDestination = Join-Path $taskTarget $taskRelative
            New-Item -ItemType Directory -Path ([System.IO.Path]::GetDirectoryName($taskDestination)) -Force | Out-Null
            Copy-Item -LiteralPath $taskSourceFile.FullName -Destination $taskDestination -Force
        }
    } else { Copy-Item -Path (Join-Path (Join-Path $taskWorkspace $taskDirectory) '*') -Destination $taskTarget -Recurse -Force }
}
$taskPackageDocs = Join-Path $taskStage 'docs'
New-Item -ItemType Directory -Path $taskPackageDocs -Force | Out-Null
foreach ($taskDocName in @('开发与验收记录.md','虚拟网卡创建故障排查.md','连接流程与组网地址.md')) {
    Copy-Item -LiteralPath (Join-Path (Join-Path $taskWorkspace 'docs') $taskDocName) -Destination $taskPackageDocs -Force
}
$taskValidationSource = Join-Path $taskWorkspace ('docs\validation\' + $taskVersion + '.json')
if (Test-Path -LiteralPath $taskValidationSource) { New-Item -ItemType Directory -Path (Join-Path $taskPackageDocs 'validation') -Force | Out-Null; Copy-Item -LiteralPath $taskValidationSource -Destination (Join-Path $taskPackageDocs 'validation') -Force }
# Updating a directory preserves its Data path and existing encrypted credentials.
# Remove only the retired browser runtime; Data/WebView2 is the user's cache.
foreach ($taskLegacyRelativePath in @('runtimes\WebView2','WebView2')) {
    $taskLegacyPath = [System.IO.Path]::GetFullPath((Join-Path $taskStage $taskLegacyRelativePath))
    if (Test-Path -LiteralPath $taskLegacyPath) {
        if (-not $taskLegacyPath.StartsWith([System.IO.Path]::GetFullPath($taskStage) + '\', [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Legacy runtime path is outside the package.' }
        if ((Get-Item -LiteralPath $taskLegacyPath -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw 'Legacy runtime must be a real directory.' }
        Remove-Item -LiteralPath $taskLegacyPath -Recurse -Force
    }
}
Copy-Item -Path (Join-Path $taskWorkspace 'runtimes\VC\x64\*.dll') -Destination $taskStage -Force
New-Item -ItemType Directory -Path (Join-Path $taskStage 'Data') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $taskStage 'portable.flag') -Value 'LLRemote portable data root: ./Data' -Encoding ASCII
Copy-Item -LiteralPath (Join-Path $taskWorkspace 'README.md') -Destination $taskStage -Force
Copy-Item -LiteralPath (Join-Path $taskWorkspace 'CHANGELOG.md') -Destination $taskStage -Force
$taskForbidden = @(Get-ChildItem -LiteralPath (Join-Path $taskStage 'engines') -Recurse -File | Where-Object { $_.Extension -in @('.pem','.key','.p12','.pfx','.log','.ini') -or $_.Name -in @('config','sunshine_state.json','preferences.json','devices.json') })
if ($taskForbidden.Count -gt 0) { throw 'Engine resources unexpectedly contain user data or credentials.' }
$taskUtf8 = [System.Text.UTF8Encoding]::new($false)
$taskPackageManifest = [ordered]@{ version=$taskVersion; target='x86_64-pc-windows-msvc'; portable=$true; components=$taskManifest; windows=[ordered]@{ requestedExecutionLevel='requireAdministrator' }; webview2=[ordered]@{ mode='system-evergreen'; bundled=$false }; delivery='unpacked-directory'; cleanData=$true; verification=('See docs/validation/' + $taskVersion + '.json for completed checks and remaining acceptance.') }
[System.IO.File]::WriteAllText((Join-Path $taskStage 'component-manifest.json'), ($taskPackageManifest | ConvertTo-Json -Depth 12) + "`n", $taskUtf8)
Get-Item -LiteralPath $taskStage | Select-Object FullName
Get-FileHash -LiteralPath (Join-Path $taskStage 'LLRemote.exe') -Algorithm SHA256 | Select-Object Hash,Path
