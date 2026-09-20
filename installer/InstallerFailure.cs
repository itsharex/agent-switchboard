using System;

namespace AgentSwitchboard.Installer
{
    internal enum InstallerFailureKind
    {
        TemporaryWorkspace,
        InvalidDirectory,
        WebViewRuntime,
        EngineLaunch,
        EngineExecution,
        ElevationDeclined,
    }

    internal sealed class InstallerException : Exception
    {
        internal InstallerException(
            InstallerFailureKind kind,
            string detail,
            Exception innerException,
            int? exitCode)
            : base(detail, innerException)
        {
            Kind = kind;
            ExitCode = exitCode;
        }

        internal InstallerFailureKind Kind { get; private set; }

        internal int? ExitCode { get; private set; }
    }
}
