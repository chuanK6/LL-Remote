param(
    [Parameter(Mandatory=$true)][ValidateSet('list','apply','remove')][string]$Action,
    [string]$Program,
    [string]$LocalAddress,
    [ValidateRange(6,65514)][int]$BasePort=47989,
    [ValidateSet('lan','internet')][string]$Access='lan'
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$directGroup = 'LLRemote Sunshine Direct'
$directNames = @('LLRemote-Sunshine-Direct-TCP','LLRemote-Sunshine-Direct-UDP')
$phase = 'interfaces'

function Get-DirectAddresses {
    foreach ($adapter in Get-NetAdapter | Where-Object { $_.Status -eq 'Up' -and $_.InterfaceDescription -notmatch 'Wintun' }) {
        foreach ($entry in Get-NetIPAddress -InterfaceIndex $adapter.ifIndex -AddressState Preferred -ErrorAction SilentlyContinue) {
            $ip = [Net.IPAddress]::Parse($entry.IPAddress)
            if ([Net.IPAddress]::IsLoopback($ip) -or $ip.IsIPv6LinkLocal -or $ip.IsIPv6Multicast) { continue }
            [pscustomobject]@{ address=$ip.ToString(); interface=$adapter.Name; index=$adapter.ifIndex }
        }
    }
}

function Get-OwnedDirectRules {
    foreach ($rule in Get-NetFirewallRule -PolicyStore PersistentStore | Where-Object { $_.Name -in $directNames }) {
        if ($rule.Group -ne $directGroup) { throw 'rule-conflict' }
        $rule
    }
}

function Remove-OwnedDirectRules($Rules) {
    foreach ($rule in $Rules) {
        Remove-NetFirewallRule -InputObject $rule | Out-Null
    }
}

function Test-DirectPortOverlap($Filter, $Ports) {
    foreach ($value in @($Filter)) {
        if ($value -eq 'Any' -or $value -in $Ports) { return $true }
        if ($value -match '^([0-9]+)-([0-9]+)$') {
            $low = [int]$Matches[1]
            $high = [int]$Matches[2]
            if ($Ports | Where-Object { $_ -ge $low -and $_ -le $high }) { return $true }
        }
    }
    return $false
}

try {
    if ($Action -eq 'list') {
        @{ ok=$true; addresses=@(Get-DirectAddresses | Select-Object -ExpandProperty address -Unique) } | ConvertTo-Json -Compress
        exit 0
    }

    $phase = 'existing-rules'
    $owned = @(Get-OwnedDirectRules)
    if ($Action -eq 'remove') {
        $phase = 'remove-rules'
        Remove-OwnedDirectRules $owned
        if (@(Get-OwnedDirectRules).Count) { throw 'rules-not-removed' }
        @{ ok=$true } | ConvertTo-Json -Compress
        exit 0
    }

    if ($owned.Count -ne 0) { throw 'rule-conflict' }
    $phase = 'program-path'
    # NetSecurity requires a standard drive path for the executable filter.
    $programPath = [IO.Path]::GetFullPath($Program)
    if ($programPath -match '^\\\\\?\\[A-Za-z]:\\') { $programPath = $programPath.Substring(4) }
    $programFile = Get-Item -LiteralPath $programPath
    if ($programFile.PSIsContainer -or $programFile.Name -ine 'sunshine.exe') { throw 'program-invalid' }
    $executable = $programFile.FullName
    $phase = 'interfaces'
    $ip = [Net.IPAddress]::Parse($LocalAddress).ToString()
    $interfaces = @(Get-DirectAddresses | Where-Object address -eq $ip)
    if ($interfaces.Count -ne 1) { throw 'address-not-bound' }
    $adapter = $interfaces[0]
    $profiles = @(Get-NetConnectionProfile -InterfaceIndex $adapter.index -ErrorAction SilentlyContinue | ForEach-Object {
        if ($_.NetworkCategory -eq 'DomainAuthenticated') { 'Domain' } else { [string]$_.NetworkCategory }
    })
    $tcp = @(($BasePort - 5), $BasePort, ($BasePort + 21))
    $udp = @(($BasePort + 9), ($BasePort + 10), ($BasePort + 11), ($BasePort + 13))
    $remote = if ($Access -eq 'lan') { 'LocalSubnet' } else { 'Any' }

    $phase = 'check-policy'
    $activeProfiles = @(Get-NetFirewallProfile -PolicyStore ActiveStore | Where-Object { $_.Name -in $profiles -and $_.Enabled -eq 'True' })
    if ($activeProfiles | Where-Object AllowInboundRules -eq 'False') { throw 'inbound-disabled' }
    if ($activeProfiles | Where-Object AllowLocalFirewallRules -eq 'False') { throw 'local-rules-disabled' }
    $blocked = Get-NetFirewallRule -PolicyStore ActiveStore | Where-Object {
        $_.Direction -eq 'Inbound' -and $_.Action -eq 'Block' -and $_.Enabled -eq 'True'
    }
    foreach ($rule in $blocked) {
        $application = $rule | Get-NetFirewallApplicationFilter
        if ([Environment]::ExpandEnvironmentVariables([string]$application.Program) -ne $executable) { continue }
        $ruleProfiles = @(([string]$rule.Profile -split ',') | ForEach-Object { $_.Trim() })
        if ($profiles.Count -gt 0 -and 'Any' -notin $ruleProfiles -and -not ($profiles | Where-Object { $_ -in $ruleProfiles })) { continue }
        $addresses = $rule | Get-NetFirewallAddressFilter
        if ('Any' -notin $addresses.LocalAddress -and $ip -notin $addresses.LocalAddress) { continue }
        if ('Any' -notin $addresses.RemoteAddress -and 'LocalSubnet' -notin $addresses.RemoteAddress) { continue }
        $ports = $rule | Get-NetFirewallPortFilter
        if (($ports.Protocol -in @('TCP', '6', 'Any') -and (Test-DirectPortOverlap $ports.LocalPort $tcp)) -or
            ($ports.Protocol -in @('UDP', '17', 'Any') -and (Test-DirectPortOverlap $ports.LocalPort $udp))) {
            throw 'firewall-block'
        }
    }

    $parameters = @{
        PolicyStore='PersistentStore'; Group=$directGroup; Program=$executable; LocalAddress=$ip
        InterfaceAlias=$adapter.interface; Direction='Inbound'; Action='Allow'; Enabled='True'
        Profile='Any'; RemoteAddress=$remote; EdgeTraversalPolicy='Block'
    }
    try {
        $phase = 'create-tcp'
        New-NetFirewallRule @parameters -Name $directNames[0] -DisplayName $directNames[0] -Protocol TCP -LocalPort $tcp | Out-Null
        $phase = 'create-udp'
        New-NetFirewallRule @parameters -Name $directNames[1] -DisplayName $directNames[1] -Protocol UDP -LocalPort $udp | Out-Null
        $phase = 'verify-rules'
        $active = @(Get-NetFirewallRule -PolicyStore ActiveStore | Where-Object {
            $_.Name -in $directNames -and $_.Group -eq $directGroup -and $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' -and $_.Direction -eq 'Inbound'
        })
        if (@($active.Name | Select-Object -Unique).Count -ne $directNames.Count) { throw 'rules-ineffective' }
    } catch {
        $firstError = $_
        try {
            Remove-OwnedDirectRules @(Get-OwnedDirectRules)
        } catch {
            # The native caller retains the marker and retries only these rules.
        }
        throw $firstError
    }
    @{ ok=$true } | ConvertTo-Json -Compress
} catch {
    $code = $_.Exception.Message
    $detail = $_.Exception
    while ($detail.InnerException) { $detail = $detail.InnerException }
    $hresult = [long]$detail.HResult
    $nativeError = $null
    if ($detail.PSObject.Properties['NativeErrorCode']) { $nativeError = [int]$detail.NativeErrorCode }
    if ($code -notin @('rule-conflict','address-not-bound','firewall-block','rules-ineffective','rules-not-removed','program-invalid','inbound-disabled','local-rules-disabled')) {
        if ($_.CategoryInfo.Category -eq 'PermissionDenied' -or $hresult -eq -2147024891 -or ($detail -is [ComponentModel.Win32Exception] -and $nativeError -eq 5)) {
            $code = 'access-denied'
        } else {
            $code = 'firewall-operation-failed'
        }
    }
    @{ ok=$false; code=$code; phase=$phase; hresult=$hresult; nativeErrorCode=$nativeError } | ConvertTo-Json -Compress
    exit 1
}
