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
    internal static readonly Stopwatch StartupClock = Stopwatch.StartNew();
    static Mutex? _mutex;

    [DllImport("kernel32.dll")]
    static extern bool IsDebuggerPresent();
    [DllImport("kernel32.dll")]
    static extern bool CheckRemoteDebuggerPresent(nint hProcess, out bool isDebuggerPresent);
    [DllImport("kernel32.dll")]
    static extern nint GetCurrentProcess();

    protected override void OnStartup(StartupEventArgs e)
    {
        AppLogger.Initialize(WuwaIDLauncher.MainWindow.AppDataFolder);
        AppLogger.Info("Startup milestone: process_start elapsed_ms=0");
        
        if (Debugger.IsAttached || IsDebuggerPresent())
        {
            AppLogger.Warn("Debugger detected at startup");
            Shutdown();
            return;
        }
        CheckRemoteDebuggerPresent(GetCurrentProcess(), out var remote);
        if (remote)
        {
            AppLogger.Warn("Remote debugger detected at startup");
            Shutdown();
            return;
        }

        _mutex = new Mutex(true, "WuwaIDLauncher_SingleInstance", out bool isNew);
        if (!isNew)
        {
            AppLogger.Warn("Second launcher instance blocked");
            Shutdown();
            return;
        }

        base.OnStartup(e);

        DispatcherUnhandledException += (_, args) =>
        {
            AppLogger.Exception(args.Exception, "Dispatcher unhandled exception");
            KillWebView2Tree();
            args.Handled = false;
        };
        AppDomain.CurrentDomain.UnhandledException += (_, args) =>
        {
            if (args.ExceptionObject is Exception ex)
                AppLogger.Exception(ex, "Unhandled exception");
            else
                AppLogger.Error("Unhandled non-exception object: " + args.ExceptionObject);
            KillWebView2Tree();
        };
        AppDomain.CurrentDomain.ProcessExit += (_, _) =>
        {
            AppLogger.Info("Process exit");
            KillWebView2Tree();
        };

        var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        timer.Tick += (_, _) =>
        {
            CheckRemoteDebuggerPresent(GetCurrentProcess(), out var r);
            if (Debugger.IsAttached || IsDebuggerPresent() || r)
            {
                AppLogger.Warn("Debugger detected after startup");
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
            AppLogger.Exception(ex, "Main window startup failed");
            MessageBox.Show($"Lỗi khởi tạo: {ex.Message}");
            Shutdown(1);
        }
    }

    protected override void OnExit(ExitEventArgs e)
    {
        AppLogger.Info("Application exit");
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
            AppLogger.Info("Killed WebView2 process tree");
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to kill WebView2 process tree");
        }
        WebView2BrowserPid = 0;
    }
}
