import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflowRoot = join(root, ".github", "workflows");

test("workflows use only GitHub-hosted runners", () => {
  const workflowFiles = readdirSync(workflowRoot)
    .filter((file) => file.endsWith(".yml"))
    .map((file) => join(workflowRoot, file));
  const workflows = workflowFiles
    .map((file) => readFileSync(file, "utf8"))
    .join("\n");
  const runners = [...workflows.matchAll(/^\s*runs-on:\s*([^\s#]+)/gm)].map(
    ([, runner]) => runner,
  );

  assert.ok(workflowFiles.length > 0, "workflow files must exist");
  assert.doesNotMatch(workflows, /self-hosted/i);
  assert.ok(runners.length > 0, "workflows must declare runners");
  assert.ok(
    runners.every((runner) =>
      ["ubuntu-latest", "windows-latest"].includes(runner),
    ),
    `unexpected runner: ${runners.join(", ")}`,
  );
  assert.equal(
    existsSync(join(workflowRoot, "windows-acceptance.yml")),
    false,
    "real-game acceptance must remain manual",
  );
});

test("CI runs the portable allocation probe on a hosted runner and keeps its output", () => {
  const workflow = readFileSync(join(workflowRoot, "ci.yml"), "utf8");

  // The probe measures work per operation, which the Windows matrix cannot,
  // so it gets its own hosted job rather than being folded into an existing
  // one: a budget failure must name the scenario, not a Windows build step.
  assert.match(workflow, /perf-probe:\s*\n\s*name: Portable allocation probe/);
  assert.match(workflow, /cargo test --locked --manifest-path src-tauri\/Cargo\.toml --lib perf_scenarios -- --test-threads=1 --nocapture/);
  // --test-threads=1 is not cosmetic: the probe's arm flag is process-wide.
  assert.match(workflow, /perf_scenarios -- --test-threads=1 --nocapture/);
  assert.match(workflow, /WUL1\|/);
  // The measurements are the evidence; a job that runs and discards them is
  // not a measurement.
  assert.match(
    workflow,
    /name: performance-evidence-\$\{\{ github\.run_number \}\}/,
  );
  assert.match(workflow, /path: \$\{\{ runner\.temp \}\}\/wuwaid-ci-evidence\/perf-probe\/scenarios\.txt/);
  assert.doesNotMatch(workflow, /self-hosted/i);
});

test("the allocation probe job stays on a hosted Linux runner", () => {
  const workflow = readFileSync(join(workflowRoot, "ci.yml"), "utf8");
  const job = workflow.slice(workflow.indexOf("\n  perf-probe:"));
  const before = job.slice(0, job.indexOf("\n  windows:"));

  assert.match(before, /runs-on:\s*ubuntu-latest/);
  assert.doesNotMatch(before, /runs-on:\s*windows-latest/);
  // A probe failure has to fail the job, not be swallowed by a capture step.
  assert.doesNotMatch(before, /continue-on-error/);
  // The Windows matrix stays exactly as it was: this job is additive.
  assert.match(workflow, /run-windows-fixture-performance\.ps1/);
  assert.match(workflow, /path:\s*performance-evidence/);
});
