param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Snapshot', 'Cleanup')]
    [string]$Mode,
    [string]$InputPath
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class SwitchboardE2ECredentials {
    public const string Prefix = "Agent Switchboard E2E.";
    private const int NotFound = 1168;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct Credential {
        public uint Flags, Type;
        public IntPtr TargetName, Comment;
        public long LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist, AttributeCount;
        public IntPtr Attributes, TargetAlias, UserName;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredEnumerateW", CharSet = CharSet.Unicode,
        SetLastError = true)]
    private static extern bool Enumerate(string filter, uint flags, out uint count, out IntPtr entries);
    [DllImport("advapi32.dll", EntryPoint = "CredDeleteW", CharSet = CharSet.Unicode,
        SetLastError = true)]
    private static extern bool Delete(string target, uint type, uint flags);
    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode,
        SetLastError = true)]
    private static extern bool Read(string target, uint type, uint flags, out IntPtr entry);
    [DllImport("advapi32.dll")]
    private static extern void CredFree(IntPtr entry);

    public static string[] Names() {
        uint count;
        IntPtr entries;
        if (!Enumerate(Prefix + "*", 0, out count, out entries)) {
            int error = Marshal.GetLastWin32Error();
            if (error == NotFound) return new string[0];
            throw new Win32Exception(error, "Could not enumerate the E2E credential namespace");
        }
        try {
            var names = new List<string>();
            for (int index = 0; index < count; index++) {
                IntPtr pointer = Marshal.ReadIntPtr(entries, index * IntPtr.Size);
                var credential = (Credential)Marshal.PtrToStructure(pointer, typeof(Credential));
                string target = Marshal.PtrToStringUni(credential.TargetName);
                if (!target.StartsWith(Prefix, StringComparison.Ordinal))
                    throw new InvalidOperationException("Credential namespace filter escaped");
                names.Add(target);
            }
            names.Sort(StringComparer.Ordinal);
            return names.ToArray();
        } finally { CredFree(entries); }
    }

    public static void DeleteAndVerify(string target) {
        if (!target.StartsWith(Prefix, StringComparison.Ordinal))
            throw new InvalidOperationException("Refusing to delete a non-E2E credential");
        if (!Delete(target, 1, 0)) {
            int error = Marshal.GetLastWin32Error();
            if (error != NotFound) throw new Win32Exception(error, "E2E credential deletion failed");
        }
        IntPtr entry;
        if (Read(target, 1, 0, out entry)) {
            CredFree(entry);
            throw new InvalidOperationException("E2E credential remained after deletion");
        }
        int readError = Marshal.GetLastWin32Error();
        if (readError != NotFound)
            throw new Win32Exception(readError, "Could not verify E2E credential removal");
    }
}
'@

if ($Mode -eq 'Snapshot') {
    ConvertTo-Json -Compress -InputObject @([SwitchboardE2ECredentials]::Names())
    exit 0
}

if (-not $InputPath) { throw 'Cleanup requires an input manifest' }
$manifest = Get-Content -LiteralPath $InputPath -Raw -Encoding UTF8 | ConvertFrom-Json
$baseline = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$targets = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$deleted = [Collections.Generic.List[string]]::new()
$errors = [Collections.Generic.List[object]]::new()
$baselineValid = $manifest.baseline -is [Array]
if (-not $baselineValid) {
    $errors.Add(@{ stage = 'baseline'; message = 'Credential baseline is missing or is not an array' })
}
foreach ($target in $manifest.baseline) {
    if ($target -isnot [string] -or -not $target.StartsWith([SwitchboardE2ECredentials]::Prefix, [StringComparison]::Ordinal)) {
        $baselineValid = $false
        $errors.Add(@{ stage = 'baseline'; message = 'Baseline contains a non-E2E target' })
    } else {
        [void]$baseline.Add($target)
    }
}
if ($baselineValid) {
    try {
        foreach ($target in [SwitchboardE2ECredentials]::Names()) {
            if (-not $baseline.Contains($target)) { [void]$targets.Add($target) }
        }
    } catch { $errors.Add(@{ stage = 'enumeration'; message = $_.Exception.Message }) }
}
if ($manifest.references -isnot [Array]) {
    $errors.Add(@{ stage = 'references'; message = 'Credential references must be an array' })
}
foreach ($reference in $manifest.references) {
    if ($reference -isnot [string] -or $reference -cnotmatch '^secret-[a-f0-9]{32}$') {
        $errors.Add(@{ stage = 'references'; message = 'Invalid E2E credential reference' })
    } elseif ($baselineValid) {
        $target = [SwitchboardE2ECredentials]::Prefix + $reference
        if (-not $baseline.Contains($target)) { [void]$targets.Add($target) }
    }
}
foreach ($target in $targets) {
    try {
        [SwitchboardE2ECredentials]::DeleteAndVerify($target)
        $deleted.Add($target)
    } catch { $errors.Add(@{ stage = 'deletion'; target = $target; message = $_.Exception.Message }) }
}
$remaining = $null
if ($baselineValid) {
    try {
        $remaining = @([SwitchboardE2ECredentials]::Names() | Where-Object { -not $baseline.Contains($_) })
        if ($remaining.Count -ne 0) {
            $errors.Add(@{ stage = 'verification'; message = 'New E2E credentials remain after cleanup' })
        }
    } catch { $errors.Add(@{ stage = 'verification'; message = $_.Exception.Message }) }
}
$verified = $errors.Count -eq 0
ConvertTo-Json -Depth 4 -Compress -InputObject @{
    verified = $verified
    baselineCount = $baseline.Count
    deletedTargets = @($deleted | Sort-Object)
    remainingNewTargets = $remaining
    errors = @($errors.ToArray())
}
if (-not $verified) { exit 1 }
