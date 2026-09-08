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
