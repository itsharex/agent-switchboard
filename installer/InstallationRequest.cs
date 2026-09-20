using System;
using System.IO;

namespace AgentSwitchboard.Installer
{
    /// The validated user intent of one install. Engine command lines are
    /// owned by <see cref="InstallerEngine"/>; this type only asserts that
    /// the requested directory is a writable-looking absolute path.
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
