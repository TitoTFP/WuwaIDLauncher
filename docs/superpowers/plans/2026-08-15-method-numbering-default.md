# Method Numbering and Default Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Resource Mount the user-facing Metode 1 and default, and make the existing signature-bypass flow the user-facing Metode 3 without changing persisted internal method IDs.

**Architecture:** Keep InstallMethods.Method1/Method2/Method3 semantic IDs unchanged so existing settings and versions.json remain compatible. Add one native default constant pointing to method3, use it only for no-selection/default call sites, and swap the web menu's labels/data-method mapping so the UI presents method3 as Metode 1 and method1 as Metode 3.

**Tech Stack:** .NET 8 WPF/C#, xUnit + FluentAssertions, embedded vanilla JavaScript, Bash/ripgrep consistency checks, Node.js static assertions.

---

## Files and responsibilities

- Modify InstallMethods.cs: define the single native default constant while preserving the semantic IDs and normalization behavior.
- Modify MainWindow.xaml.cs: use the new default for stored-setting fallbacks and native bridge method defaults.
- Modify ActivePlayerService.cs: initialize heartbeat state with the new default.
- Modify WuwaIDLauncher.Tests/Helpers/InstallMethodTests.cs: lock the default to Resource Mount.
- Create tests/check_method_mapping.js: assert the user-facing menu mapping and JavaScript defaults without requiring a browser.
- Modify Resources/Web/index.html: swap menu labels/descriptions and internal data-method values.
- Modify Resources/Web/script-core.js, Resources/Web/script-nav.js, and Resources/Web/script-home.js: use the new default and user-facing toast/error names.
- Modify README.md, CONTEXT.md, and E2eRunner.cs: document the new user-facing numbering while calling out internal IDs where needed.

## Task 1: Add and implement the native default contract

**Files:**
- Modify: WuwaIDLauncher.Tests/Helpers/InstallMethodTests.cs
- Modify: InstallMethods.cs
- Modify: MainWindow.xaml.cs:302-316,892,1492
- Modify: ActivePlayerService.cs:16

- [ ] **Step 1: Write the failing unit test**

Add this test after ManualPakFileName_RemainsMethod2LocalName in InstallMethodTests:

~~~
[Fact]
public void DefaultMethod_IsResourceMount()
{
    InstallMethods.Default.Should().Be(InstallMethods.Method3);
    InstallMethods.UsesResourceMount(InstallMethods.Default).Should().BeTrue();
}
~~~

- [ ] **Step 2: Run the focused test and verify RED**

Run:

~~~
dotnet test WuwaIDLauncher.Tests/WuwaIDLauncher.Tests.csproj --filter "FullyQualifiedName~InstallMethodTests.DefaultMethod_IsResourceMount" --no-restore -v minimal
~~~

Expected: compilation fails because InstallMethods.Default does not exist yet.

- [ ] **Step 3: Implement the minimal native default**

Add the compile-time constant below Method3 in InstallMethods.cs:

~~~
internal const string Default = Method3;
~~~

Use InstallMethods.Default only where the code currently means “no saved selection/default”:

~~~
if (!File.Exists(SettingsPath)) return InstallMethods.Default;
...
: InstallMethods.Default;
...
return InstallMethods.Default;
~~~

Change the native bridge defaults to:

~~~
internal async Task RunInstallation(
    string gamePath, string vhMode, bool backup,
    string installMethod = InstallMethods.Default)
~~~

~~~
internal void LaunchGame(
    string gamePath, bool dx11,
    string installMethod = InstallMethods.Default)
~~~

Initialize ActivePlayerService with InstallMethods.Default instead of the literal "method1". Do not change InstallMethods.Normalize; explicit legacy IDs must retain their existing semantics.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the same focused dotnet test command. Expected: the test passes with no failures.

- [ ] **Step 5: Commit the native default change**

~~~
git add InstallMethods.cs MainWindow.xaml.cs ActivePlayerService.cs WuwaIDLauncher.Tests/Helpers/InstallMethodTests.cs
git commit -m "feat: default to resource mount"
~~~

## Task 2: Add and implement the user-facing web mapping

**Files:**
- Create: tests/check_method_mapping.js
- Modify: Resources/Web/index.html:67-78
- Modify: Resources/Web/script-core.js:8
- Modify: Resources/Web/script-nav.js:40-72
- Modify: Resources/Web/script-home.js:120,134,197,233

- [ ] **Step 1: Write the failing mapping check**

Create tests/check_method_mapping.js with this exact content:

~~~js
const assert = require('node:assert/strict');
const fs = require('node:fs');

const read = path => fs.readFileSync(path, 'utf8');
const html = read('Resources/Web/index.html');
const core = read('Resources/Web/script-core.js');
const nav = read('Resources/Web/script-nav.js');
const home = read('Resources/Web/script-home.js');

assert.match(
  html,
  /data-method="method3"[\s\S]*?<span class="method-menu__title">Metode 1<\/span>[\s\S]*?Resource Mount/,
  'Metode 1 must map to the internal Resource Mount ID method3'
);
assert.match(
  html,
  /data-method="method2"[\s\S]*?<span class="method-menu__title">Metode 2<\/span>[\s\S]*?winhttp\.dll loader/,
  'Metode 2 must remain the manual loader'
);
assert.match(
  html,
  /data-method="method1"[\s\S]*?<span class="method-menu__title">Metode 3<\/span>[\s\S]*?Signature bypass/,
  'Metode 3 must map to the internal signature-bypass ID method1'
);
assert.match(core, /installMethod:'method3'/, 'web state must default to method3');
assert.equal((home.match(/\|\| 'method1'/g) || []).length, 0, 'web call sites must not default to method1');
assert.ok((home.match(/\|\| 'method3'/g) || []).length >= 4, 'web call sites must default to method3');
assert.match(nav, /S\.cfg\.installMethod === 'method1'\) methodMessage = 'Metode 3 dipilih\.'/);
assert.match(nav, /resource_unavailable.*Metode 1/);
assert.match(nav, /conflict.*Metode 1/);

console.log('method mapping checks passed');
~~~

- [ ] **Step 2: Run the mapping check and verify RED**

Run:

~~~
node tests/check_method_mapping.js
~~~

Expected: it fails on the current Method 1/Method 3 menu order or current method1 defaults.

- [ ] **Step 3: Implement the minimal web remap**

In Resources/Web/index.html, make the menu order and values exactly:

~~~html
<button class="method-menu__item active" data-method="method3" type="button">
    <span class="method-menu__title">Metode 1</span>
    <span class="method-menu__desc">Resource Mount · tanpa signature bypass</span>
</button>
<button class="method-menu__item" data-method="method2" type="button">
    <span class="method-menu__title">Metode 2</span>
    <span class="method-menu__desc">winhttp.dll loader</span>
</button>
<button class="method-menu__item" data-method="method1" type="button">
    <span class="method-menu__title">Metode 3</span>
    <span class="method-menu__desc">Signature bypass</span>
</button>
~~~

Set S.cfg.installMethod in script-core.js to 'method3'. In script-home.js, change every fallback S.cfg.installMethod || 'method1' to S.cfg.installMethod || 'method3' (installation, status check, launch, and status filtering).

In script-nav.js, keep normalizeInstallMethod unchanged for invalid saved IDs, but update selection messaging and Resource Mount errors:

~~~js
let methodMessage = 'Metode 1 dipilih.';
if (S.cfg.installMethod === 'method2') methodMessage = 'Metode 2 dipilih.';
if (S.cfg.installMethod === 'method1') methodMessage = 'Metode 3 dipilih.';
~~~

Replace both Metode 3 suffixes in the resource_unavailable and conflict messages with Metode 1. Change the admin-check fallback to S.cfg.installMethod || 'method3'.

- [ ] **Step 4: Run the mapping check and verify GREEN**

Run node tests/check_method_mapping.js. Expected: method mapping checks passed and exit code 0.

- [ ] **Step 5: Commit the web mapping change**

~~~
git add Resources/Web/index.html Resources/Web/script-core.js Resources/Web/script-nav.js Resources/Web/script-home.js tests/check_method_mapping.js
git commit -m "feat: remap method menu numbering"
~~~

## Task 3: Update user-facing documentation and E2E wording

**Files:**
- Modify: README.md:36-39,142,183-184
- Modify: CONTEXT.md:7-9
- Modify: E2eRunner.cs:46-97

- [ ] **Step 1: Update README and CONTEXT terminology**

Document the user-facing mapping as:

~~~
Metode 1 (default) — Resource Mount; internal ID method3.
Metode 2 — manual loader winhttp.dll; internal ID method2.
Metode 3 — PAK canonical + signature bypass; internal ID method1.
~~~

Explain in CONTEXT.md that Resource Mount remains internal method3 for cache compatibility but is displayed as Metode 1. Remove the old statement that Method 3 is experimental and the old statement that Method 1 is the signature-bypass default.

- [ ] **Step 2: Clarify E2E comments without changing internal test IDs**

Keep the E2E calls and cache assertions using method1, method2, and method3, because those are persisted semantic IDs. Update comments and scenario labels to state the user-facing name, for example:

~~~csharp
// S1 — user-facing Metode 3 (internal method1): canonical pak.
// S4 — user-facing Metode 1 (internal method3): resource mount.
~~~

Do not rename Helpers.Method1PakPath, Helpers.Method2PakPath, or the internal cache keys; they describe storage semantics, not UI numbering.

- [ ] **Step 3: Check for stale user-facing claims**

Run:

~~~
rg -n "Method 1.*default|Metode 1.*signature|Metode 3.*eksperimental|Metode 3.*Resource Mount" README.md CONTEXT.md Resources/Web E2eRunner.cs
~~~

Expected: no stale user-facing descriptions remain; internal method1 and method3 references are allowed when explicitly identified as internal IDs.

- [ ] **Step 4: Commit documentation updates**

~~~
git add README.md CONTEXT.md E2eRunner.cs
git commit -m "docs: update method numbering"
~~~

## Task 4: Run complete verification

**Files:** No additional source changes expected.

- [ ] **Step 1: Run the focused unit test**

~~~
dotnet test WuwaIDLauncher.Tests/WuwaIDLauncher.Tests.csproj --filter "FullyQualifiedName~InstallMethodTests.DefaultMethod_IsResourceMount" --no-restore -v minimal
~~~

Expected: 1 test passed.

- [ ] **Step 2: Run the web mapping check**

~~~
node tests/check_method_mapping.js
~~~

Expected: method mapping checks passed.

- [ ] **Step 3: Run the full .NET test suite**

~~~
dotnet test WuwaIDLauncher.Tests/WuwaIDLauncher.Tests.csproj --no-restore -c Release --verbosity normal
~~~

Expected: exit code 0 and no failed tests.

- [ ] **Step 4: Run launcher consistency checks**

~~~
bash tests/verify_launcher_consistency.sh
~~~

Expected: launcher consistency checks passed.

- [ ] **Step 5: Inspect the final diff and status**

~~~
git diff HEAD~3..HEAD --stat
git diff HEAD~3..HEAD --check
git status --short --branch
~~~

Expected: only the planned source, web, documentation, and test files are committed; pre-existing untracked .venv/, dist/, src/, and tests_python/ remain untouched.
