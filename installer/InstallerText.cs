using System;
using System.Globalization;

namespace AgentSwitchboard.Installer
{
    internal sealed class InstallerText
    {
        private readonly bool chinese = CultureInfo.CurrentUICulture.TwoLetterISOLanguageName == "zh";

        internal string WindowTitle
        {
            get { return InstallerProductMetadata.ProductName; }
        }

        internal string BrandName
        {
            get { return InstallerProductMetadata.ProductName; }
        }

        internal string BrandContext
        {
            get { return T("安装程序", "Installer"); }
        }

        internal string ReadyTitle
        {
            get { return T("准备安装", "Ready to install"); }
        }

        internal string ReadySummary
        {
            get { return T("在本机统一管理 Codex 与 Claude Code 的供应商和配置。", "Manage Codex and Claude Code providers and configuration on this PC."); }
        }

        internal string InstallLocation
        {
            get { return T("安装位置", "Install location"); }
        }

        internal string ChooseLocation
        {
            get { return T("选择…", "Choose…"); }
        }

        internal string NewInstallationNotice
        {
            get
            {
                return InstallerProductMetadata.UsesMsiEngine
                    ? T("将为这台电脑的所有用户安装；安装时 Windows 将请求管理员权限。", "For all users of this PC. Windows will ask for administrator approval.")
                    : T("仅为当前 Windows 帐户安装。", "Available to this Windows account only.");
            }
        }

        internal string ExistingInstallationNotice
        {
            get
            {
                return InstallerProductMetadata.UsesMsiEngine
                    ? T("将更新这台电脑上的 Agent Switchboard，并保留应用数据。", "This PC's copy will be updated and its application data will be kept.")
                    : T("将更新此位置中的 Agent Switchboard，并保留应用数据。", "This copy will be updated and its application data will be kept.");
            }
        }

        internal string Cancel
        {
            get { return T("取消", "Cancel"); }
        }

        internal string Install
        {
            get { return T("安装", "Install"); }
        }

        internal string Update
        {
            get { return T("更新", "Update"); }
        }

        internal string Retry
        {
            get { return T("再试一次", "Try again"); }
        }

        internal string Finish
        {
            get { return T("完成", "Done"); }
        }

        internal string InstallingTitle
        {
            get { return T("正在安装", "Installing"); }
        }

        internal string InstallingStatus
        {
            get { return T("正在安装 Agent Switchboard。请保持此窗口打开。", "Installing Agent Switchboard. Keep this window open."); }
        }

        internal string CompleteTitle
        {
            get { return T("安装完成", "Installation complete"); }
        }

        internal string CompleteStatus
        {
            get { return T("Agent Switchboard 已准备就绪。", "Agent Switchboard is ready to use."); }
        }

        internal string LaunchWhenFinished
        {
            get { return T("现在打开 Agent Switchboard", "Open Agent Switchboard now"); }
        }

        internal string InstallingCannotClose
        {
            get { return T("正在安装。完成后即可关闭此窗口。", "Installation is in progress. You can close this window when it finishes."); }
        }

        internal string Minimize
        {
            get { return T("最小化", "Minimize"); }
        }

        internal string Close
        {
            get { return T("关闭", "Close"); }
        }

        internal string ChooseInstallLocation
        {
            get { return T("选择安装位置", "Choose install location"); }
        }

        internal string Version(string version)
        {
            return T("版本 ", "Version ") + version;
        }

        internal string FailureTitle(Exception error)
        {
            var installerError = error as InstallerException;
            if (installerError == null) return T("安装未完成", "Installation did not complete");

            switch (installerError.Kind)
            {
                case InstallerFailureKind.TemporaryWorkspace:
                    return T("无法准备安装空间", "Unable to prepare setup");
                case InstallerFailureKind.InvalidDirectory:
                    return T("选择另一个位置", "Choose another location");
                case InstallerFailureKind.WebViewRuntime:
                    return T("无法准备安装环境", "Unable to prepare setup");
                case InstallerFailureKind.ElevationDeclined:
                    return T("已取消安装", "Installation cancelled");
                default:
                    return T("安装未完成", "Installation did not complete");
            }
        }

        internal string FailureStatus(Exception error)
        {
            var installerError = error as InstallerException;
            if (installerError == null)
                return T("请再试一次；如果问题仍然存在，请重新下载安装包。", "Try again. If the issue continues, download the installer again.");

            switch (installerError.Kind)
            {
                case InstallerFailureKind.TemporaryWorkspace:
                    return T("请确认临时文件夹和磁盘空间可用后再试。", "Make sure the temporary folder and disk space are available, then try again.");
                case InstallerFailureKind.InvalidDirectory:
                    return T("请选择可写入的应用文件夹，而不是驱动器根目录。", "Choose a writable application folder, not a drive root.");
                case InstallerFailureKind.WebViewRuntime:
                    return T("请检查网络连接，或先安装 Microsoft Edge WebView2 Runtime 后再试。", "Check the network connection, or install Microsoft Edge WebView2 Runtime and try again.");
                case InstallerFailureKind.EngineLaunch:
                    return T("无法启动安装引擎。请重新下载安装包后再试。", "The installation engine could not start. Download the installer again and try again.");
                case InstallerFailureKind.EngineExecution:
                    return T("请确认目标文件夹未被占用，并且当前帐户可以写入。", "Make sure the destination is not in use and this account can write to it.");
                case InstallerFailureKind.ElevationDeclined:
                    return T("这台电脑的安装需要管理员权限。请在权限请求中选择“是”后重试。", "Installing on this PC requires administrator approval. Accept the prompt and try again.");
                default:
                    return T("请再试一次。", "Try again.");
            }
        }

        internal string FailureDiagnostic(Exception error)
        {
            var installerError = error as InstallerException;
            if (installerError == null || !installerError.ExitCode.HasValue) return String.Empty;
            return T("安装引擎返回代码：", "Installation engine exit code: ") + installerError.ExitCode.Value;
        }

        internal string LaunchFailureStatus
        {
            get { return T("已完成安装，但无法自动打开应用。", "Installed, but the app could not open automatically."); }
        }

        internal string LaunchFailureDiagnostic(string detail)
        {
            return T("详细信息：", "Details: ") + detail;
        }

        internal string StartupFailure(Exception error)
        {
            if (error is ArgumentException)
                return T("安装程序参数无效。请直接运行安装程序，或从 Agent Switchboard 中启动更新。", "The installer arguments are not valid. Run the installer directly, or start the update from Agent Switchboard.");
            return T("安装程序无法启动。请重新下载安装包后再试。", "The installer could not start. Download the installer again and try again.");
        }

        private string T(string zh, string en)
        {
            return chinese ? zh : en;
        }
    }
}
