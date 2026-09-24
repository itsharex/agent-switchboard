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
    internal enum InstallerViewState
    {
        Ready,
        Installing,
        Complete,
        Failure,
    }

    internal sealed partial class InstallerWindow : Window
    {
        private readonly InstallerInvocation invocation;
        private readonly InstallerText copy;
        private TextBox directory;
        private TextBlock title;
        private TextBlock summary;
        private TextBlock version;
        private TextBlock diagnostic;
        private TextBlock existing;
        private FrameworkElement locationCard;
        private Button primary;
        private Button cancel;
        private Button close;
        private Button browse;
        private CheckBox launch;
        private ProgressBar progress;
        private string installedDirectory;
        private InstallerViewState viewState = InstallerViewState.Ready;
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
            if (viewState == InstallerViewState.Complete) Finish();
            else await InstallAsync();
        }

        private async System.Threading.Tasks.Task InstallAsync()
        {
            if (viewState == InstallerViewState.Installing) return;

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

            if (invocation.StartsImmediately && viewState == InstallerViewState.Complete && !restartFailed) Close();
        }

        private void EnterRunningState()
        {
            viewState = InstallerViewState.Installing;
            directory.IsEnabled = false;
            browse.IsEnabled = false;
            locationCard.Visibility = Visibility.Collapsed;
            version.Visibility = Visibility.Collapsed;
            title.Text = copy.InstallingTitle;
            summary.Foreground = Brush("Muted");
            summary.Text = copy.InstallingStatus;
            diagnostic.Visibility = Visibility.Collapsed;
            launch.Visibility = Visibility.Collapsed;
            progress.Value = SystemParameters.ClientAreaAnimation ? 0 : 35;
            progress.IsIndeterminate = SystemParameters.ClientAreaAnimation;
            progress.Visibility = Visibility.Visible;
            primary.IsEnabled = false;
            cancel.Visibility = Visibility.Collapsed;
            close.IsEnabled = false;
        }

        private void EnterCompletedState(InstallResult result)
        {
            viewState = InstallerViewState.Complete;
            ExitCode = 0;
            locationCard.Visibility = Visibility.Collapsed;
            version.Visibility = Visibility.Collapsed;
            progress.IsIndeterminate = false;
            progress.Visibility = Visibility.Collapsed;
            title.Text = copy.CompleteTitle;
            summary.Foreground = Brush("Muted");
            summary.Text = copy.CompleteStatus;
            restartFailed = result.LaunchError != null;
            diagnostic.Visibility = Visibility.Collapsed;

            if (restartFailed)
            {
                summary.Foreground = Brush("Error");
                summary.Text = copy.LaunchFailureStatus;
                diagnostic.Text = copy.LaunchFailureDiagnostic(result.LaunchError);
                diagnostic.Visibility = Visibility.Visible;
            }

            primary.Content = copy.Finish;
            cancel.Visibility = Visibility.Collapsed;
            launch.Content = copy.LaunchWhenFinished;
            launch.IsChecked = true;
            launch.Visibility = invocation.RestartAfterInstall && !restartFailed
                ? Visibility.Collapsed
                : Visibility.Visible;
        }

        private void EnterFailureState(Exception error)
        {
            viewState = InstallerViewState.Failure;
            restartFailed = false;
            version.Visibility = Visibility.Collapsed;
            progress.IsIndeterminate = false;
            progress.Visibility = Visibility.Collapsed;
            locationCard.Visibility = FailureUsesLocation(error)
                ? Visibility.Visible
                : Visibility.Collapsed;
            title.Text = copy.FailureTitle(error);
            summary.Foreground = Brush("Error");
            summary.Text = copy.FailureStatus(error);
            diagnostic.Text = copy.FailureDiagnostic(error);
            diagnostic.Visibility = String.IsNullOrWhiteSpace(diagnostic.Text)
                ? Visibility.Collapsed
                : Visibility.Visible;
            primary.Content = copy.Retry;
            cancel.Visibility = Visibility.Visible;
            launch.Visibility = Visibility.Collapsed;
        }

        private static bool FailureUsesLocation(Exception error)
        {
            var installerError = error as InstallerException;
            return installerError != null
                && installerError.Kind == InstallerFailureKind.InvalidDirectory;
        }

        private void LeaveRunningState()
        {
            primary.IsEnabled = true;
            cancel.IsEnabled = true;
            close.IsEnabled = true;
            if (viewState == InstallerViewState.Failure
                && locationCard.Visibility == Visibility.Visible
                && invocation.Kind == InstallerInvocationKind.Interactive
                && !InstallerProductMetadata.UsesMsiEngine)
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
                    summary.Foreground = Brush("Error");
                    summary.Text = copy.LaunchFailureStatus;
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
            if (viewState == InstallerViewState.Ready && primary != null)
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
            if (viewState != InstallerViewState.Installing) return;
            eventArgs.Cancel = true;
            summary.Text = copy.InstallingCannotClose;
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
