param([Parameter(Mandatory = $true)][string]$LocalAddress)
$ErrorActionPreference = 'Stop'
$taskAddress = [System.Net.IPAddress]::Parse($LocalAddress)
$taskInterfaces = @(Get-NetIPAddress -IPAddress $taskAddress.ToString() -ErrorAction SilentlyContinue)
$taskVirtualDescription = [string]([char]0x865A) + [char]0x62DF + [char]0x7F51 + [char]0x7EDC
$taskPeers = [System.Collections.Generic.List[string]]::new()
$taskSeen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$taskExcluded = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
[void]$taskExcluded.Add($taskAddress.ToString())

function Add-TaskPeer([string]$Value) {
    $taskCandidate = $null
    if (-not [Net.IPAddress]::TryParse($Value, [ref]$taskCandidate)) { return }
    if ([Net.IPAddress]::IsLoopback($taskCandidate) -or $taskExcluded.Contains($taskCandidate.ToString())) { return }
    $taskBytes = $taskCandidate.GetAddressBytes()
    if ($taskCandidate.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork) {
        if ($taskBytes[0] -eq 0 -or $taskBytes[0] -ge 224 -or $taskCandidate.Equals([Net.IPAddress]::Any)) { return }
    } elseif ($taskCandidate.IsIPv6Multicast -or $taskCandidate.IsIPv6LinkLocal -or $taskCandidate.Equals([Net.IPAddress]::IPv6Any)) { return }
    $taskText = $taskCandidate.ToString()
    if ($taskSeen.Add($taskText)) { $taskPeers.Add($taskText) }
}

foreach ($taskInterface in $taskInterfaces) {
    $taskAdapter = Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Where-Object { $_.ifIndex -eq $taskInterface.InterfaceIndex }
    if (-not $taskAdapter -or
        ($taskAdapter.InterfaceDescription -notmatch ('Wintun|' + $taskVirtualDescription) -and $taskAdapter.DriverFileName -notmatch 'wintun\.sys$')) { continue }
    # Never discover peers through a physical LAN/default gateway. Only the
    # already established Wintun interface corresponding to LocalAddress is read.
    if ($taskAddress.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $taskInterface.PrefixLength -le 30) {
        $taskBytes = $taskAddress.GetAddressBytes()
        [uint64]$taskNumber = ([uint64]$taskBytes[0] -shl 24) -bor ([uint64]$taskBytes[1] -shl 16) -bor ([uint64]$taskBytes[2] -shl 8) -bor [uint64]$taskBytes[3]
        [uint64]$taskHostMask = ([uint64]1 -shl (32 - [int]$taskInterface.PrefixLength)) - 1
        [uint64]$taskNetwork = $taskNumber -band ([uint64]4294967295 -bxor $taskHostMask)
        [uint64]$taskBroadcast = $taskNetwork -bor $taskHostMask
        foreach ($taskExcludedNumber in @($taskNetwork,$taskBroadcast)) {
            $taskExcludedBytes = [byte[]]@((($taskExcludedNumber -shr 24) -band 255), (($taskExcludedNumber -shr 16) -band 255), (($taskExcludedNumber -shr 8) -band 255), ($taskExcludedNumber -band 255))
            [void]$taskExcluded.Add(([Net.IPAddress]::new($taskExcludedBytes)).ToString())
        }
    }
    $taskRoutes = @(Get-NetRoute -InterfaceIndex $taskInterface.InterfaceIndex -ErrorAction SilentlyContinue)
    foreach ($taskRoute in $taskRoutes) {
        $taskDestination = $taskRoute.DestinationPrefix -split '/',2
        $taskNextHop = $null
        if ($taskDestination.Count -ne 2 -or -not [Net.IPAddress]::TryParse($taskRoute.NextHop,[ref]$taskNextHop)) { continue }
        $taskHostPrefix = if ($taskAddress.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork) { '32' } else { '128' }
        if ($taskDestination[1] -eq $taskHostPrefix -and
            ($taskNextHop.Equals([Net.IPAddress]::Any) -or $taskNextHop.Equals([Net.IPAddress]::IPv6Any))) {
            Add-TaskPeer $taskDestination[0]
        }
    }
    foreach ($taskNeighbor in @(Get-NetNeighbor -InterfaceIndex $taskInterface.InterfaceIndex -ErrorAction SilentlyContinue)) {
        if ($taskNeighbor.State -in @('Reachable','Stale','Delay','Probe','Permanent')) { Add-TaskPeer $taskNeighbor.IPAddress }
    }
    foreach ($taskRoute in $taskRoutes) { Add-TaskPeer $taskRoute.NextHop }
}
ConvertTo-Json -InputObject @($taskPeers.ToArray()) -Compress
