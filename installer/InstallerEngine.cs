using System;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Reflection;
using System.Threading.Tasks;
using Microsoft.Win32;

namespace AgentSwitchboard.Installer
{
    internal sealed class InstallResult
    {
        internal string LaunchError { get; set; }
    }

    internal static class InstallerEngine
    {
        private const string PayloadResource = "AgentSwitchboard.Installer.Engine.exe";

        private enum InstalledDirectoryState
        {
            Present,
            Missing,
            Unknown,
        }

        internal static string DetectDirectory()
        {
            string installed = TryReadInstalledDirectory();
            if (!String.IsNullOrWhiteSpace(installed))
            {
                switch (GetInstalledDirectoryState(installed))
                {
                    case InstalledDirectoryState.Present:
                        return installed;
                    case InstalledDirectoryState.Unknown:
                        return installed;
                    default:
                        RemoveStaleInstalledDirectory();
                        break;
                }
            }
            return Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                InstallerProductMetadata.ProductName);
        }

        internal static async Task<InstallResult> Run(
            InstallationRequest request,
            InstallerInvocation invocation)
        {
            string temporary = CreateTemporaryDirectory();
            try
            {
                await EnsureWebViewRuntime(temporary);
                string engine = await ExtractEngine(temporary);
                int exitCode = await RunEngine(engine, request.EngineArguments());
                if (exitCode != 0)
                    throw new InstallerException(
                        InstallerFailureKind.EngineExecution,
                        "The installation engine returned a non-zero exit code.",
                        null,
                        exitCode);

                var result = new InstallResult();
                if (invocation.RestartAfterInstall)
                {
                    try { Launch(request.Directory, invocation.ApplicationArguments()); }
                    catch (Exception error)
                    {
                        result.LaunchError = error.Message;
                        Trace.TraceWarning("Application restart: " + error.Message);
                    }
                }
                return result;
            }
            finally
            {
                CleanupTemporaryDirectory(temporary);
            }
        }

        internal static void Launch(string directory)
        {
            Launch(directory, String.Empty);
        }

        internal static bool IsInstalledDirectory(string directory)
        {
            return GetInstalledDirectoryState(directory) == InstalledDirectoryState.Present;
        }

        private static InstalledDirectoryState GetInstalledDirectoryState(string directory)
        {
            if (String.IsNullOrWhiteSpace(directory)) return InstalledDirectoryState.Missing;
            try
            {
                string application = Path.Combine(
                    directory,
                    InstallerProductMetadata.ApplicationFileStem + ".exe");
                FileAttributes attributes = File.GetAttributes(application);
                return (attributes & FileAttributes.Directory) == 0
                    ? InstalledDirectoryState.Present
                    : InstalledDirectoryState.Missing;
            }
            catch (FileNotFoundException)
            {
                return InstalledDirectoryState.Missing;
            }
            catch (DirectoryNotFoundException)
            {
                return InstalledDirectoryState.Missing;
            }
            catch (ArgumentException)
            {
                return InstalledDirectoryState.Missing;
            }
            catch (NotSupportedException)
            {
                return InstalledDirectoryState.Missing;
            }
            catch (PathTooLongException)
            {
                return InstalledDirectoryState.Missing;
            }
            catch (IOException error)
            {
                Trace.TraceWarning("Installer directory lookup: " + error.Message);
                return InstalledDirectoryState.Unknown;
            }
            catch (UnauthorizedAccessException error)
            {
                Trace.TraceWarning("Installer directory lookup: " + error.Message);
                return InstalledDirectoryState.Unknown;
            }
            catch (System.Security.SecurityException error)
            {
                Trace.TraceWarning("Installer directory lookup: " + error.Message);
                return InstalledDirectoryState.Unknown;
            }
        }

        private static string RegistryPath
        {
            get
            {
                return "Software\\" + InstallerProductMetadata.ApplicationFileStem
                    + "\\" + InstallerProductMetadata.ProductName;
            }
        }

        private static string CreateTemporaryDirectory()
        {
            try
            {
                string temporary = Path.Combine(
                    Path.GetTempPath(),
                    "agent-switchboard-setup-" + Guid.NewGuid().ToString("N"));
                System.IO.Directory.CreateDirectory(temporary);
                return temporary;
            }
            catch (IOException error)
            {
                throw TemporaryWorkspace(error);
            }
            catch (UnauthorizedAccessException error)
            {
                throw TemporaryWorkspace(error);
            }
            catch (System.Security.SecurityException error)
            {
                throw TemporaryWorkspace(error);
            }
        }

        private static InstallerException TemporaryWorkspace(Exception error)
        {
            return new InstallerException(
                InstallerFailureKind.TemporaryWorkspace,
                "The installer could not create its temporary workspace.",
                error,
                null);
        }

        private static string TryReadInstalledDirectory()
        {
            try
            {
                using (var hive = RegistryKey.OpenBaseKey(RegistryHive.CurrentUser, RegistryView.Registry64))
                using (var key = hive.OpenSubKey(RegistryPath))
                    return key == null ? null : key.GetValue(null) as string;
            }
            catch (IOException error)
            {
                Trace.TraceWarning("Installer directory lookup: " + error.Message);
                return null;
            }
            catch (UnauthorizedAccessException error)
            {
                Trace.TraceWarning("Installer directory lookup: " + error.Message);
                return null;
            }
            catch (System.Security.SecurityException error)
            {
                Trace.TraceWarning("Installer directory lookup: " + error.Message);
                return null;
            }
        }

        private static void RemoveStaleInstalledDirectory()
        {
            try
            {
                using (var hive = RegistryKey.OpenBaseKey(RegistryHive.CurrentUser, RegistryView.Registry64))
                    hive.DeleteSubKeyTree(RegistryPath, false);
            }
            catch (IOException error)
            {
                Trace.TraceWarning("Installer stale-directory cleanup: " + error.Message);
            }
            catch (UnauthorizedAccessException error)
            {
                Trace.TraceWarning("Installer stale-directory cleanup: " + error.Message);
            }
            catch (System.Security.SecurityException error)
            {
                Trace.TraceWarning("Installer stale-directory cleanup: " + error.Message);
            }
        }

        private static async Task<string> ExtractEngine(string temporary)
        {
            string engine = Path.Combine(temporary, "engine.exe");
            try
            {
                using (var input = Assembly.GetExecutingAssembly().GetManifestResourceStream(PayloadResource))
                {
                    if (input == null)
                        throw new InvalidDataException("Installer engine resource is missing.");
                    using (var output = File.Create(engine))
                        await input.CopyToAsync(output);
                }
                return engine;
            }
            catch (Exception error)
            {
                throw new InstallerException(
                    InstallerFailureKind.EngineLaunch,
                    "The installation engine could not be prepared.",
                    error,
                    null);
            }
        }

        private static async Task EnsureWebViewRuntime(string temporary)
        {
            if (HasWebViewRuntime()) return;

            try
            {
                string bootstrapper = Path.Combine(temporary, "MicrosoftEdgeWebview2Setup.exe");
                ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12;
                using (var client = new WebClient())
                    await client.DownloadFileTaskAsync(
                        new Uri("https://go.microsoft.com/fwlink/p/?LinkId=2124703"),
                        bootstrapper);

                await RunProcess(bootstrapper, "/silent /install");
                if (!HasWebViewRuntime())
                    throw new IOException("WebView2 installation did not complete.");
            }
            catch (Exception error)
            {
                throw new InstallerException(
                    InstallerFailureKind.WebViewRuntime,
                    "Microsoft Edge WebView2 Runtime could not be prepared.",
                    error,
                    null);
            }
        }

        private static async Task<int> RunEngine(string executable, string arguments)
        {
            try
            {
                return await RunProcess(executable, arguments);
            }
            catch (Exception error)
            {
                throw new InstallerException(
                    InstallerFailureKind.EngineLaunch,
                    "The installation engine could not start.",
                    error,
                    null);
            }
        }

        private static Task<int> RunProcess(string executable, string arguments)
        {
            return Task.Run(() =>
            {
                using (var process = Process.Start(new ProcessStartInfo(executable, arguments)
                {
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    WorkingDirectory = Path.GetDirectoryName(executable),
                }))
                {
                    if (process == null)
                        throw new IOException("The installer process could not start.");
                    process.WaitForExit();
                    return process.ExitCode;
                }
            });
        }

        private static bool HasWebViewRuntime()
        {
            try
            {
                const string client = @"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
                foreach (var hive in new[] { RegistryHive.LocalMachine, RegistryHive.CurrentUser })
                foreach (var view in new[] { RegistryView.Registry32, RegistryView.Registry64 })
                using (var root = RegistryKey.OpenBaseKey(hive, view))
                using (var key = root.OpenSubKey(client))
                {
                    Version version;
                    if (key != null
                        && Version.TryParse(key.GetValue("pv") as string, out version)
                        && version.Major > 0)
                        return true;
                }
            }
            catch (IOException error)
            {
                Trace.TraceWarning("WebView runtime lookup: " + error.Message);
            }
            catch (UnauthorizedAccessException error)
            {
                Trace.TraceWarning("WebView runtime lookup: " + error.Message);
            }
            catch (System.Security.SecurityException error)
            {
                Trace.TraceWarning("WebView runtime lookup: " + error.Message);
            }
            return false;
        }

        private static void CleanupTemporaryDirectory(string temporary)
        {
            try { System.IO.Directory.Delete(temporary, true); }
            catch (IOException error) { Trace.TraceWarning("Installer staging cleanup: " + error.Message); }
            catch (UnauthorizedAccessException error)
            {
                Trace.TraceWarning("Installer staging cleanup: " + error.Message);
            }
        }

        private static void Launch(string directory, string arguments)
        {
            string executable = Path.Combine(
                directory,
                InstallerProductMetadata.ApplicationFileStem + ".exe");
            var process = Process.Start(new ProcessStartInfo(executable, arguments)
            {
                UseShellExecute = true,
                WorkingDirectory = directory,
            });
            if (process == null) throw new IOException("The application could not start.");
        }
    }
}
