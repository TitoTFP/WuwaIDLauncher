using System.Net;
using System.Security.Cryptography;
using System.Text;

namespace WuwaIDLauncher;

// ponytail: minimal fake-GitHub HTTP server, hosted inside the process under test.
// The tamper flags swap the manifest's SHA-256 to wrong values, which is exactly
// how the checksum-rejection paths get exercised end to end.
internal sealed class E2eStubServer : IDisposable
{
    const string ManifestName = "SHA256sums.txt";
    readonly HttpListener _listener = new();
    readonly CancellationTokenSource _cts = new();
    readonly Dictionary<string, byte[]> _assets = new(StringComparer.OrdinalIgnoreCase);
    readonly Dictionary<string, string> _checksums = new(StringComparer.OrdinalIgnoreCase);
    volatile bool _tamperPak;
    volatile bool _tamperZip;

    internal string BaseUrl { get; }

    internal bool TamperPak { set => _tamperPak = value; }
    internal bool TamperZip { set => _tamperZip = value; }

    internal E2eStubServer()
    {
        BaseUrl = $"http://127.0.0.1:{ReservePort()}/";
        _listener.Prefixes.Add(BaseUrl);
        _listener.Start();
        _ = Task.Run(ListenLoop);
    }

    internal void AddAsset(string name, byte[] bytes)
    {
        _assets[name] = bytes;
        _checksums[name] = Convert.ToHexString(SHA256.HashData(bytes));
    }

    internal string ShaOf(string name) => _checksums[name];

    static int ReservePort()
    {
        using var probe = new System.Net.Sockets.TcpListener(IPAddress.Loopback, 0);
        probe.Start();
        var port = ((IPEndPoint)probe.LocalEndpoint).Port;
        probe.Stop();
        return port;
    }

    async Task ListenLoop()
    {
        while (!_cts.IsCancellationRequested)
        {
            HttpListenerContext ctx;
            try { ctx = await _listener.GetContextAsync(); }
            catch { break; }

            _ = Task.Run(() => Respond(ctx));
        }
    }

    void Respond(HttpListenerContext ctx)
    {
        try
        {
            var name = Uri.UnescapeDataString(ctx.Request.Url!.AbsolutePath.TrimStart('/'));
            byte[] body;
            var contentType = "application/octet-stream";

            if (name == ManifestName)
            {
                var sb = new StringBuilder();
                foreach (var (assetName, hex) in _checksums)
                {
                    var bad = (assetName == Helpers.PakFileName && _tamperPak) ||
                              (assetName.EndsWith(".zip", StringComparison.OrdinalIgnoreCase) && _tamperZip);
                    sb.Append(bad ? WrongSha(hex) : hex).Append("  ").Append(assetName).Append('\n');
                }
                body = Encoding.UTF8.GetBytes(sb.ToString());
                contentType = "text/plain";
            }
            else if (_assets.TryGetValue(name, out var data))
            {
                body = data;
            }
            else
            {
                ctx.Response.StatusCode = 404;
                ctx.Response.ContentLength64 = 0;
                ctx.Response.Close();
                return;
            }

            ctx.Response.ContentType = contentType;
            ctx.Response.ContentLength64 = body.Length;
            if (ctx.Request.HttpMethod == "GET")
                ctx.Response.OutputStream.Write(body, 0, body.Length);
            ctx.Response.Close();
        }
        catch
        {
            try { ctx.Response.Abort(); } catch { }
        }
    }

    static string WrongSha(string hex) =>
        (hex[0] == '0' ? "1" : "0") + hex[1..];

    public void Dispose()
    {
        _cts.Cancel();
        try { _listener.Stop(); } catch { }
    }
}
