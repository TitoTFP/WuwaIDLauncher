namespace WuwaIDLauncher;

// ponytail: minimal E2E switchboard. The process self-selects headless --e2e mode;
// BaseUrlOverride points every release download at the in-process stub server.
internal static class E2eConfig
{
    internal static bool Enabled =>
        Environment.GetCommandLineArgs().Any(a => string.Equals(a, "--e2e", StringComparison.OrdinalIgnoreCase));

    internal static string? BaseUrlOverride;
}
