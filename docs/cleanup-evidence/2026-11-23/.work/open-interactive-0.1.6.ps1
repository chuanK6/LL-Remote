$ErrorActionPreference = 'Stop'
$taskWorkspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskDirectory = Join-Path $taskWorkspace 'release\LLRemote-Portable-x64-0.1.6'
$taskExecutable = Join-Path $taskDirectory 'LLRemote.exe'
$taskListener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
$taskListener.Start()
$taskDebugPort = ([Net.IPEndPoint]$taskListener.LocalEndpoint).Port
$taskListener.Stop()
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=' + $taskDebugPort
$taskApplication = Start-Process -FilePath $taskExecutable -WorkingDirectory $taskDirectory -WindowStyle Normal -PassThru
$taskElevated = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$taskInfo = [ordered]@{ProcessId=$taskApplication.Id;DebugPort=$taskDebugPort;Elevated=$taskElevated;Root=$taskDirectory;StartedUtc=[datetime]::UtcNow.ToString('o')}
[IO.File]::WriteAllText((Join-Path $PSScriptRoot 'interactive-desktop.json'),($taskInfo | ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
