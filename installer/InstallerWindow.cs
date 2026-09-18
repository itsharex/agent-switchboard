using System;
using System.ComponentModel;
using System.IO;
using System.Reflection;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Markup;
using System.Windows.Shell;

namespace AgentSwitchboard.Installer
{
    internal sealed partial class InstallerWindow : Window
    {
        private readonly InstallerInvocation invocation;
        private readonly InstallerText copy;
        private TextBox directory;
        private TextBlock title;
        private TextBlock status;
        private TextBlock diagnostic;
        private TextBlock existing;
        private Button primary;
        private Button cancel;
        private Button close;
        private Button browse;
        private CheckBox launch;
        private ProgressBar progress;
        private string installedDirectory;
        private bool running;
        private bool completed;
        private bool restartFailed;

        internal InstallerWindow(InstallerInvocation invocation)
        {
            this.invocation = invocation;
            copy = new InstallerText();
            ExitCode = 1;
            LoadResources();
            ConfigureWindow();
            Content = BuildLayout();
            UpdateExisting();
            SourceInitialized += delegate { ApplySystemMaterial(); };
            Closing += OnClosing;
            PreviewKeyDown += OnPreviewKeyDown;
            Loaded += OnLoaded;
        }

        internal int ExitCode { get; private set; }

        private void LoadResources()
        {
            using (Stream stream = Assembly.GetExecutingAssembly().GetManifestResourceStream(
                "AgentSwitchboard.Installer.Theme.xaml"))
            {
                if (stream == null) throw new InvalidDataException("Installer theme is missing.");
                Resources = (ResourceDictionary)XamlReader.Load(stream);
            }
        }

        private void ConfigureWindow()
        {
            Title = copy.WindowTitle;
            Height = Math.Min(600, SystemParameters.WorkArea.Height - 32);
            Width = Math.Min(680, SystemParameters.WorkArea.Width - 32);
            WindowStartupLocation = WindowStartupLocation.CenterScreen;
            WindowStyle = WindowStyle.None;
            ResizeMode = ResizeMode.CanMinimize;
            Background = BackgroundBrush();
            FontFamily = Font("InterfaceFont");
            UseLayoutRounding = true;
            WindowChrome.SetWindowChrome(this, new WindowChrome
            {
                CaptionHeight = 52,
                ResizeBorderThickness = new Thickness(0),
                CornerRadius = new CornerRadius(0),
                GlassFrameThickness = new Thickness(0),
            });
        }

        private async void OnLoaded(object sender, RoutedEventArgs eventArgs)
        {
            if (invocation.StartsImmediately) await InstallAsync();
            else primary.Focus();
        }

        private async void OnPrimaryClick(object sender, RoutedEventArgs eventArgs)
        {
            if (completed) Finish();
            else await InstallAsync();
        }

        private async System.Threading.Tasks.Task InstallAsync()
        {
            if (running) return;

            running = true;
            ExitCode = 1;
            EnterRunningState();
            try
            {
                bool update = invocation.IsUpdate || InstallerEngine.IsInstalledDirectory(directory.Text);
                var request = InstallationRequest.Create(directory.Text, update);
                installedDirectory = request.Directory;
                var result = await InstallerEngine.Run(request, invocation);
                EnterCompletedState(result);
            }
            catch (Exception error)
            {
                EnterFailureState(error);
            }
            finally
            {
                LeaveRunningState();
            }

            if (invocation.StartsImmediately && completed && !restartFailed) Close();
        }

        private void EnterRunningState()
        {
            directory.IsEnabled = false;
            browse.IsEnabled = false;
            primary.IsEnabled = false;
            cancel.IsEnabled = false;
            close.IsEnabled = false;
            launch.Visibility = Visibility.Collapsed;
            progress.Visibility = Visibility.Visible;
            progress.IsIndeterminate = SystemParameters.ClientAreaAnimation;
            title.Text = copy.InstallingTitle;
            status.Foreground = Brush("Muted");
            status.Text = copy.InstallingStatus;
            status.Visibility = Visibility.Visible;
            diagnostic.Visibility = Visibility.Collapsed;
        }

        private void EnterCompletedState(InstallResult result)
        {
            completed = true;
            ExitCode = 0;
            title.Text = copy.CompleteTitle;
            status.Foreground = Brush("Ink");
            status.Text = copy.CompleteStatus;
            restartFailed = result.LaunchError != null;
            diagnostic.Visibility = Visibility.Collapsed;

            if (restartFailed)
            {
                status.Text = copy.LaunchFailureStatus;
                diagnostic.Text = copy.LaunchFailureDiagnostic(result.LaunchError);
                diagnostic.Visibility = Visibility.Visible;
            }

            primary.Content = copy.Finish;
            cancel.Visibility = Visibility.Collapsed;
            launch.Content = copy.LaunchWhenFinished;
            // 勾选（打开应用）是完成页的默认意图；不勾选才是仅完成。
            launch.IsChecked = true;
            launch.Visibility = invocation.RestartAfterInstall && !restartFailed
                ? Visibility.Collapsed
                : Visibility.Visible;
        }

        private void EnterFailureState(Exception error)
        {
            completed = false;
            restartFailed = false;
            title.Text = copy.FailureTitle(error);
            status.Foreground = Brush("Error");
            status.Text = copy.FailureStatus(error);
            status.Visibility = Visibility.Visible;
            diagnostic.Text = copy.FailureDiagnostic(error);
            diagnostic.Visibility = String.IsNullOrWhiteSpace(diagnostic.Text)
                ? Visibility.Collapsed
                : Visibility.Visible;
            primary.Content = copy.Retry;
            cancel.Visibility = Visibility.Visible;
            launch.Visibility = Visibility.Collapsed;
        }

        private void LeaveRunningState()
        {
            running = false;
            progress.IsIndeterminate = false;
            progress.Visibility = Visibility.Collapsed;
            primary.IsEnabled = true;
            cancel.IsEnabled = true;
            close.IsEnabled = true;
            if (!completed && invocation.Kind == InstallerInvocationKind.Interactive)
            {
                directory.IsEnabled = true;
                browse.IsEnabled = true;
            }
            primary.Focus();
        }

        private void Finish()
        {
            if (launch.IsChecked == true && (!invocation.RestartAfterInstall || restartFailed))
            {
                try { InstallerEngine.Launch(installedDirectory); }
                catch (Exception error)
                {
                    status.Foreground = Brush("Error");
                    status.Text = copy.LaunchFailureStatus;
                    diagnostic.Text = copy.LaunchFailureDiagnostic(error.Message);
                    diagnostic.Visibility = Visibility.Visible;
                    launch.IsChecked = false;
                    return;
                }
            }
            Close();
        }

        private void UpdateExisting()
        {
            bool existingInstallation = InstallerEngine.IsInstalledDirectory(directory.Text);
            existing.Text = existingInstallation
                ? copy.ExistingInstallationNotice
                : copy.NewInstallationNotice;
            if (!running && !completed && primary != null)
                primary.Content = existingInstallation ? copy.Update : copy.Install;
        }

        private void Browse(object sender, RoutedEventArgs eventArgs)
        {
            using (var dialog = new System.Windows.Forms.FolderBrowserDialog())
            {
                dialog.Description = copy.ChooseInstallLocation;
                if (System.IO.Directory.Exists(directory.Text)) dialog.SelectedPath = directory.Text;
                if (dialog.ShowDialog() == System.Windows.Forms.DialogResult.OK)
                    directory.Text = dialog.SelectedPath;
            }
        }

        private void OnPreviewKeyDown(object sender, KeyEventArgs eventArgs)
        {
            if (eventArgs.Key != Key.Escape) return;
            eventArgs.Handled = true;
            Close();
        }

        private void OnClosing(object sender, CancelEventArgs eventArgs)
        {
            if (!running) return;
            eventArgs.Cancel = true;
            status.Text = copy.InstallingCannotClose;
            status.Visibility = Visibility.Visible;
            diagnostic.Visibility = Visibility.Collapsed;
        }

        [System.Runtime.InteropServices.DllImport("dwmapi.dll", PreserveSig = true)]
        private static extern int DwmSetWindowAttribute(
            IntPtr window,
            int attribute,
            ref int value,
            int size);

        private void ApplySystemMaterial()
        {
            var handle = new WindowInteropHelper(this).Handle;
            int round = 2;
            DwmSetWindowAttribute(handle, 33, ref round, sizeof(int));
            int noBorder = unchecked((int)0xFFFFFFFE);
            DwmSetWindowAttribute(handle, 34, ref noBorder, sizeof(int));
        }
    }
}
