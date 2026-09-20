using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Shell;
using ShapePath = System.Windows.Shapes.Path;

namespace AgentSwitchboard.Installer
{
    internal sealed partial class InstallerWindow
    {
        private Grid BuildLayout()
        {
            var layout = new Grid { Margin = new Thickness(32, 8, 32, 28) };
            layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            layout.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            layout.Children.Add(BuildHeader());
            var body = BuildBody();
            body.Width = 536;
            var scroll = new ScrollViewer
            {
                Content = body,
                VerticalScrollBarVisibility = ScrollBarVisibility.Hidden,
                HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
            };
            scroll.SizeChanged += delegate { body.Width = Math.Min(536, scroll.ActualWidth); };
            Grid.SetRow(scroll, 1);
            layout.Children.Add(scroll);

            var footer = BuildFooter();
            footer.HorizontalAlignment = HorizontalAlignment.Center;
            footer.SetBinding(FrameworkElement.WidthProperty, new Binding("ActualWidth") { Source = body });
            Grid.SetRow(footer, 2);
            layout.Children.Add(footer);
            return layout;
        }

        private Grid BuildHeader()
        {
            var header = new Grid { Height = 44, Background = Brushes.Transparent };
            header.ColumnDefinitions.Add(new ColumnDefinition());
            header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var brand = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                VerticalAlignment = VerticalAlignment.Center,
            };
            AutomationProperties.SetName(brand, copy.BrandName + " " + copy.BrandContext);
            var brandMark = CreateBrandMark(20);
            brandMark.Margin = new Thickness(0, 0, 9, 0);
            brand.Children.Add(brandMark);

            var brandName = Text(copy.BrandName, "LabelSize");
            brandName.FontWeight = FontWeights.SemiBold;
            brand.Children.Add(brandName);

            var brandContext = Text(copy.BrandContext, "CaptionSize");
            brandContext.Foreground = Brush("Muted");
            brandContext.Margin = new Thickness(8, 1, 0, 0);
            brand.Children.Add(brandContext);
            header.Children.Add(brand);

            var minimize = MakeTitlebarButton("−", false);
            minimize.Name = "MinimizeButton";
            minimize.ToolTip = copy.Minimize;
            AutomationProperties.SetName(minimize, copy.Minimize);
            WindowChrome.SetIsHitTestVisibleInChrome(minimize, true);
            minimize.Click += delegate { WindowState = WindowState.Minimized; };
            Grid.SetColumn(minimize, 1);
            header.Children.Add(minimize);

            close = MakeTitlebarButton("×", true);
            close.ToolTip = copy.Close;
            AutomationProperties.SetName(close, copy.Close);
            WindowChrome.SetIsHitTestVisibleInChrome(close, true);
            close.Click += delegate { Close(); };
            Grid.SetColumn(close, 2);
            header.Children.Add(close);
            return header;
        }

        private FrameworkElement BuildBody()
        {
            var body = new StackPanel
            {
                HorizontalAlignment = HorizontalAlignment.Center,
                MaxWidth = 536,
                Margin = new Thickness(0, 22, 0, 20),
            };
            body.Children.Add(BuildHero());
            body.Children.Add(BuildLocationCard());
            body.Children.Add(BuildProgressArea());
            return body;
        }

        private FrameworkElement BuildHero()
        {
            var hero = new StackPanel
            {
                HorizontalAlignment = HorizontalAlignment.Center,
                Margin = new Thickness(0, 0, 0, 30),
            };
            var mark = CreateBrandMark(54);
            mark.Margin = new Thickness(0, 0, 0, 18);
            hero.Children.Add(mark);

            title = Text(copy.ReadyTitle, "TitleSize");
            title.FontFamily = Font("DisplayFont");
            title.FontWeight = FontWeights.SemiBold;
            title.TextAlignment = TextAlignment.Center;
            title.HorizontalAlignment = HorizontalAlignment.Center;
            hero.Children.Add(title);

            var summary = Text(copy.ReadySummary, "LeadSize");
            summary.Foreground = Brush("Muted");
            summary.LineHeight = 23;
            summary.TextAlignment = TextAlignment.Center;
            summary.MaxWidth = 440;
            summary.Margin = new Thickness(0, 10, 0, 0);
            hero.Children.Add(summary);

            var version = Text(copy.Version(InstallerProductMetadata.Version), "CaptionSize");
            version.Foreground = Brush("Muted");
            version.TextAlignment = TextAlignment.Center;
            version.Margin = new Thickness(0, 10, 0, 0);
            hero.Children.Add(version);
            return hero;
        }

        private FrameworkElement BuildLocationCard()
        {
            var card = new Border { Style = (Style)Resources["SectionCard"] };
            var content = new StackPanel();
            card.Child = content;

            var label = Text(copy.InstallLocation, "BodySize");
            label.FontWeight = FontWeights.SemiBold;
            content.Children.Add(label);

            var pathRow = new Grid { Margin = new Thickness(0, 12, 0, 0) };
            pathRow.ColumnDefinitions.Add(new ColumnDefinition());
            pathRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            directory = new TextBox { Text = InstallerEngine.DetectDirectory() };
            AutomationProperties.SetName(directory, copy.InstallLocation);
            pathRow.Children.Add(directory);

            browse = MakeButton(copy.ChooseLocation, false);
            browse.Margin = new Thickness(8, 0, 0, 0);
            browse.Click += Browse;
            Grid.SetColumn(browse, 1);
            pathRow.Children.Add(browse);
            content.Children.Add(pathRow);

            existing = Text(String.Empty, "CaptionSize");
            existing.Foreground = Brush("Muted");
            existing.Margin = new Thickness(0, 12, 0, 0);
            content.Children.Add(existing);
            directory.TextChanged += delegate { UpdateExisting(); };
            UpdateExisting();

            // Two read-only flows share this card: the updater invocation
            // (it reinstalls over the detected location) and the MSI engine
            // (WiX owns the per-machine directory).
            if (invocation.Kind == InstallerInvocationKind.Updater
                || InstallerProductMetadata.UsesMsiEngine)
            {
                directory.IsEnabled = false;
                browse.Visibility = Visibility.Collapsed;
            }
            return card;
        }

        private FrameworkElement BuildProgressArea()
        {
            var content = new StackPanel { Margin = new Thickness(0, 18, 0, 0) };
            progress = new ProgressBar { Visibility = Visibility.Collapsed };
            content.Children.Add(progress);

            status = Text(String.Empty, "BodySize");
            status.Visibility = Visibility.Collapsed;
            status.Margin = new Thickness(0, 13, 0, 0);
            AutomationProperties.SetLiveSetting(status, AutomationLiveSetting.Polite);
            content.Children.Add(status);

            diagnostic = Text(String.Empty, "CaptionSize");
            diagnostic.Foreground = Brush("Muted");
            diagnostic.Visibility = Visibility.Collapsed;
            diagnostic.Margin = new Thickness(0, 6, 0, 0);
            content.Children.Add(diagnostic);

            launch = new CheckBox
            {
                Content = copy.LaunchWhenFinished,
                IsChecked = true,
                Visibility = Visibility.Collapsed,
                Margin = new Thickness(0, 10, 0, 0),
            };
            content.Children.Add(launch);
            return content;
        }

        private FrameworkElement BuildFooter()
        {
            var footer = new Grid { Margin = new Thickness(0, 16, 0, 0) };
            footer.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            footer.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            footer.Children.Add(new Border { Height = 1, Background = Brush("Line") });

            var actions = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                HorizontalAlignment = HorizontalAlignment.Right,
                Margin = new Thickness(0, 16, 0, 0),
            };
            Grid.SetRow(actions, 1);
            footer.Children.Add(actions);

            cancel = MakeButton(copy.Cancel, false);
            cancel.Click += delegate { Close(); };
            actions.Children.Add(cancel);

            primary = MakeButton(copy.Install, true);
            primary.Margin = new Thickness(10, 0, 0, 0);
            primary.MinWidth = 104;
            primary.IsDefault = true;
            primary.Click += OnPrimaryClick;
            actions.Children.Add(primary);
            return footer;
        }

        private Brush BackgroundBrush()
        {
            try
            {
                using (Stream stream = Assembly.GetExecutingAssembly().GetManifestResourceStream(
                    "AgentSwitchboard.Installer.Assets.InstallerBackground.wdp"))
                {
                    if (stream == null) return Brush("Surface");
                    var decoder = new WmpBitmapDecoder(
                        stream,
                        BitmapCreateOptions.None,
                        BitmapCacheOption.OnLoad);
                    if (decoder.Frames.Count == 0) return Brush("Surface");
                    var image = new ImageBrush(decoder.Frames[0]) { Stretch = Stretch.Fill };
                    image.Freeze();
                    return image;
                }
            }
            catch (Exception error)
            {
                Trace.TraceWarning("Installer background: " + error.Message);
                return Brush("Surface");
            }
        }

        /// The brand mark mirrors the two crossing switch paths of
        /// src/assets/app-icon.svg; keep this geometry in sync with that
        /// master when the icon changes.
        private FrameworkElement CreateBrandMark(double size)
        {
            var artwork = new Grid { Width = 512, Height = 512, SnapsToDevicePixels = true };
            artwork.Children.Add(SwitchPath(
                "M 120,168 C 202,168 204,256 256,256 C 308,256 310,344 392,344",
                Brush("BrandBlue")));
            artwork.Children.Add(SwitchPath(
                "M 120,344 C 202,344 204,256 256,256 C 308,256 310,168 392,168",
                Brush("Violet")));
            return new Viewbox { Width = size, Height = size, Stretch = Stretch.Uniform, Child = artwork };
        }

        private static ShapePath SwitchPath(string data, Brush stroke)
        {
            return new ShapePath
            {
                Data = Geometry.Parse(data),
                Stroke = stroke,
                StrokeThickness = 56,
                StrokeStartLineCap = PenLineCap.Round,
                StrokeEndLineCap = PenLineCap.Round,
                StrokeLineJoin = PenLineJoin.Round,
            };
        }

        private Button MakeButton(string label, bool primaryButton)
        {
            return new Button
            {
                Content = label,
                Style = (Style)Resources[primaryButton ? (object)"Primary" : typeof(Button)],
            };
        }

        private Button MakeTitlebarButton(string glyph, bool closeButton)
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
                RelativeSource = new RelativeSource(RelativeSourceMode.FindAncestor, typeof(Button), 1),
            });
            return new Button
            {
                Content = icon,
                Style = (Style)Resources[closeButton ? "TitlebarCloseButton" : "TitlebarButton"],
            };
        }

        private TextBlock Text(string value, string size)
        {
            return new TextBlock { Text = value, FontSize = (double)Resources[size] };
        }

        private Brush Brush(string key)
        {
            return (Brush)Resources[key];
        }

        private FontFamily Font(string key)
        {
            return (FontFamily)Resources[key];
        }
    }
}
