using System;
using System.IO;
using System.Text;

namespace AgentSwitchboard.Installer
{
    internal sealed class InstallationRequest
    {
        private InstallationRequest(string directory, bool update)
        {
            Directory = directory;
            Update = update;
        }

        internal string Directory { get; private set; }

        internal bool Update { get; private set; }

        internal static InstallationRequest Create(string directory, bool update)
        {
            return new InstallationRequest(NormalizeDirectory(directory), update);
        }

        internal string EngineArguments()
        {
            var arguments = new StringBuilder("/S");
            if (Update) arguments.Append(" /UPDATE");
            arguments.Append(" /D=").Append(Directory);
            return arguments.ToString();
        }

        private static string NormalizeDirectory(string directory)
        {
            if (String.IsNullOrWhiteSpace(directory)
                || !Path.IsPathRooted(directory)
                || directory.IndexOfAny(new[] { '"', '\r', '\n' }) >= 0)
                throw InvalidDirectory(null);

            try
            {
                string full = Path.GetFullPath(directory).TrimEnd(
                    Path.DirectorySeparatorChar,
                    Path.AltDirectorySeparatorChar);
                string root = Path.GetPathRoot(directory);
                if (String.IsNullOrWhiteSpace(full) || String.IsNullOrWhiteSpace(root))
                    throw InvalidDirectory(null);

                string normalizedRoot = root.TrimEnd(
                    Path.DirectorySeparatorChar,
                    Path.AltDirectorySeparatorChar);
                if (String.Equals(full, normalizedRoot, StringComparison.OrdinalIgnoreCase))
                    throw InvalidDirectory(null);

                return full;
            }
            catch (InstallerException)
            {
                throw;
            }
            catch (ArgumentException error)
            {
                throw InvalidDirectory(error);
            }
            catch (NotSupportedException error)
            {
                throw InvalidDirectory(error);
            }
            catch (PathTooLongException error)
            {
                throw InvalidDirectory(error);
            }
            catch (System.Security.SecurityException error)
            {
                throw InvalidDirectory(error);
            }
        }

        private static InstallerException InvalidDirectory(Exception error)
        {
            return new InstallerException(
                InstallerFailureKind.InvalidDirectory,
                "The selected installation directory is not valid.",
                error,
                null);
        }
    }
}
