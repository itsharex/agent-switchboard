param([Parameter(Mandatory = $true)][string]$Reference)
$ErrorActionPreference = 'Stop'
if ($Reference -notmatch '^secret-[a-f0-9]{32}$') { throw 'Invalid E2E fixture reference' }

# Used only by sandbox.test.mjs to verify native cleanup, never by desktop specs.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class SwitchboardCredentialFixture {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct Credential {
        public uint Flags, Type;
        public string TargetName, Comment;
        public long LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist, AttributeCount;
        public IntPtr Attributes;
        public string TargetAlias, UserName;
    }
    [DllImport("advapi32.dll", EntryPoint = "CredWriteW", CharSet = CharSet.Unicode,
        SetLastError = true)]
    private static extern bool Write(ref Credential entry, uint flags);

    public static void Create(string reference) {
        const string value = "e2e-support-test-only";
        IntPtr blob = Marshal.StringToCoTaskMemUni(value);
        try {
            var entry = new Credential {
                Type = 1, TargetName = "Agent Switchboard E2E." + reference,
                UserName = reference, CredentialBlobSize = (uint)value.Length * 2,
                CredentialBlob = blob, Persist = 2
            };
            if (!Write(ref entry, 0)) throw new Win32Exception(Marshal.GetLastWin32Error());
        } finally { Marshal.ZeroFreeCoTaskMemUnicode(blob); }
    }
}
'@
[SwitchboardCredentialFixture]::Create($Reference)
