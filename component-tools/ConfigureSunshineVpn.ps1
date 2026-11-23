param(
    [Parameter(Mandatory=$true)][string]$Program,
    [string]$LocalAddress,
    [int]$BasePort=47989,
    [Parameter(Mandatory=$true)][string]$StateFile,
    [switch]$Apply,
    [switch]$Restore,
    [ValidateRange(-1,256)][int]$ExpectedBlockCount=-1,
    [ValidateRange(-1,256)][int]$ExpectedOwnedRuleCount=-1
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$taskMarker = 'LLRemote Sunshine VPN'
$taskStatePath = [IO.Path]::GetFullPath($StateFile)
$taskJournalPath = $taskStatePath + '.pending'
$taskStateBefore = $null
$taskState = $null
$taskChangedBlocks = [Collections.Generic.List[object]]::new()
$taskChangedOwned = [Collections.Generic.List[object]]::new()
$taskMutationStarted = $false
$taskMutex = $null
$taskMutexAcquired = $false
$taskPhase = 'validation'
$taskResult = [ordered]@{ok=$false;code='validation';phase='validation';dryRun=(-not $Apply);restore=[bool]$Restore;blockCount=0;ownedRuleCount=0;expectedBlockCount=$ExpectedBlockCount;expectedOwnedRuleCount=$ExpectedOwnedRuleCount;mutationStarted=$false;rollbackAttempted=$false;rollbackComplete=$true;exceptionType=$null;hresult=$null;nativeErrorCode=$null;win32Code=$null;blockingRuleDetected=$false;blockerKind=$null;policySource=$null;blockerDisplayName=$null}

function Get-TaskFailureInfo($Failure,[string]$Phase) {
    # Return fixed classifications and numeric provider diagnostics, never an
    # arbitrary exception message (which can contain an IP or a private path).
    $taskKnownCodes=@('program-invalid','firewall-busy','recovery-required','recovery-failed','state-conflict','owned-rule-conflict','vpn-address-invalid','vpn-address-not-bound','vpn-address-not-unique','vpn-interface-invalid','vpn-interface-not-up','port-invalid','policy-blocked','profile-inbound-blocked','profile-local-rules-disabled','rules-not-effective','block-rule-missing','plan-count-mismatch')
    $taskFailureCode=if ($Failure.Exception.Message -in $taskKnownCodes) { $Failure.Exception.Message } else { 'firewall-operation-failed' }
    $taskException=$Failure.Exception
    for ($taskDepth=0; $taskDepth -lt 4 -and $taskException.InnerException; $taskDepth++) { $taskException=$taskException.InnerException }
    $taskExceptionType=$taskException.GetType().FullName
    $taskHresult=[long]$taskException.HResult
    $taskNativeErrorCode=if ($taskException.PSObject.Properties['NativeErrorCode']) { [int]$taskException.NativeErrorCode } else { $null }
    $taskWin32Code=if ($taskException -is [ComponentModel.Win32Exception]) { [int]$taskException.NativeErrorCode } else { $null }
    if ($taskFailureCode -eq 'firewall-operation-failed' -and ($taskException -is [UnauthorizedAccessException] -or $Failure.CategoryInfo.Category -eq 'PermissionDenied' -or $taskHresult -eq -2147024891 -or $taskWin32Code -eq 5)) { $taskFailureCode='access-denied' }
    return [ordered]@{code=$taskFailureCode;phase=$Phase;exceptionType=$taskExceptionType;hresult=$taskHresult;nativeErrorCode=$taskNativeErrorCode;win32Code=$taskWin32Code}
}

function Assert-TaskExpectedCounts([int]$BlockCount,[int]$OwnedRuleCount) {
    $taskResult.blockCount=$BlockCount
    $taskResult.ownedRuleCount=$OwnedRuleCount
    # These optional constraints are evaluated while holding the program mutex,
    # before recovering an existing journal or starting any new transaction.
    if (($ExpectedBlockCount -ge 0 -and $BlockCount -ne $ExpectedBlockCount) -or ($ExpectedOwnedRuleCount -ge 0 -and $OwnedRuleCount -ne $ExpectedOwnedRuleCount)) { throw 'plan-count-mismatch' }
}

function Write-TaskAtomic([string]$Path,[string]$Text) {
    $taskParent = [IO.Path]::GetDirectoryName($Path)
    [IO.Directory]::CreateDirectory($taskParent) | Out-Null
    $taskTemporary = $Path + '.tmp-' + [guid]::NewGuid().ToString('N')
    try {
        [IO.File]::WriteAllText($taskTemporary,$Text,[Text.UTF8Encoding]::new($false))
        Move-Item -LiteralPath $taskTemporary -Destination $Path -Force
    } finally { if (Test-Path -LiteralPath $taskTemporary) { Remove-Item -LiteralPath $taskTemporary -Force } }
}
function Save-TaskState([string]$Text) { Write-TaskAtomic $taskStatePath $Text }

function Write-TaskJournal {
    $taskJournal=[ordered]@{schema=1;program=$taskProgram;stateBefore=$taskStateBefore;blocks=@($taskChangedBlocks.ToArray());owned=@($taskChangedOwned.ToArray())}
    Write-TaskAtomic $taskJournalPath ($taskJournal | ConvertTo-Json -Depth 12)
}

function Get-TaskRule([string]$Name) {
    # Literal comparisons also work when Windows embeds brackets in a path.
    return Get-NetFirewallRule -PolicyStore PersistentStore -ErrorAction Stop | Where-Object { $_.Name -eq $Name } | Select-Object -First 1
}

function Test-TaskSameSet($Left,$Right) {
    return (@($Left | ForEach-Object { [string]$_ } | Sort-Object) -join '|') -eq (@($Right | ForEach-Object { [string]$_ } | Sort-Object) -join '|')
}

function Get-TaskProfiles($Rule) {
    return @(([string]$Rule.Profile -split ',') | ForEach-Object { $_.Trim() })
}

function Test-TaskScopedBlock($Rule,$Application,$Port,$Address,$Interface) {
    if ($Rule.PolicyStoreSourceType -ne 'Local' -or $Rule.Action -ne 'Block' -or $Rule.Direction -ne 'Inbound' -or
        $Rule.Group -eq $taskMarker -or
        -not [string]::Equals($Application.Program,$taskProgram,[StringComparison]::OrdinalIgnoreCase) -or
        $Port.Protocol -notin @('TCP','UDP','6','17') -or
        ($Application.Package -and $Application.Package -ne 'Any')) { return $false }
    $taskProfiles=@(Get-TaskProfiles $Rule)
    if (-not (Test-TaskSameSet $Port.RemotePort @('Any')) -or
        -not (Test-TaskSameSet $Address.RemoteAddress @('Any')) -or
        -not (Test-TaskSameSet $Interface.InterfaceAlias @('Any')) -or
        @($taskProfiles | Where-Object { $_ -notin @('Any','Domain','Private','Public') }).Count) { return $false }
    if (($Rule | Get-NetFirewallServiceFilter).Service -ne 'Any' -or
        ($Rule | Get-NetFirewallInterfaceTypeFilter).InterfaceType -ne 'Any') { return $false }
    $taskSecurity=$Rule | Get-NetFirewallSecurityFilter
    return $taskSecurity.Authentication -eq 'NotRequired' -and $taskSecurity.Encryption -eq 'NotRequired' -and
        $taskSecurity.LocalUser -eq 'Any' -and $taskSecurity.RemoteUser -eq 'Any' -and
        $taskSecurity.RemoteMachine -eq 'Any' -and [string]$taskSecurity.OverrideBlockRules -eq 'False'
}

function Test-TaskOnlyStreamPorts($Ports,$Required) {
    if (-not @($Ports).Count) { return $false }
    foreach ($taskValue in $Ports) {
        $taskParts=([string]$taskValue) -split '-',2
        $taskFirst=0; $taskLast=0
        if (-not [int]::TryParse($taskParts[0],[ref]$taskFirst)) { return $false }
        if ($taskParts.Count -eq 1) { $taskLast=$taskFirst }
        elseif (-not [int]::TryParse($taskParts[1],[ref]$taskLast)) { return $false }
        if ($taskFirst -lt 1 -or $taskLast -gt 65535 -or $taskLast -lt $taskFirst -or
            $taskLast-$taskFirst+1 -gt @($Required).Count) { return $false }
        foreach ($taskPortNumber in $taskFirst..$taskLast) {
            if ($taskPortNumber -notin $Required) { return $false }
        }
    }
    return $true
}

function Assert-TaskTrackedBlock($Rule,$Block) {
    $taskExpectedProfiles=if ($Block.profiles) { $Block.profiles } else { @('Public') }
    $taskExpectedLocalPorts=if ($Block.localPorts) { $Block.localPorts } else { @('Any') }
    $taskApplication=$Rule | Get-NetFirewallApplicationFilter
    $taskPort=$Rule | Get-NetFirewallPortFilter
    $taskAddress=$Rule | Get-NetFirewallAddressFilter
    $taskInterface=$Rule | Get-NetFirewallInterfaceFilter
    if (-not (Test-TaskScopedBlock $Rule $taskApplication $taskPort $taskAddress $taskInterface) -or
        -not (Test-TaskSameSet $taskPort.LocalPort $taskExpectedLocalPorts) -or
        ($Block.protocol -and [string]$taskPort.Protocol -ne $Block.protocol) -or
        -not (Test-TaskSameSet (Get-TaskProfiles $Rule) $taskExpectedProfiles)) { throw 'state-conflict' }
}

function Get-TaskAddressComplement([Net.IPAddress]$Address) {
    # Windows NetSecurity's CIM provider rejects some zero-base subnet prefixes.
    # Explicit ranges are documented LocalAddress values and preserve the same
    # exact complement, including every IPv6 address, without /0 prefixes.
    $taskBytes = $Address.GetAddressBytes()
    [uint64]$taskNumber=([uint64]$taskBytes[0] -shl 24) -bor ([uint64]$taskBytes[1] -shl 16) -bor ([uint64]$taskBytes[2] -shl 8) -bor [uint64]$taskBytes[3]
    $taskRanges=[Collections.Generic.List[string]]::new()
    if ($taskNumber -gt 0) {
        [uint64]$taskLowEnd=$taskNumber-1
        $taskLowBytes=[byte[]]@((($taskLowEnd -shr 24) -band 255),(($taskLowEnd -shr 16) -band 255),(($taskLowEnd -shr 8) -band 255),($taskLowEnd -band 255))
        $taskRanges.Add('0.0.0.0-'+([Net.IPAddress]::new($taskLowBytes)).ToString())
    }
    if ($taskNumber -lt [uint64]4294967295) {
        [uint64]$taskHighStart=$taskNumber+1
        $taskHighBytes=[byte[]]@((($taskHighStart -shr 24) -band 255),(($taskHighStart -shr 16) -band 255),(($taskHighStart -shr 8) -band 255),($taskHighStart -band 255))
        $taskRanges.Add(([Net.IPAddress]::new($taskHighBytes)).ToString()+'-255.255.255.255')
    }
    $taskRanges.Add('::-ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff')
    return @($taskRanges.ToArray())
}

function Get-TaskOtherPorts($Ports) {
    $taskRanges = [Collections.Generic.List[string]]::new()
    $taskFirst = 1
    foreach ($taskPort in @($Ports | Sort-Object -Unique)) {
        if ($taskFirst -lt $taskPort) { $taskRanges.Add($(if ($taskFirst -eq $taskPort-1) { [string]$taskFirst } else { [string]$taskFirst + '-' + ($taskPort-1) })) }
        $taskFirst = [int]$taskPort+1
    }
    if ($taskFirst -le 65535) { $taskRanges.Add($(if ($taskFirst -eq 65535) { '65535' } else { [string]$taskFirst + '-65535' })) }
    return @($taskRanges.ToArray())
}

function Test-TaskAddressCovered($Values,[Net.IPAddress]$Address) {
    $taskBytes = $Address.GetAddressBytes()
    foreach ($taskValue in @($Values)) {
        if ($taskValue -eq 'Any') { return $true }
        $taskRange=([string]$taskValue) -split '-',2
        if ($taskRange.Count -eq 2) {
            $taskLow=$null; $taskHigh=$null
            if (-not [Net.IPAddress]::TryParse($taskRange[0],[ref]$taskLow) -or -not [Net.IPAddress]::TryParse($taskRange[1],[ref]$taskHigh)) { return $true }
            if ($taskLow.AddressFamily -ne $Address.AddressFamily -or $taskHigh.AddressFamily -ne $Address.AddressFamily) { continue }
            $taskLowBytes=$taskLow.GetAddressBytes(); $taskHighBytes=$taskHigh.GetAddressBytes()
            [uint64]$taskNumber=([uint64]$taskBytes[0] -shl 24) -bor ([uint64]$taskBytes[1] -shl 16) -bor ([uint64]$taskBytes[2] -shl 8) -bor [uint64]$taskBytes[3]
            [uint64]$taskLowNumber=([uint64]$taskLowBytes[0] -shl 24) -bor ([uint64]$taskLowBytes[1] -shl 16) -bor ([uint64]$taskLowBytes[2] -shl 8) -bor [uint64]$taskLowBytes[3]
            [uint64]$taskHighNumber=([uint64]$taskHighBytes[0] -shl 24) -bor ([uint64]$taskHighBytes[1] -shl 16) -bor ([uint64]$taskHighBytes[2] -shl 8) -bor [uint64]$taskHighBytes[3]
            if ($taskNumber -ge $taskLowNumber -and $taskNumber -le $taskHighNumber) { return $true }
            continue
        }
        $taskParts = ([string]$taskValue) -split '/',2
        $taskNetwork = $null
        if (-not [Net.IPAddress]::TryParse($taskParts[0],[ref]$taskNetwork)) { return $true }
        if ($taskNetwork.AddressFamily -ne $Address.AddressFamily) { continue }
        if ($taskParts.Count -eq 1) { if ($taskNetwork.Equals($Address)) { return $true }; continue }
        $taskPrefix = 0
        if (-not [int]::TryParse($taskParts[1],[ref]$taskPrefix) -or $taskPrefix -lt 0 -or $taskPrefix -gt 32) { return $true }
        $taskNetworkBytes = $taskNetwork.GetAddressBytes()
        $taskMatches = $true
        for ($taskBit=0; $taskBit -lt $taskPrefix; $taskBit++) {
            $taskByte = [int][Math]::Floor($taskBit/8)
            $taskMask = 1 -shl (7-($taskBit%8))
            if (($taskBytes[$taskByte] -band $taskMask) -ne ($taskNetworkBytes[$taskByte] -band $taskMask)) { $taskMatches=$false; break }
        }
        if ($taskMatches) { return $true }
    }
    return $false
}

function Test-TaskPortsCovered($Values,$Required) {
    foreach ($taskValue in @($Values)) {
        if ($taskValue -eq 'Any') { return $true }
        $taskParts = ([string]$taskValue) -split '-',2
        $taskLow=0; $taskHigh=0
        if (-not [int]::TryParse($taskParts[0],[ref]$taskLow)) { return $true }
        $taskHigh=$taskLow
        if ($taskParts.Count -gt 1 -and -not [int]::TryParse($taskParts[1],[ref]$taskHigh)) { return $true }
        foreach ($taskRequired in $Required) { if ($taskRequired -ge $taskLow -and $taskRequired -le $taskHigh) { return $true } }
    }
    return $false
}

function Get-TaskOwnedSnapshot([string]$Name) {
    if ($Name -notin @(($taskPrefix+'-TCP'),($taskPrefix+'-UDP'),($taskPrefix+'-TCP-Other'),($taskPrefix+'-UDP-Other'))) { throw 'owned-rule-conflict' }
    $taskRule = Get-TaskRule $Name
    if (-not $taskRule) { return [ordered]@{name=$Name;exists=$false} }
    $taskApplication = $taskRule | Get-NetFirewallApplicationFilter
    if ($taskRule.Group -ne $taskMarker -or -not [string]::Equals($taskApplication.Program,$taskProgram,[StringComparison]::OrdinalIgnoreCase)) { throw 'owned-rule-conflict' }
    $taskPort = $taskRule | Get-NetFirewallPortFilter
    $taskAddress = $taskRule | Get-NetFirewallAddressFilter
    $taskInterface = $taskRule | Get-NetFirewallInterfaceFilter
    $taskInterfaceType = $taskRule | Get-NetFirewallInterfaceTypeFilter
    return [ordered]@{name=$Name;exists=$true;displayName=$taskRule.DisplayName;description=$taskRule.Description;enabled=$taskRule.Enabled.ToString();direction=$taskRule.Direction.ToString();action=$taskRule.Action.ToString();profile=$taskRule.Profile.ToString();program=$taskApplication.Program;protocol=$taskPort.Protocol;localPort=@($taskPort.LocalPort);remotePort=@($taskPort.RemotePort);localAddress=@($taskAddress.LocalAddress);remoteAddress=@($taskAddress.RemoteAddress);interfaceAlias=@($taskInterface.InterfaceAlias);interfaceType=$taskInterfaceType.InterfaceType;edgeTraversalPolicy=$taskRule.EdgeTraversalPolicy.ToString()}
}

function Set-TaskOwnedRule($Specification) {
    $taskRule = Get-TaskRule $Specification.name
    $taskParameters = @{Enabled='True';Direction='Inbound';Action=$Specification.action;Profile=($Specification.profile -join ', ');Program=$taskProgram;Protocol=$Specification.protocol;LocalPort=$Specification.ports;RemotePort='Any';LocalAddress=$taskIp.ToString();RemoteAddress='Any';InterfaceAlias=$taskAlias;InterfaceType='Any';EdgeTraversalPolicy='Block'}
    if ($taskRule) {
        Set-NetFirewallRule -InputObject $taskRule @taskParameters | Out-Null
    } else {
        New-NetFirewallRule -PolicyStore PersistentStore -Name $Specification.name -DisplayName $Specification.name -Group $taskMarker -Description $taskMarker @taskParameters | Out-Null
    }
}

function Restore-TaskOwnedSnapshot($Snapshot) {
    # Validate an existing rule before removing/restoring it, including rollback.
    [void](Get-TaskOwnedSnapshot $Snapshot.name)
    $taskRule = Get-TaskRule $Snapshot.name
    if (-not $Snapshot.exists) { if ($taskRule) { Remove-NetFirewallRule -InputObject $taskRule | Out-Null }; return }
    $taskParameters=@{Enabled=$Snapshot.enabled;Direction=$Snapshot.direction;Action=$Snapshot.action;Profile=$Snapshot.profile;Program=$Snapshot.program;Protocol=$Snapshot.protocol;LocalPort=$Snapshot.localPort;RemotePort=$Snapshot.remotePort;LocalAddress=$Snapshot.localAddress;RemoteAddress=$Snapshot.remoteAddress;InterfaceAlias=$Snapshot.interfaceAlias;InterfaceType=$Snapshot.interfaceType;EdgeTraversalPolicy=$Snapshot.edgeTraversalPolicy;Description=$Snapshot.description}
    if ($taskRule) { Set-NetFirewallRule -InputObject $taskRule @taskParameters | Out-Null }
    else { New-NetFirewallRule -PolicyStore PersistentStore -Name $Snapshot.name -DisplayName $Snapshot.displayName -Group $taskMarker @taskParameters | Out-Null }
}

function Undo-TaskTransaction($Blocks,$Owned,[string]$PreviousState) {
    $taskComplete=$true
    foreach ($taskBlock in @($Blocks)) {
        try {
            $taskRule=Get-TaskRule $taskBlock.name
            if ($taskRule) {
                Assert-TaskTrackedBlock $taskRule $taskBlock
                Set-NetFirewallRule -InputObject $taskRule -LocalAddress $taskBlock.before | Out-Null
            }
        } catch { $taskComplete=$false }
    }
    foreach ($taskSnapshot in @($Owned)) { try { Restore-TaskOwnedSnapshot $taskSnapshot } catch { $taskComplete=$false } }
    try { if ($PreviousState) { Save-TaskState $PreviousState } elseif (Test-Path -LiteralPath $taskStatePath) { Remove-Item -LiteralPath $taskStatePath -Force } } catch { $taskComplete=$false }
    if ($taskComplete -and (Test-Path -LiteralPath $taskJournalPath)) { try { Remove-Item -LiteralPath $taskJournalPath -Force } catch { $taskComplete=$false } }
    return $taskComplete
}

try {
    $taskPhase='program-validation'
    $taskProgramCandidate=[IO.Path]::GetFullPath($Program)
    # Tauri can pass an extended drive path, which NetSecurity rejects as a program filter.
    if ($taskProgramCandidate -match '^\\\\\?\\[A-Za-z]:\\') { $taskProgramCandidate=$taskProgramCandidate.Substring(4) }
    if ([IO.Path]::GetFileName($taskProgramCandidate) -ine 'sunshine.exe') { throw 'program-invalid' }
    if (Test-Path -LiteralPath $taskProgramCandidate) {
        $taskProgramItem = Get-Item -LiteralPath $taskProgramCandidate -ErrorAction Stop
        if ($taskProgramItem.PSIsContainer -or ($taskProgramItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'program-invalid' }
        $taskProgram=$taskProgramItem.FullName
    } elseif ($Restore) {
        # Restoring owned rules is still necessary if a package was removed.
        # Program/state identity and every rule association are checked below.
        $taskProgram=$taskProgramCandidate
    } else { throw 'program-invalid' }
    $taskHash=[Security.Cryptography.SHA256]::Create()
    try { $taskIdentifier=([BitConverter]::ToString($taskHash.ComputeHash([Text.Encoding]::UTF8.GetBytes($taskProgram.ToLowerInvariant()))).Replace('-','').Substring(0,16)) }
    finally { $taskHash.Dispose() }
    $taskPrefix='LLRemote-SunshineVpn-'+$taskIdentifier
    $taskPhase='lock-acquisition'
    $taskMutex=[Threading.Mutex]::new($false,'Local\LLRemoteSunshineFirewall-'+$taskIdentifier)
    try { $taskMutexAcquired=$taskMutex.WaitOne([TimeSpan]::FromSeconds(40)) } catch [Threading.AbandonedMutexException] { $taskMutexAcquired=$true }
    if (-not $taskMutexAcquired) { throw 'firewall-busy' }
    if (Test-Path -LiteralPath $taskJournalPath) {
        $taskPhase='journal-recovery'
        if (-not $Apply) { throw 'recovery-required' }
        $taskPending=[IO.File]::ReadAllText($taskJournalPath,[Text.Encoding]::UTF8) | ConvertFrom-Json
        if ($taskPending.schema -ne 1 -or -not [string]::Equals($taskPending.program,$taskProgram,[StringComparison]::OrdinalIgnoreCase)) { throw 'state-conflict' }
        $taskPhase='recovery-plan-validation'
        Assert-TaskExpectedCounts -BlockCount (@($taskPending.blocks).Count) -OwnedRuleCount (@($taskPending.owned).Count)
        $taskPhase='journal-recovery'
        if (-not (Undo-TaskTransaction $taskPending.blocks $taskPending.owned $taskPending.stateBefore)) { $taskResult.rollbackComplete=$false; throw 'recovery-failed' }
    }
    $taskPhase='state-validation'
    $taskStateBefore=if (Test-Path -LiteralPath $taskStatePath) { [IO.File]::ReadAllText($taskStatePath,[Text.Encoding]::UTF8) } else { $null }
    $taskState=if ($taskStateBefore) { $taskStateBefore | ConvertFrom-Json } else { $null }
    if ($taskState -and ($taskState.schema -ne 1 -or -not [string]::Equals($taskState.program,$taskProgram,[StringComparison]::OrdinalIgnoreCase))) { throw 'state-conflict' }
    if ($Restore) {
        if (-not $taskState) { $taskPhase='restore-plan-validation'; Assert-TaskExpectedCounts 0 0; $taskResult.ok=$true; $taskResult.code='unchanged' }
        else {
            $taskPhase='restore-plan-validation'
            Assert-TaskExpectedCounts -BlockCount (@($taskState.blocks).Count) -OwnedRuleCount (@($taskState.ownedRules).Count)
            $taskPhase='restore-plan'
            foreach ($taskBlock in @($taskState.blocks)) {
                $taskRule = Get-TaskRule $taskBlock.name
                if (-not $taskRule) { continue }
                Assert-TaskTrackedBlock $taskRule $taskBlock
                if (-not (Test-TaskSameSet $taskBlock.originalAddress @('Any'))) { throw 'state-conflict' }
                $taskCurrent = $taskRule | Get-NetFirewallAddressFilter
                if (-not (Test-TaskSameSet $taskCurrent.LocalAddress $taskBlock.appliedAddress) -and -not (Test-TaskSameSet $taskCurrent.LocalAddress $taskBlock.originalAddress)) { throw 'state-conflict' }
                $taskBeforeAddresses=if (Test-TaskSameSet $taskCurrent.LocalAddress @('Any')) { @('Any') } else { Get-TaskAddressComplement ([Net.IPAddress]::Parse($taskState.localAddress)) }
                $taskChangedBlocks.Add([ordered]@{name=$taskBlock.name;profiles=@(Get-TaskProfiles $taskRule);localPorts=$taskBlock.localPorts;protocol=$taskBlock.protocol;before=@($taskBeforeAddresses)})
            }
            foreach ($taskName in @($taskState.ownedRules)) { $taskChangedOwned.Add((Get-TaskOwnedSnapshot $taskName)) }
            $taskResult.blockCount=$taskChangedBlocks.Count
            $taskResult.ownedRuleCount=$taskChangedOwned.Count
            if ($Apply) {
                $taskPhase='restore-journal'
                Write-TaskJournal
                $taskMutationStarted=$true
                $taskPhase='restore-blocks'
                foreach ($taskBlock in @($taskState.blocks)) {
                    $taskRule=Get-TaskRule $taskBlock.name
                    if ($taskRule) {
                        Assert-TaskTrackedBlock $taskRule $taskBlock
                        Set-NetFirewallRule -InputObject $taskRule -LocalAddress $taskBlock.originalAddress | Out-Null
                    }
                }
                $taskPhase='restore-rules'
                foreach ($taskSnapshot in $taskChangedOwned) { $taskRule=Get-TaskRule $taskSnapshot.name; if ($taskRule) { Remove-NetFirewallRule -InputObject $taskRule | Out-Null } }
                $taskPhase='restore-state'
                Remove-Item -LiteralPath $taskStatePath -Force
                Remove-Item -LiteralPath $taskJournalPath -Force
            }
            $taskResult.ok=$true; $taskResult.code='restored'
        }
    } else {
        $taskPhase='address-validation'
        $taskIp = $null
        if (-not [Net.IPAddress]::TryParse($LocalAddress,[ref]$taskIp) -or $taskIp.AddressFamily -ne [Net.Sockets.AddressFamily]::InterNetwork -or [Net.IPAddress]::IsLoopback($taskIp) -or $taskIp.Equals([Net.IPAddress]::Any)) { throw 'vpn-address-invalid' }
        if ($BasePort -lt 6 -or $BasePort -gt 65514) { throw 'port-invalid' }
        $taskPhase='interface-address-query'
        # -IPAddress throws a provider exception when an asynchronously created
        # VPN address is not bound yet. An empty result is a readiness condition,
        # not evidence that Windows policy rejected a firewall operation.
        $taskInterfaces = @(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction Stop | Where-Object { $_.IPAddress -eq $taskIp.ToString() })
        if ($taskInterfaces.Count -eq 0) { throw 'vpn-address-not-bound' }
        if ($taskInterfaces.Count -ne 1) { throw 'vpn-address-not-unique' }
        $taskInterface = $taskInterfaces[0]
        $taskPhase='interface-adapter-query'
        $taskAdapter = @(Get-NetAdapter -IncludeHidden -ErrorAction Stop | Where-Object { $_.ifIndex -eq $taskInterface.InterfaceIndex })
        if ($taskAdapter.Count -ne 1 -or ($taskAdapter[0].PnPDeviceID -notlike 'SWD\WINTUN\*' -and $taskAdapter[0].DriverFileName -notmatch 'wintun\.sys$')) { throw 'vpn-interface-invalid' }
        if ($taskAdapter[0].Status -ne 'Up') { throw 'vpn-interface-not-up' }
        $taskAlias=$taskAdapter[0].Name
        $taskPhase='interface-profile-query'
        $taskNetworkProfiles=@(Get-NetConnectionProfile -InterfaceIndex $taskInterface.InterfaceIndex -ErrorAction SilentlyContinue | ForEach-Object { if ($_.NetworkCategory -eq 'DomainAuthenticated') { 'Domain' } else { $_.NetworkCategory.ToString() } })
        if (-not $taskNetworkProfiles.Count) { $taskNetworkProfiles=@('Public') }
        $taskTcp=@(($BasePort-5),$BasePort,($BasePort+21))
        $taskUdp=@(($BasePort+9),($BasePort+10),($BasePort+11),($BasePort+13))
        $taskPhase='profile-policy-query'
        foreach ($taskProfile in (Get-NetFirewallProfile -PolicyStore ActiveStore -ErrorAction Stop)) {
            if ($taskProfile.Name -notin $taskNetworkProfiles -or [string]$taskProfile.Enabled -eq 'False' -or
                $taskAlias -in @($taskProfile.DisabledInterfaceAliases)) { continue }
            if ([string]$taskProfile.AllowInboundRules -eq 'False') { throw 'profile-inbound-blocked' }
            if ([string]$taskProfile.AllowLocalFirewallRules -eq 'False') { throw 'profile-local-rules-disabled' }
        }
        $taskComplement=Get-TaskAddressComplement $taskIp
        $taskSpecifications = [Collections.Generic.List[object]]::new()
        $taskSpecifications.Add([ordered]@{name=$taskPrefix+'-TCP';action='Allow';profile='Any';protocol='TCP';ports=$taskTcp})
        $taskSpecifications.Add([ordered]@{name=$taskPrefix+'-UDP';action='Allow';profile='Any';protocol='UDP';ports=$taskUdp})
        $taskBlocks = [Collections.Generic.List[object]]::new()
        $taskPhase='firewall-rule-query'
        # A filtered NetSecurity query reports no matches as a CIM error.
        $taskActiveBlocks = @(Get-NetFirewallRule -PolicyStore ActiveStore -ErrorAction Stop | Where-Object {
            $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' -and $_.Action -eq 'Block'
        })
        foreach ($taskRule in $taskActiveBlocks) {
            $taskPhase='blocker-inspection'
            if ($taskRule.Group -eq $taskMarker -and $taskRule.Name.StartsWith($taskPrefix,[StringComparison]::OrdinalIgnoreCase)) { continue }
            $taskApplication = $taskRule | Get-NetFirewallApplicationFilter
            if ($taskApplication.Program -ne 'Any' -and -not [string]::Equals($taskApplication.Program,$taskProgram,[StringComparison]::OrdinalIgnoreCase)) { continue }
            $taskSaved=@($taskState.blocks | Where-Object { $_.name -eq $taskRule.Name }) | Select-Object -First 1
            $taskProfiles=@(Get-TaskProfiles $taskRule)
            # Keep a previously tracked exception reversible if the adapter profile changes.
            if (-not $taskSaved -and 'Any' -notin $taskProfiles -and -not @($taskProfiles | Where-Object { $_ -in $taskNetworkProfiles }).Count) { continue }
            $taskPort=$taskRule | Get-NetFirewallPortFilter
            $taskRequired = if ($taskPort.Protocol -in @('TCP','6')) { $taskTcp } elseif ($taskPort.Protocol -in @('UDP','17')) { $taskUdp } elseif ($taskPort.Protocol -in @('Any','256')) { @($taskTcp)+@($taskUdp) } else { @() }
            if (-not $taskRequired.Count -or -not (Test-TaskPortsCovered $taskPort.LocalPort $taskRequired)) { continue }
            $taskAddress=$taskRule | Get-NetFirewallAddressFilter
            if (-not $taskSaved -and -not (Test-TaskAddressCovered $taskAddress.LocalAddress $taskIp)) { continue }
            $taskRuleInterface=$taskRule | Get-NetFirewallInterfaceFilter
            if ('Any' -notin @($taskRuleInterface.InterfaceAlias) -and -not @($taskRuleInterface.InterfaceAlias | Where-Object { $taskAlias -like $_ }).Count) { continue }
            $taskAllPorts=Test-TaskSameSet $taskPort.LocalPort @('Any')
            if (-not (Test-TaskScopedBlock $taskRule $taskApplication $taskPort $taskAddress $taskRuleInterface) -or
                (-not $taskAllPorts -and -not (Test-TaskOnlyStreamPorts $taskPort.LocalPort $taskRequired))) {
                $taskResult.blockingRuleDetected=$true
                $taskResult.policySource=if ([string]$taskRule.PolicyStoreSourceType -in @('Local','GroupPolicy','Dynamic','Hardcoded','Generated')) { [string]$taskRule.PolicyStoreSourceType } else { 'Unknown' }
                $taskResult.blockerKind=if ($taskResult.policySource -eq 'GroupPolicy') { 'group-policy' } elseif ($taskResult.policySource -eq 'Local') { 'custom-rule' } else { 'nonlocal-rule' }
                $taskResult.blockerDisplayName=$taskRule.DisplayName
                throw 'policy-blocked'
            }
            if ($taskSaved) {
                Assert-TaskTrackedBlock $taskRule $taskSaved
                if (-not (Test-TaskSameSet $taskSaved.originalAddress @('Any')) -or (-not (Test-TaskSameSet $taskAddress.LocalAddress $taskSaved.appliedAddress) -and -not (Test-TaskSameSet $taskAddress.LocalAddress @('Any')))) { throw 'state-conflict' }
            } elseif (-not (Test-TaskSameSet $taskAddress.LocalAddress @('Any'))) {
                $taskResult.blockingRuleDetected=$true
                $taskResult.policySource='Local'
                $taskResult.blockerKind='modified-automatic-rule'
                $taskResult.blockerDisplayName=$taskRule.DisplayName
                throw 'policy-blocked'
            }
            $taskBlocks.Add([ordered]@{name=$taskRule.Name;profiles=$taskProfiles;localPorts=@($taskPort.LocalPort);protocol=[string]$taskPort.Protocol;originalAddress=@('Any');appliedAddress=$taskComplement})
            $taskBeforeAddresses=if (Test-TaskSameSet $taskAddress.LocalAddress @('Any')) { @('Any') } else { Get-TaskAddressComplement ([Net.IPAddress]::Parse($taskState.localAddress)) }
            $taskChangedBlocks.Add([ordered]@{name=$taskRule.Name;profiles=$taskProfiles;localPorts=@($taskPort.LocalPort);protocol=[string]$taskPort.Protocol;before=@($taskBeforeAddresses)})
            if (-not $taskAllPorts) { continue }
            $taskProtocol=if ($taskPort.Protocol -in @('TCP','6')) { 'TCP' } else { 'UDP' }
            $taskGuardName=$taskPrefix+'-'+$taskProtocol+'-Other'
            $taskGuard=$taskSpecifications | Where-Object { $_.name -eq $taskGuardName } | Select-Object -First 1
            if ($taskGuard) {
                $taskGuardProfiles=@(@($taskGuard.profile) + $taskProfiles | Sort-Object -Unique)
                $taskGuard.profile=if ('Any' -in $taskGuardProfiles) { 'Any' } else { $taskGuardProfiles }
            } else {
                $taskAllowedPorts=if ($taskProtocol -eq 'TCP') { $taskTcp } else { $taskUdp }
                $taskSpecifications.Insert(0,[ordered]@{name=$taskGuardName;action='Block';profile=$taskProfiles;protocol=$taskProtocol;ports=(Get-TaskOtherPorts $taskAllowedPorts)})
            }
        }
        # Include original tracked blocks even if a prior apply already exempted
        # this same IP and ActiveStore's matching logic therefore skipped it.
        foreach ($taskBlock in @($taskState.blocks)) {
            if (-not @($taskBlocks | Where-Object { $_.name -eq $taskBlock.name }).Count -and (Get-TaskRule $taskBlock.name)) { throw 'state-conflict' }
        }
        $taskPhase='plan-validation'
        Assert-TaskExpectedCounts -BlockCount $taskBlocks.Count -OwnedRuleCount $taskSpecifications.Count
        $taskNextState=[ordered]@{schema=1;program=$taskProgram;localAddress=$taskIp.ToString();interfaceAlias=$taskAlias;blocks=@($taskBlocks.ToArray());ownedRules=@($taskSpecifications | ForEach-Object { $_.name })}
        $taskPhase='owned-rule-snapshot'
        foreach ($taskSpecification in $taskSpecifications) { $taskChangedOwned.Add((Get-TaskOwnedSnapshot $taskSpecification.name)) }
        $taskResult.blockCount=$taskBlocks.Count
        $taskResult.ownedRuleCount=$taskSpecifications.Count
        $taskResult.tcpPorts=$taskTcp
        $taskResult.udpPorts=$taskUdp
        $taskResult.adminWebAllowed=$false
        if ($Apply) {
            $taskPhase='apply-journal'
            Write-TaskJournal
            $taskMutationStarted=$true
            foreach ($taskSpecification in $taskSpecifications) {
                $taskPhase='apply-'+$taskSpecification.protocol.ToLowerInvariant()+'-'+$taskSpecification.action.ToLowerInvariant()
                Set-TaskOwnedRule $taskSpecification
            }
            $taskPhase='apply-blocks'
            foreach ($taskBlock in $taskBlocks) {
                $taskRule=Get-TaskRule $taskBlock.name
                if (-not $taskRule) { throw 'block-rule-missing' }
                Assert-TaskTrackedBlock $taskRule $taskBlock
                Set-NetFirewallRule -InputObject $taskRule -LocalAddress $taskBlock.appliedAddress | Out-Null
                # Read back the provider's canonical representation so subsequent
                # apply/restore comparisons do not treat normalization as edits.
                $taskEffective=Get-TaskRule $taskBlock.name | Get-NetFirewallAddressFilter
                $taskBlock.appliedAddress=@($taskEffective.LocalAddress)
            }
            $taskPhase='verify-active-rules'
            $taskActiveRules=@(Get-NetFirewallRule -PolicyStore ActiveStore -ErrorAction Stop |
                Where-Object { $_.Name -in $taskNextState.ownedRules })
            foreach ($taskSpecification in $taskSpecifications) {
                $taskEffective=@($taskActiveRules | Where-Object { $_.Name -eq $taskSpecification.name })
                if ($taskEffective.Count -ne 1) { throw 'rules-not-effective' }
                $taskRule=$taskEffective[0]
                if ($taskRule.Group -ne $taskMarker -or $taskRule.Direction -ne 'Inbound' -or
                    $taskRule.Enabled -ne 'True' -or $taskRule.Action -ne $taskSpecification.action) { throw 'rules-not-effective' }
                $taskAppFilter=$taskRule | Get-NetFirewallApplicationFilter
                $taskPortFilter=$taskRule | Get-NetFirewallPortFilter
                $taskAddressFilter=$taskRule | Get-NetFirewallAddressFilter
                $taskInterfaceFilter=$taskRule | Get-NetFirewallInterfaceFilter
                $taskProtocolNumber=if ($taskSpecification.protocol -eq 'TCP') { '6' } else { '17' }
                if (-not [string]::Equals($taskAppFilter.Program,$taskProgram,[StringComparison]::OrdinalIgnoreCase) -or
                    $taskPortFilter.Protocol -notin @($taskSpecification.protocol,$taskProtocolNumber) -or
                    -not (Test-TaskSameSet $taskPortFilter.LocalPort $taskSpecification.ports) -or
                    -not (Test-TaskSameSet $taskAddressFilter.LocalAddress @($taskIp.ToString())) -or
                    -not (Test-TaskSameSet $taskInterfaceFilter.InterfaceAlias @($taskAlias)) -or
                    -not (Test-TaskSameSet (Get-TaskProfiles $taskRule) $taskSpecification.profile)) { throw 'rules-not-effective' }
            }
            $taskPhase='persist-state'
            Save-TaskState ($taskNextState | ConvertTo-Json -Depth 10)
            Remove-Item -LiteralPath $taskJournalPath -Force
        }
        $taskResult.ok=$true; $taskResult.code='prepared'
    }
} catch {
    $taskFailureInfo=Get-TaskFailureInfo $_ $taskPhase
    foreach ($taskFailureKey in $taskFailureInfo.Keys) { $taskResult[$taskFailureKey]=$taskFailureInfo[$taskFailureKey] }
    if ($Apply -and $taskMutationStarted) {
        $taskResult.rollbackAttempted=$true
        $taskResult.rollbackComplete=Undo-TaskTransaction $taskChangedBlocks.ToArray() $taskChangedOwned.ToArray() $taskStateBefore
    }
} finally {
    if ($taskMutexAcquired) { $taskMutex.ReleaseMutex() }
    if ($taskMutex) { $taskMutex.Dispose() }
}
$taskResult.mutationStarted=$taskMutationStarted
if ($taskResult.ok) { $taskResult.phase='complete' }
$taskResult.stateFileRemaining=Test-Path -LiteralPath $taskStatePath
$taskResult.journalFileRemaining=Test-Path -LiteralPath $taskJournalPath
Write-Output ($taskResult | ConvertTo-Json -Depth 6 -Compress)
if (-not $taskResult.ok) { exit 1 }
