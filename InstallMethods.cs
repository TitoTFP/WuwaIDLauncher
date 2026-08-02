using System;

namespace WuwaIDLauncher;

internal static class InstallMethods
{
    internal const string Method1 = "method1";
    internal const string Method2 = "method2";
    internal const string Method3 = "method3";

    internal static string Normalize(string? method) =>
        string.Equals(method, Method2, StringComparison.OrdinalIgnoreCase) ? Method2 :
        string.Equals(method, Method3, StringComparison.OrdinalIgnoreCase) ? Method3 :
        Method1;

    internal static bool UsesManualLoader(string? method) => Normalize(method) == Method2;

    internal static bool UsesResourceMount(string? method) => Normalize(method) == Method3;

    internal static bool RequiresSignatureBypass(string? method) => Normalize(method) == Method1;
}
