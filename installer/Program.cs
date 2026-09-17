using System;
using System.Windows;

namespace AgentSwitchboard.Installer
{
    internal static class Program
    {
        [STAThread]
        private static int Main(string[] args)
        {
            try
            {
                var invocation = InstallerInvocation.Parse(args);
                var application = new Application();
                var window = new InstallerWindow(invocation);
                application.Run(window);
                return window.ExitCode;
            }
            catch (Exception error)
            {
                var text = new InstallerText();
                MessageBox.Show(
                    text.StartupFailure(error),
                    text.WindowTitle,
                    MessageBoxButton.OK,
                    MessageBoxImage.Error);
                return 2;
            }
        }
    }
}
