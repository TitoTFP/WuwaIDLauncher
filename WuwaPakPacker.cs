using System.IO;
using System.Security.Cryptography;

namespace WuwaIDLauncher;

static class WuwaPakPacker
{
    const uint Magic            = 0x5A6F12E1;
    const uint VersionMajorWuwa = 12;

    const string FontInPakPath   = "Client/Content/Aki/UI/Framework/LGUI/Font/LaguSansBold.ufont";
    const string DefaultMount    = "../../../";


    static void WriteString(BinaryWriter w, string s)
    {
        byte[] b = System.Text.Encoding.UTF8.GetBytes(s);
        w.Write((uint)(b.Length + 1));
        w.Write(b);
        w.Write((byte)0);
    }

    static int StringSize(string s) => 4 + System.Text.Encoding.UTF8.GetByteCount(s) + 1;

    static byte[] Sha1(byte[] data) => SHA1.HashData(data);

    internal static ulong Fnv64Path(string path, ulong seed)
    {
        const ulong Off  = 0xcbf29ce484222325UL;
        const ulong Prime = 0x00000100000001b3UL;
        ulong h = unchecked(Off + seed);
        foreach (char c in path.ToLowerInvariant())
        {
            ushort u = c;
            h ^= (byte)(u & 0xFF); h = unchecked(h * Prime);
            h ^= (byte)(u >> 8);   h = unchecked(h * Prime);
        }
        return h;
    }

    internal static uint ScrambleFlags(uint f)
        => ((f & 0x3fu) << 16)
         | ((f >> 6) & 0xFFFFu)
         | ((f << 6) & (1u << 28))
         | ((f >> 1) & 0x0FC00000u)
         | (f & 0xE0000000u);

    static (string dir, string name)? SplitPath(string path)
    {
        if (path == "/" || path.Length == 0) return null;
        path = path.TrimEnd('/');
        int i = path.LastIndexOf('/');
        return i < 0 ? ("/", path) : (path[..(i + 1)], path[(i + 1)..]);
    }

    static byte[] BuildFdi(IReadOnlyList<string> paths, uint[] offsets)
    {
        var fdi = new SortedDictionary<string, SortedDictionary<string, uint>>(StringComparer.Ordinal);
        for (int i = 0; i < paths.Count; i++)
        {
            string p = paths[i];
            while (SplitPath(p) is var (par, _))
            {
                fdi.TryAdd(par, new SortedDictionary<string, uint>(StringComparer.Ordinal));
                p = par;
                if (p == "/") break;
            }
            if (SplitPath(paths[i]) is var (dir, name))
            {
                fdi.TryAdd(dir, new SortedDictionary<string, uint>(StringComparer.Ordinal));
                fdi[dir][name] = offsets[i];
            }
        }
        using var ms = new MemoryStream();
        using var w  = new BinaryWriter(ms, System.Text.Encoding.UTF8, leaveOpen: true);
        w.Write((uint)fdi.Count);
        foreach (var (dir, files) in fdi)
        {
            WriteString(w, dir);
            w.Write((uint)files.Count);
            foreach (var (file, off) in files) { WriteString(w, file); w.Write(off); }
        }
        w.Flush();
        return ms.ToArray();
    }


    static void Pack(string dest, string mount, ulong seed, IReadOnlyList<(string Path, byte[] Data)> files)
    {
        using var fs = new FileStream(dest, FileMode.Create, FileAccess.Write, FileShare.None);
        using var w  = new BinaryWriter(fs, System.Text.Encoding.UTF8, leaveOpen: true);

        var dataOffsets = new ulong[files.Count];
        for (int i = 0; i < files.Count; i++)
        {
            var data = files[i].Data;
            dataOffsets[i] = (ulong)fs.Position;
            w.Write((ulong)0);
            w.Write((ulong)data.Length);
            w.Write((ulong)data.Length);
            w.Write((uint)0);
            w.Write(Sha1(data));
            w.Write((byte)0);
            w.Write((uint)0);
            w.Write(data);
        }

        ulong indexOffset = (ulong)fs.Position;

        using var encMs = new MemoryStream();
        using var encW  = new BinaryWriter(encMs, System.Text.Encoding.UTF8, leaveOpen: true);
        var encodedOffsets = new uint[files.Count];
        for (int i = 0; i < files.Count; i++)
        {
            encodedOffsets[i] = (uint)encMs.Position;
            ulong sz  = (ulong)files[i].Data.Length;
            ulong off = dataOffsets[i];
            bool s32  = sz  <= uint.MaxValue;
            bool o32  = off <= uint.MaxValue;
            uint flags = 0;
            if (s32) flags |= (1u << 29) | (1u << 30);
            if (o32) flags |= 1u << 31;
            encW.Write(ScrambleFlags(flags));
            encW.Write((byte)0);
            if (s32) encW.Write((uint)sz);  else encW.Write(sz);
            if (o32) encW.Write((uint)off); else encW.Write(off);
        }
        encW.Flush();
        byte[] enc = encMs.ToArray();

        using var phiMs = new MemoryStream();
        using var phiW  = new BinaryWriter(phiMs, System.Text.Encoding.UTF8, leaveOpen: true);
        phiW.Write((uint)files.Count);
        for (int i = 0; i < files.Count; i++)
        {
            phiW.Write(Fnv64Path(files[i].Path, seed));
            phiW.Write(encodedOffsets[i]);
        }
        phiW.Write((uint)0);
        phiW.Flush();
        byte[] phi = phiMs.ToArray();

        byte[] fdi = BuildFdi(files.Select(f => f.Path).ToList(), encodedOffsets);

        ulong bytesBeforePhi =
              (ulong)StringSize(mount)
            + 4 + 8                 
            + 4 + 8 + 8 + 20       
            + 4 + 8 + 8 + 20       
            + 4 + (ulong)enc.Length + 4;

        ulong phiOffset = indexOffset + bytesBeforePhi;
        ulong fdiOffset = phiOffset   + (ulong)phi.Length;

        using var idxMs = new MemoryStream();
        using var idxW  = new BinaryWriter(idxMs, System.Text.Encoding.UTF8, leaveOpen: true);
        WriteString(idxW, mount);
        idxW.Write((uint)files.Count);
        idxW.Write(seed);
        idxW.Write((uint)1); idxW.Write(phiOffset); idxW.Write((ulong)phi.Length); idxW.Write(Sha1(phi));
        idxW.Write((uint)1); idxW.Write(fdiOffset); idxW.Write((ulong)fdi.Length); idxW.Write(Sha1(fdi));
        idxW.Write((uint)enc.Length);
        idxW.Write(enc);
        idxW.Write((uint)0);
        idxW.Flush();
        byte[] indexBuf = idxMs.ToArray();

        w.Write(indexBuf);
        w.Write(phi);
        w.Write(fdi);

        w.Write((ulong)0); w.Write((ulong)0); 
        w.Write((byte)0);                     
        w.Write(Magic);
        w.Write(VersionMajorWuwa);
        w.Write(indexOffset);
        w.Write((ulong)indexBuf.Length);
        w.Write(Sha1(indexBuf));
        w.Write(new byte[32 * 5]);            
        w.Flush();
    }


    public static string PackFont(string modDir, string pakName, byte[] fontData)
    {
        Directory.CreateDirectory(modDir);
        var dest = Path.Combine(modDir, pakName + "_100_P.pak");
        Pack(dest, DefaultMount, 0, [(FontInPakPath, fontData)]);
        return dest;
    }
}
