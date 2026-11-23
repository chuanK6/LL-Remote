$ErrorActionPreference = 'Stop'
$taskWorkspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskNode = 'D:\JSP WEB\Node\node.exe'
$taskScript = Join-Path $PSScriptRoot 'sdk-accept-control-smoke.mjs'
Set-Location -LiteralPath $taskWorkspace
$taskStatusPath = Join-Path $PSScriptRoot 'sdk-runner-status.json'
$taskStatus = [ordered]@{StartedUtc=[datetime]::UtcNow.ToString('o');Node=$taskNode;Phase='starting';ExitCode=$null;Error=$null}
function Write-TaskStatus { [IO.File]::WriteAllText($taskStatusPath, ($taskStatus | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false)) }
Write-TaskStatus
try {
    $taskArguments = '"' + $taskScript + '"'
    $taskProcess = Start-Process -FilePath $taskNode -ArgumentList $taskArguments -WorkingDirectory $taskWorkspace -WindowStyle Hidden -RedirectStandardOutput (Join-Path $PSScriptRoot 'sdk-accept-control.stdout.txt') -RedirectStandardError (Join-Path $PSScriptRoot 'sdk-accept-control.stderr.txt') -PassThru
    $taskStatus.Phase = 'running'
    Write-TaskStatus
    $taskProcess.WaitForExit()
    $taskStatus.ExitCode = $taskProcess.ExitCode
    $taskStatus.Phase = 'finished'
} catch { $taskStatus.Error=$_.Exception.Message; $taskStatus.Phase='failed' }
Write-TaskStatus
if ($taskStatus.Error) { exit 1 }
exit $taskStatus.ExitCode
