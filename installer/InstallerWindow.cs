using System;
using System.ComponentModel;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Input;
using System.Windows.Markup;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using ShapePath = System.Windows.Shapes.Path;
using System.Windows.Shell;

namespace AgentSwitchboard.Installer
{
    internal sealed class InstallerWindow : Window
    {
        private readonly InstallOptions options;
        private readonly bool chinese = CultureInfo.CurrentUICulture.TwoLetterISOLanguageName == "zh";
        private readonly TextBox directory;
        private readonly TextBlock title;
        private readonly TextBlock status;
        private readonly TextBlock existing;
        private readonly Button primary;
        private readonly Button cancel;
        private readonly Button close;
        private readonly Button browse;
        private readonly CheckBox launch;
        private readonly ProgressBar progress;
        private bool running;
        private bool completed;
        private bool restartFailed;
        public int ExitCode { get; private set; }

        public InstallerWindow(InstallOptions options, string version)
        {
            this.options = options;
            ExitCode = 1;
            using (Stream stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("AgentSwitchboard.Installer.Theme.xaml"))
                Resources = (ResourceDictionary)XamlReader.Load(stream);
            Title = "Agent Switchboard";
            Height = Math.Min(620, SystemParameters.WorkArea.Height - 24);
            Width = Math.Min(640, SystemParameters.WorkArea.Width - 24);
            WindowStartupLocation = WindowStartupLocation.CenterScreen;
            WindowStyle = WindowStyle.None;
            ResizeMode = ResizeMode.CanMinimize;
            Background = Brush("Surface");
            FontFamily = (FontFamily)Resources["InterfaceFont"];
            UseLayoutRounding = true;
            WindowChrome.SetWindowChrome(this, new WindowChrome { CaptionHeight = 60, ResizeBorderThickness = new Thickness(0), CornerRadius = new CornerRadius(0), GlassFrameThickness = new Thickness(0) });
            var frame = new Grid { Background = BackgroundBrush() };
            Content = frame;
            SourceInitialized += delegate { ApplySystemMaterial(); };
            var layout = new Grid { Margin = new Thickness(28, 8, 28, 24) };
            frame.Children.Add(layout);
            layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            layout.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var header = new Grid { Height = 44, Background = Brushes.Transparent };
            header.ColumnDefinitions.Add(new ColumnDefinition());
            header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            layout.Children.Add(header);
            var brand = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
            AutomationProperties.SetName(brand, T("Agent Switchboard 安装程序", "Agent Switchboard installer"));
            var brandMark = CreateBrandMark(22);
            brandMark.Margin = new Thickness(0, 0, 10, 0);
            brand.Children.Add(brandMark);
            var brandName = Text("Agent Switchboard", "BodySize");
            brandName.FontWeight = FontWeights.SemiBold;
            brand.Children.Add(brandName);
            var brandContext = Text(T("安装程序", "Installer"), "CaptionSize");
            brandContext.Foreground = Brush("Muted");
            brandContext.Margin = new Thickness(10, 1, 0, 0);
            brand.Children.Add(brandContext);
            header.Children.Add(brand);
            var minimize = MakeTitlebarButton("−", false);
            minimize.Name = "MinimizeButton";
            AutomationProperties.SetName(minimize, T("最小化", "Minimize"));
            minimize.ToolTip = T("最小化", "Minimize");
            WindowChrome.SetIsHitTestVisibleInChrome(minimize, true);
            minimize.Click += delegate { WindowState = WindowState.Minimized; };
            Grid.SetColumn(minimize, 1);
            header.Children.Add(minimize);
            close = MakeTitlebarButton("×", true);
            close.ToolTip = T("关闭", "Close");
            AutomationProperties.SetName(close, T("关闭安装程序", "Close installer"));
            close.Click += delegate { Close(); };
            WindowChrome.SetIsHitTestVisibleInChrome(close, true);
            Grid.SetColumn(close, 2);
            header.Children.Add(close);
            var body = new StackPanel { Margin = new Thickness(0, 26, 0, 20) };
            var scroll = new ScrollViewer { Content = body, VerticalScrollBarVisibility = ScrollBarVisibility.Hidden, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled };
            Grid.SetRow(scroll, 1);
            layout.Children.Add(scroll);
            var intro = new StackPanel { Margin = new Thickness(0, 0, 0, 26) };
            title = Text(T("准备安装", "Ready to install"), "TitleSize");
            title.FontWeight = FontWeights.SemiBold;
            intro.Children.Add(title);
            var versionText = Text(T("版本 ", "Version ") + version, "CaptionSize");
            versionText.Foreground = Brush("Muted");
            versionText.Margin = new Thickness(0, 6, 0, 0);
            intro.Children.Add(versionText);
            body.Children.Add(intro);
            body.Children.Add(new Border { Height = 1, Background = Brush("Line"), Margin = new Thickness(0, 0, 0, 22) });
            var locationLabel = Text(T("安装位置", "Install location"), "BodySize");
            locationLabel.FontWeight = FontWeights.SemiBold;
            body.Children.Add(locationLabel);
            var pathRow = new Grid { Margin = new Thickness(0, 8, 0, 10) };
            pathRow.ColumnDefinitions.Add(new ColumnDefinition());
            pathRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            directory = new TextBox { Text = options.Directory ?? InstallerEngine.DetectDirectory() };
            AutomationProperties.SetName(directory, T("安装位置", "Install location"));
            pathRow.Children.Add(directory);
            browse = MakeButton(T("浏览…", "Browse…"), false);
            browse.Margin = new Thickness(8, 0, 0, 0);
            browse.Click += Browse;
            Grid.SetColumn(browse, 1);
            pathRow.Children.Add(browse);
            body.Children.Add(pathRow);
            existing = Text("", "CaptionSize");
            existing.Foreground = Brush("Muted");
            body.Children.Add(existing);
            directory.TextChanged += delegate { UpdateExisting(); };
            UpdateExisting();
            progress = new ProgressBar { Visibility = Visibility.Collapsed, Margin = new Thickness(0, 24, 0, 0) };
            body.Children.Add(progress);
            status = Text("", "BodySize");
            status.Visibility = Visibility.Collapsed;
            status.Margin = new Thickness(0, 12, 0, 0);
            AutomationProperties.SetLiveSetting(status, AutomationLiveSetting.Polite);
            body.Children.Add(status);
            launch = new CheckBox { Content = T("完成后启动应用", "Launch the application when finished"), IsChecked = false, Visibility = Visibility.Collapsed, Margin = new Thickness(0, 12, 0, 0) };
            body.Children.Add(launch);
            var footer = new Grid { Margin = new Thickness(0, 16, 0, 0) };
            footer.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            footer.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            footer.Children.Add(new Border { Height = 1, Background = Brush("Line") });
            var actions = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 16, 0, 0) };
            Grid.SetRow(actions, 1);
            footer.Children.Add(actions);
            Grid.SetRow(footer, 2);
            layout.Children.Add(footer);
            cancel = MakeButton(T("取消", "Cancel"), false);
            cancel.Click += delegate { Close(); };
            actions.Children.Add(cancel);
            primary = MakeButton(T("安装", "Install"), true);
            primary.Margin = new Thickness(12, 0, 0, 0);
            primary.MinWidth = 104;
            primary.MinHeight = 44;
            primary.IsDefault = true;
            primary.Click += async delegate { if (completed) Finish(); else await Install(); };
            actions.Children.Add(primary);
            Closing += OnClosing;
            PreviewKeyDown += delegate(object sender, KeyEventArgs e) { if (e.Key == Key.Escape) { e.Handled = true; Close(); } };
            Loaded += async delegate { if (options.Passive) await Install(); else primary.Focus(); };
        }

        private string T(string zh, string en) { return chinese ? zh : en; }
        private Brush Brush(string key) { return (Brush)Resources[key]; }
        private Brush BackgroundBrush()
        {
            using (Stream stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("AgentSwitchboard.Installer.Assets.InstallerBackground.wdp"))
            {
                if (stream == null) return Brush("Surface");
                var decoder = new WmpBitmapDecoder(stream, BitmapCreateOptions.None, BitmapCacheOption.OnLoad);
                if (decoder.Frames.Count == 0) return Brush("Surface");
                var image = new ImageBrush(decoder.Frames[0]) { Stretch = Stretch.Fill };
                image.Freeze();
                return image;
            }
        }
        private TextBlock Text(string value, string size) { return new TextBlock { Text = value, FontSize = (double)Resources[size] }; }
        private Button MakeButton(string label, bool main) { return new Button { Content = label, Style = (Style)Resources[main ? (object)"Primary" : typeof(Button)] }; }

        private FrameworkElement CreateBrandMark(double size)
        {
            var artwork = new Grid { Width = 48, Height = 48, SnapsToDevicePixels = true };
            artwork.Children.Add(new ShapePath { Data = Geometry.Parse("M 23,0 C 9,4 1,14 1,24 C 1,34 9,44 23,48 Z"), Fill = Brush("Action") });
            artwork.Children.Add(new ShapePath { Data = Geometry.Parse("M 26,0 C 40,4 48,14 48,24 C 48,34 40,44 26,48 Z"), Fill = Brush("Violet") });
            return new Viewbox { Width = size, Height = size, Stretch = Stretch.Uniform, Child = artwork };
        }

        private Button MakeTitlebarButton(string glyph, bool isClose)
        {
            var icon = Text(glyph, "BodySize");
            icon.FontFamily = new FontFamily("Segoe UI Symbol");
            icon.FontSize = 16;
            icon.FontWeight = FontWeights.SemiBold;
            icon.HorizontalAlignment = HorizontalAlignment.Center;
            icon.VerticalAlignment = VerticalAlignment.Center;
            icon.TextAlignment = TextAlignment.Center;
            icon.SetBinding(TextBlock.ForegroundProperty, new Binding("Foreground")
            {
                RelativeSource = new RelativeSource(RelativeSourceMode.FindAncestor, typeof(Button), 1)
            });
            return new Button
            {
                Content = icon,
                Style = (Style)Resources[isClose ? "TitlebarCloseButton" : "TitlebarButton"]
            };
        }

        [DllImport("dwmapi.dll", PreserveSig = true)]
        private static extern int DwmSetWindowAttribute(IntPtr window, int attribute, ref int value, int size);

        private void ApplySystemMaterial()
        {
            var handle = new WindowInteropHelper(this).Handle;
            // DWM owns the outer contour; the WPF surface fills the client area without another frame.
            int round = 2;
            DwmSetWindowAttribute(handle, 33, ref round, sizeof(int));
            int noBorder = unchecked((int)0xFFFFFFFE);
            DwmSetWindowAttribute(handle, 34, ref noBorder, sizeof(int));
        }

        private void UpdateExisting()
        {
            existing.Text = InstallerEngine.IsInstalledDirectory(directory.Text)
                ? T("此位置已安装应用，将更新程序并保留应用数据。", "The application is installed here. Setup will update it and keep application data.")
                : T("仅为当前 Windows 用户安装。", "Install for the current Windows user only.");
        }

        private void Browse(object sender, RoutedEventArgs e)
        {
            using (var dialog = new System.Windows.Forms.FolderBrowserDialog())
            {
                dialog.Description = T("选择安装位置", "Choose install location");
                dialog.SelectedPath = directory.Text;
                if (dialog.ShowDialog() == System.Windows.Forms.DialogResult.OK) directory.Text = dialog.SelectedPath;
            }
        }

        private async System.Threading.Tasks.Task Install()
        {
            if (running) return;
            running = true;
            ExitCode = 1;
            directory.IsEnabled = browse.IsEnabled = primary.IsEnabled = cancel.IsEnabled = close.IsEnabled = false;
            progress.Visibility = Visibility.Visible;
            progress.IsIndeterminate = SystemParameters.ClientAreaAnimation;
            status.Visibility = Visibility.Visible;
            status.Foreground = Brush("Muted");
            status.Text = T("正在安装，请保持此窗口打开。", "Installing. Keep this window open until setup finishes.");
            try
            {
                options.Directory = directory.Text;
                var result = await InstallerEngine.Run(options);
                ExitCode = result.ExitCode;
                if (ExitCode != 0) throw new InvalidOperationException(T("安装程序退出代码：", "Installer exit code: ") + ExitCode);
                completed = true;
                title.Text = T("安装完成", "Installation complete");
                status.Text = T("应用已准备就绪。", "The application is ready.");
                restartFailed = result.LaunchError != null;
                if (restartFailed) status.Text = T("已安装，但自动启动失败：", "Installed, but automatic launch failed: ") + result.LaunchError;
                primary.Content = T("完成", "Finish");
                cancel.Visibility = Visibility.Collapsed;
                launch.Visibility = options.Restart && !restartFailed ? Visibility.Collapsed : Visibility.Visible;
            }
            catch (Exception error)
            {
                if (ExitCode == 0) ExitCode = 1;
                status.Foreground = Brush("Error");
                status.Text = T("安装未完成。", "Installation did not complete. ") + error.Message;
                primary.Content = T("重试", "Retry");
                directory.IsEnabled = browse.IsEnabled = true;
            }
            finally
            {
                running = false;
                progress.IsIndeterminate = false;
                progress.Visibility = Visibility.Collapsed;
                primary.IsEnabled = cancel.IsEnabled = close.IsEnabled = true;
                primary.Focus();
            }
            if (options.Passive && completed && !restartFailed) Close();
        }

        private void Finish()
        {
            try { if (launch.IsChecked == true && (!options.Restart || restartFailed)) InstallerEngine.Launch(options.Directory); }
            catch (Exception error) { status.Foreground = Brush("Error"); status.Text = T("已安装，但启动失败：", "Installed, but could not launch: ") + error.Message; launch.IsChecked = false; return; }
            Close();
        }

        private void OnClosing(object sender, CancelEventArgs e)
        {
            if (!running) return;
            e.Cancel = true;
            status.Text = T("正在安装，完成后即可关闭。", "Installation is running. You can close this window when it finishes.");
        }
    }
}
