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

test("the allocation probe job runs the scenarios opt-in, in isolation", () => {
  const workflow = readFileSync(join(workflowRoot, "ci.yml"), "utf8");
  const job = workflow.slice(workflow.indexOf("\n  perf-probe:"));
  const before = job.slice(0, job.indexOf("\n  windows:"));

  // The scenarios are #[ignore]d, so the general correctness suite skips them
  // and this job has to ask for them by name. Without --ignored the step would
  // run nothing and pass, which is the failure mode this assertion exists for.
  assert.match(
    before,
    /--lib perf_scenarios -- --test-threads=1 --nocapture --ignored/,
  );
  // Exactly one probe invocation, so the opt-in cannot be quietly dropped from
  // the job and left running only inside the general suite.
  assert.equal(
    [...before.matchAll(/--lib perf_scenarios/g)].length,
    1,
    "the probe job must invoke the scenarios exactly once",
  );
  // The general suite must not opt in: --all-targets is the required Windows
  // correctness check, and the probe is a measurement, not a correctness test.
  assert.doesNotMatch(
    workflow,
    /--all-targets -- --ignored/,
    "the general correctness suite must not run the opt-in scenarios",
  );
  // --test-threads=1 is not cosmetic: the probe's arm flag is process-wide.
  assert.match(before, /--test-threads=1/);
});

test("the allocation probe step fails on a failing scenario and still logs the measurements", () => {
  const workflow = readFileSync(join(workflowRoot, "ci.yml"), "utf8");
  const job = workflow.slice(workflow.indexOf("\n  perf-probe:"));
  const before = job.slice(0, job.indexOf("\n  windows:"));
  const step = before.slice(before.indexOf("- name: Run allocation probe scenarios"));

  // `cargo test | tee` hands the pipeline's status to tee, so without
  // pipefail a budget breach is swallowed and the job goes green.
  assert.match(step, /set -[a-z]*o pipefail|set -o pipefail/);
  // The harness prints a scenario's captured stdout as a suffix of that test's
  // own status line ("test perf_scenarios::idle_monitor_tick ... WUL1|..."),
  // so an anchored `^WUL1|` matches nothing and fails the step after the tests
  // passed. The marker has to be matched anywhere in the line.
  assert.doesNotMatch(
    step,
    /grep\s+['"]\^WUL1/,
    "WUL1| is a line suffix, so it must not be anchored to the start of a line",
  );
  assert.match(step, /grep -F 'WUL1\|'/);
  // The measurements must reach the job log, not only the uploaded artifact.
  assert.match(step, /\|\s*tee\s+"\$RUNNER_TEMP\/wuwaid-ci-evidence\/perf-probe\/scenarios\.txt"/);
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
