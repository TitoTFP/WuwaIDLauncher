using System.Security.Cryptography;
using System.Text;
using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public sealed class ResourceMountInstallerTests : IDisposable
{
    readonly string _tempDir = Path.Combine(Path.GetTempPath(), "resource_mount_test_" + Guid.NewGuid().ToString("N"));
    readonly string _gamePath;

    public ResourceMountInstallerTests()
    {
        _gamePath = Path.Combine(_tempDir, "Game");
        Directory.CreateDirectory(Helpers.GameBinaryFolderPath(_gamePath));
    }

    [Fact]
    public void Probe_SelectsHighestReadySemanticVersion()
    {
        CreateReadyVersion("3.9.0");
        var expected = CreateReadyVersion("3.10.0");
        Directory.CreateDirectory(Path.Combine(ResourceMountInstaller.ResourcesRootPath(_gamePath), "4.0.0", "Mount"));
        Directory.CreateDirectory(Path.Combine(ResourceMountInstaller.ResourcesRootPath(_gamePath), "Video"));

        var plan = ResourceMountInstaller.Probe(_gamePath);

        plan.ResourceVersion.Should().Be("3.10.0");
        plan.VersionDirectory.Should().Be(expected);
        plan.SourceSignaturePath.Should().EndWith(Path.Combine("Lang_en", "Base", "official.sig"));
    }

    [Fact]
    public void ExpectedPatchAssets_Method3UsesResolvedMountPak()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);

        var assets = MainWindow.ExpectedPatchAssets(
            _gamePath, InstallMethods.Method3, new Dictionary<string, string>(), useCachedFingerprint: false);

        assets.Should().ContainSingle(asset =>
            asset.Name == Helpers.PakFileName && asset.Path == plan.PakPath);
    }

    [Fact]
    public void Install_WritesVerifiedPakSignatureAndMount()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        var sourcePak = WriteSourcePak("translated patch");
        var hash = Sha256(sourcePak);

        ResourceMountInstaller.Install(plan, sourcePak, hash);

        File.ReadAllText(plan.PakPath).Should().Be("translated patch");
        File.ReadAllText(plan.SignaturePath).Should().Be("official signature");
        File.ReadAllText(plan.MountPath).Should().Be(ResourceMountInstaller.MountContent(
            Sha1(plan.PakPath), Sha1(plan.SignaturePath)));
        ResourceMountInstaller.IsHealthy(plan).Should().BeTrue();
        ResourceMountInstaller.IsManaged(plan).Should().BeTrue();
        File.Exists(plan.OwnerMarkerPath).Should().BeTrue();
        File.Exists(plan.StagedPakPath).Should().BeFalse();
        File.Exists(plan.PakPath + ".bak").Should().BeFalse();
    }

    [Fact]
    public void IsManaged_RejectsInstalledSignatureThatDiffersFromOfficialSource()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        var sourcePak = WriteSourcePak("translated patch");
        ResourceMountInstaller.Install(plan, sourcePak, Sha256(sourcePak));
        File.WriteAllText(plan.SignaturePath, "tampered signature");
        File.WriteAllText(plan.MountPath, ResourceMountInstaller.MountContent(Sha1(plan.PakPath), Sha1(plan.SignaturePath)));

        ResourceMountInstaller.IsHealthy(plan).Should().BeTrue();
        ResourceMountInstaller.IsManaged(plan).Should().BeFalse();
    }

    [Fact]
    public void Install_HashMismatch_PreservesExistingOwnedPatch()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(plan.PakPath)!);
        File.WriteAllText(plan.PakPath, "old pak");
        File.WriteAllText(plan.SignaturePath, "old signature");
        File.WriteAllText(plan.MountPath, ResourceMountInstaller.MountContent(Sha1(plan.PakPath), Sha1(plan.SignaturePath)));
        var sourcePak = WriteSourcePak("new pak");

        FluentActions.Invoking(() => ResourceMountInstaller.Install(plan, sourcePak, new string('0', 64)))
            .Should().Throw<InvalidDataException>();

        File.ReadAllText(plan.PakPath).Should().Be("old pak");
        File.ReadAllText(plan.SignaturePath).Should().Be("old signature");
    }

    [Fact]
    public void Install_RefusesForeignArtifactsWithoutChangingThem()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(plan.PakPath)!);
        File.WriteAllText(plan.PakPath, "foreign pak");
        File.WriteAllText(plan.SignaturePath, "foreign signature");
        File.WriteAllText(plan.MountPath, ResourceMountInstaller.MountContent(Sha1(plan.PakPath), Sha1(plan.SignaturePath)));
        var sourcePak = WriteSourcePak("foreign pak");

        FluentActions.Invoking(() => ResourceMountInstaller.Install(plan, sourcePak, Sha256(sourcePak)))
            .Should().Throw<InvalidDataException>();

        File.ReadAllText(plan.PakPath).Should().Be("foreign pak");
        File.ReadAllText(plan.SignaturePath).Should().Be("foreign signature");
        File.ReadAllText(plan.MountPath).Should().Be(ResourceMountInstaller.MountContent(Sha1(plan.PakPath), Sha1(plan.SignaturePath)));
    }

    [Fact]
    public void Install_RecoversOwnedPartialCommitBeforeUpdating()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        var oldPak = WriteSourcePak("old pak");
        ResourceMountInstaller.Install(plan, oldPak, Sha256(oldPak));
        File.Copy(plan.PakPath, plan.PakPath + ".bak");
        File.Copy(plan.SignaturePath, plan.SignaturePath + ".bak");
        File.WriteAllText(plan.PakPath, "interrupted pak");
        File.WriteAllText(plan.SignaturePath, "interrupted signature");
        var newPak = WriteSourcePak("new pak");

        ResourceMountInstaller.Install(plan, newPak, Sha256(newPak));

        ResourceMountInstaller.IsHealthy(plan).Should().BeTrue();
        File.ReadAllText(plan.PakPath).Should().Be("new pak");
        File.Exists(plan.PakPath + ".bak").Should().BeFalse();
        File.Exists(plan.SignaturePath + ".bak").Should().BeFalse();
    }

    [Fact]
    public void Install_RecoversInterruptedFirstInstallAfterOwnerMarker()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        var sourcePak = WriteSourcePak("new pak");
        Directory.CreateDirectory(Path.GetDirectoryName(plan.PakPath)!);
        File.WriteAllText(plan.OwnerMarkerPath, OwnerMarker(Sha256(sourcePak)));
        File.WriteAllText(plan.PakPath, "partial pak");

        ResourceMountInstaller.Install(plan, sourcePak, Sha256(sourcePak));

        ResourceMountInstaller.IsManaged(plan).Should().BeTrue();
        File.ReadAllText(plan.PakPath).Should().Be("new pak");
    }

    [Fact]
    public void RemoveAllOwnedArtifacts_CleansInterruptedFirstInstall()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(plan.PakPath)!);
        File.WriteAllText(plan.OwnerMarkerPath, OwnerMarker(new string('a', 64)));
        File.WriteAllText(plan.PakPath, "partial pak");

        ResourceMountInstaller.RemoveAllOwnedArtifacts(_gamePath);

        File.Exists(plan.OwnerMarkerPath).Should().BeFalse();
        File.Exists(plan.PakPath).Should().BeFalse();
    }

    [Fact]
    public void Probe_BlocksHighPriorityConflictingMount()
    {
        var version = CreateReadyVersion("3.5.0");
        File.WriteAllText(Path.Combine(version, "Mount", "other-mod.txt"), "::Mount::\nother/mod,99,AA,BB,,\n::Del::\n");

        var plan = ResourceMountInstaller.Probe(_gamePath);

        plan.Conflicts.Should().Contain(conflict => conflict.Contains("prioritas tinggi", StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public void RemoveAllOwnedArtifacts_PreservesForeignMatchingArtifacts()
    {
        CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(plan.PakPath)!);
        File.WriteAllText(plan.PakPath, "foreign pak");
        File.WriteAllText(plan.SignaturePath, "foreign signature");
        File.WriteAllText(plan.MountPath, ResourceMountInstaller.MountContent(Sha1(plan.PakPath), Sha1(plan.SignaturePath)));

        ResourceMountInstaller.RemoveAllOwnedArtifacts(_gamePath);

        File.Exists(plan.PakPath).Should().BeTrue();
        File.Exists(plan.SignaturePath).Should().BeTrue();
        File.Exists(plan.MountPath).Should().BeTrue();
    }

    [Fact]
    public void RemoveAllOwnedArtifacts_PreservesOfficialMountFiles()
    {
        var version = CreateReadyVersion("3.5.0");
        var plan = ResourceMountInstaller.Probe(_gamePath);
        var sourcePak = WriteSourcePak("pak");
        ResourceMountInstaller.Install(plan, sourcePak, Sha256(sourcePak));
        var officialMount = Path.Combine(version, "Mount", "MountResource.txt");
        File.WriteAllText(officialMount, "::Mount::\n::Del::\n");

        ResourceMountInstaller.RemoveAllOwnedArtifacts(_gamePath);

        File.Exists(plan.PakPath).Should().BeFalse();
        File.Exists(plan.SignaturePath).Should().BeFalse();
        File.Exists(plan.MountPath).Should().BeFalse();
        File.Exists(officialMount).Should().BeTrue();
    }

    string CreateReadyVersion(string version)
    {
        var root = Path.Combine(ResourceMountInstaller.ResourcesRootPath(_gamePath), version);
        Directory.CreateDirectory(Path.Combine(root, "ResManifest"));
        Directory.CreateDirectory(Path.Combine(root, "Mount"));
        var signatureDirectory = Path.Combine(root, "Lang_en", "Base");
        Directory.CreateDirectory(signatureDirectory);
        var signaturePath = Path.Combine(signatureDirectory, "official.sig");
        var pakPath = Path.Combine(signatureDirectory, "official.pak");
        File.WriteAllText(signaturePath, "official signature");
        File.WriteAllText(pakPath, "official pak");
        File.WriteAllText(Path.Combine(root, "Mount", "MountLang_en.txt"),
            $"::Mount::\nLang_en/Base/official,4,{Sha1(pakPath)},{Sha1(signaturePath)},,\n::Del::\n");
        return root;
    }

    string WriteSourcePak(string content)
    {
        var path = Path.Combine(_tempDir, "source.pak");
        File.WriteAllText(path, content);
        return path;
    }

    static string Sha256(string path) => Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path)));

    static string Sha1(string path) => Convert.ToHexString(SHA1.HashData(File.ReadAllBytes(path)));

    static string OwnerMarker(string sha256) => $"WuwaID Resource Mount v1\nsha256={sha256.ToLowerInvariant()}\n";

    public void Dispose()
    {
        if (Directory.Exists(_tempDir))
            Directory.Delete(_tempDir, true);
    }
}
