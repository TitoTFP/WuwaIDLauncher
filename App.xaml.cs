using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;
using System.Windows.Threading;

namespace WuwaIDLauncher;

public partial class App : Application
{
    internal static uint WebView2BrowserPid;
    static Mutex? _mutex;

    [DllImport("kernel32.dll")]
    static extern bool IsDebuggerPresent();
    [DllImport("kernel32.dll")]
    static extern bool CheckRemoteDebuggerPresent(nint hProcess, out bool isDebuggerPresent);
    [DllImport("kernel32.dll")]
    static extern nint GetCurrentProcess();

    protected override void OnStartup(StartupEventArgs e)
    {
        
        if (Debugger.IsAttached || IsDebuggerPresent())
        { Shutdown(); return; }
        CheckRemoteDebuggerPresent(GetCurrentProcess(), out var remote);
        if (remote) { Shutdown(); return; }

        _mutex = new Mutex(true, "WuwaIDLauncher_SingleInstance", out bool isNew);
        if (!isNew) { Shutdown(); return; }

        base.OnStartup(e);

        DispatcherUnhandledException += (_, args) => { KillWebView2Tree(); args.Handled = false; };
        AppDomain.CurrentDomain.UnhandledException += (_, _) => KillWebView2Tree();
        AppDomain.CurrentDomain.ProcessExit += (_, _) => KillWebView2Tree();

        var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        timer.Tick += (_, _) =>
        {
            CheckRemoteDebuggerPresent(GetCurrentProcess(), out var r);
            if (Debugger.IsAttached || IsDebuggerPresent() || r)
            {
                KillWebView2Tree();
                Environment.Exit(1);
            }
        };
        timer.Start();

        try
        {
            var win = new MainWindow();
            MainWindow = win;
            win.Show();
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Lỗi khởi tạo: {ex.Message}");
            Shutdown(1);
        }
    }

    protected override void OnExit(ExitEventArgs e)
    {
        KillWebView2Tree();
        _mutex?.ReleaseMutex();
        _mutex?.Dispose();
        base.OnExit(e);
    }

    internal static void KillWebView2Tree()
    {
        if (WebView2BrowserPid == 0) return;
        try
        {
            var proc = Process.GetProcessById((int)WebView2BrowserPid);
            proc.Kill(entireProcessTree: true);
        }
        catch { }
        WebView2BrowserPid = 0;
    }
}
