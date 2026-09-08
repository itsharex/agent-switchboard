$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$name = 'Global\AgentSwitchboard.DesktopE2E.' + $identity.User.Value
$mutex = [Threading.Mutex]::new($false, $name)
$acquired = $false
$abandoned = $false
try {
    try { $acquired = $mutex.WaitOne(0) }
    catch [Threading.AbandonedMutexException] {
        $acquired = $true
        $abandoned = $true
    }
    ConvertTo-Json -Compress -InputObject @{
        acquired = $acquired; abandoned = $abandoned; name = $name; ownerPid = $PID
    }
    [Console]::Out.Flush()
    if (-not $acquired) { exit 73 }
    # EOF also releases the mutex when the runner dies and its pipe closes.
    [void][Console]::In.ReadLine()
} finally {
    if ($acquired) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
    $identity.Dispose()
}
