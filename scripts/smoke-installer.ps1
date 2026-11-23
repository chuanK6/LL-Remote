param([string]$Installer)
$ErrorActionPreference = 'Stop'
$taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Installer) { $Installer = Join-Path $taskWorkspace 'release\LLRemote-Setup-x64.exe' }
$taskInstaller = (Get-Item -LiteralPath $Installer).FullName
$taskWorkRoot = Join-Path $taskWorkspace '.work'
$taskInstallDirectory = [System.IO.Path]::GetFullPath((Join-Path $taskWorkRoot ('安装 路径验证-' + [guid]::NewGuid().ToString())))
if (-not $taskInstallDirectory.StartsWith($taskWorkRoot + '\', [System.StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $taskInstallDirectory)) { throw 'Unsafe installer test directory.' }
$taskUninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\LL远程'
$taskProductKey = 'HKCU:\Software\llremote\LL远程'
if (Test-Path -LiteralPath $taskUninstallKey) { throw 'An existing LL installation must not be replaced by the isolated installer test.' }
$taskLocationBefore = if (Test-Path -LiteralPath $taskProductKey) { (Get-Item -LiteralPath $taskProductKey).GetValue('') } else { $null }
if (@(Get-CimInstance Win32_Process -Filter "Name='LLRemote.exe'").Count -gt 0) { throw 'Exit the running LLRemote through its tray menu before testing the installer.' }
$taskRunPath = 'Software\Microsoft\Windows\CurrentVersion\Run'
$taskRunKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($taskRunPath)
$taskStartupBefore = if ($taskRunKey) { $taskRunKey.GetValue('LLRemote', $null) } else { $null }
if ($taskRunKey) { $taskRunKey.Dispose() }
$taskServiceBefore = (Get-Service -Name SunshineService -ErrorAction SilentlyContinue).Status
$taskEvidence = $null
try {
    $taskInstall = Start-Process -FilePath $taskInstaller -ArgumentList "/S /NS /D=$taskInstallDirectory" -WindowStyle Hidden -PassThru -Wait
    if ($taskInstall.ExitCode -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $taskInstallDirectory 'LLRemote.exe'))) { throw "Installer failed: $($taskInstall.ExitCode)" }
    & node (Join-Path $PSScriptRoot 'smoke-packaged.mjs') (Join-Path $taskInstallDirectory 'LLRemote.exe') --installed
    if ($LASTEXITCODE -ne 0) { throw 'Installed release launch verification failed.' }
    & python (Join-Path $PSScriptRoot 'verify-bundle.py') $taskInstallDirectory --reference $taskWorkspace
    if ($LASTEXITCODE -ne 0) { throw 'Installed resources differ from the complete build.' }
    $taskEvidence = Get-Content -LiteralPath (Join-Path $taskWorkRoot 'installer-evidence.json') -Raw -Encoding UTF8 | ConvertFrom-Json
} finally {
    $taskUninstaller = Join-Path $taskInstallDirectory 'uninstall.exe'
    if (Test-Path -LiteralPath $taskUninstaller) {
        $taskInstalledLocation = (Get-ItemProperty -LiteralPath $taskUninstallKey).InstallLocation.Trim('"')
        if ([System.IO.Path]::GetFullPath($taskInstalledLocation).TrimEnd('\') -ne $taskInstallDirectory) { throw 'The uninstall registry entry does not belong to this isolated test.' }
        $taskUninstall = Start-Process -FilePath $taskUninstaller -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
        $taskDeadline = [DateTime]::UtcNow.AddSeconds(45)
        while (((Test-Path -LiteralPath (Join-Path $taskInstallDirectory 'LLRemote.exe')) -or (Test-Path -LiteralPath $taskUninstallKey)) -and [DateTime]::UtcNow -lt $taskDeadline) { Start-Sleep -Milliseconds 200 }
    }
    if (-not (Test-Path -LiteralPath $taskUninstallKey) -and (Test-Path -LiteralPath $taskProductKey)) {
        $taskProduct = Get-Item -LiteralPath $taskProductKey
        $taskRememberedLocation = [System.IO.Path]::GetFullPath($taskProduct.GetValue('').Trim('"'))
        if ($taskRememberedLocation -ne $taskInstallDirectory) { throw 'Installer location metadata does not belong to this test.' }
        $taskRestoreLocation = if ($null -ne $taskLocationBefore) { $taskLocationBefore } else { Join-Path $env:LOCALAPPDATA 'LL远程' }
        $taskWritableProduct = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\llremote\LL远程', $true)
        try { $taskWritableProduct.SetValue('', $taskRestoreLocation, [Microsoft.Win32.RegistryValueKind]::String) } finally { $taskWritableProduct.Dispose() }
    }
    $taskRunKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($taskRunPath, $true)
    if ($taskRunKey) {
        if ($null -ne $taskStartupBefore -and $null -eq $taskRunKey.GetValue('LLRemote', $null)) { $taskRunKey.SetValue('LLRemote', $taskStartupBefore) }
        $taskStartupAfter = $taskRunKey.GetValue('LLRemote', $null)
        $taskRunKey.Dispose()
    } else { $taskStartupAfter = $null }
    if ($taskStartupAfter -ne $taskStartupBefore) { throw 'The installer test changed the existing LL startup entry.' }
    if ((Get-Service -Name SunshineService -ErrorAction SilentlyContinue).Status -ne $taskServiceBefore) { throw 'The external Sunshine service state changed during the test.' }
}
if ((Test-Path -LiteralPath (Join-Path $taskInstallDirectory 'LLRemote.exe')) -or (Test-Path -LiteralPath $taskUninstallKey)) { throw 'The isolated installation was not fully uninstalled.' }
$taskEvidence | Add-Member -NotePropertyName localInstallAndUninstallPassed -NotePropertyValue $true
$taskEvidence | Add-Member -NotePropertyName existingStartupPreserved -NotePropertyValue $true
$taskEvidence | Add-Member -NotePropertyName installerLocationRecordPreserved -NotePropertyValue $true
$taskEvidence | Add-Member -NotePropertyName externalSunshineServicePreserved -NotePropertyValue $true
[System.IO.File]::WriteAllText((Join-Path $taskWorkRoot 'installer-evidence.json'), ($taskEvidence | ConvertTo-Json -Depth 8) + "`n", [System.Text.UTF8Encoding]::new($false))
$taskEvidence | ConvertTo-Json -Depth 8
