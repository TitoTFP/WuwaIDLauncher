import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

// This is only the transport adapter. Every command response and event payload
// comes from the Rust/Tauri fixture process, while production bridge.ts remains
// the module under test.
const repoRoot = process.env.WUWAID_FIXTURE_ROOT;
if (!repoRoot) throw new Error("WUWAID_FIXTURE_ROOT is required");

export let LEGACY_GAME_PATH = "";
export let CANONICAL_GAME_PATH = "";

export const calls = {
  invoke: [],
  events: [],
};

const pending = new Map();
const listeners = new Map();
let nextRequestId = 1;
let stopped = false;
let readySettled = false;
let readyResolve;
let readyReject;
let closeResolve;

const fixtureBinary = `${repoRoot}/src-tauri/target/debug/${
  process.platform === "win32"
    ? "launcher-state-fixture.exe"
    : "launcher-state-fixture"
}`;
const child = spawn(fixtureBinary, [], {
  cwd: repoRoot,
  stdio: ["pipe", "pipe", "pipe"],
});

const fixtureReady = new Promise((resolve, reject) => {
  readyResolve = resolve;
  readyReject = reject;
});
const fixtureClosed = new Promise((resolve) => {
  closeResolve = resolve;
});

function clone(value) {
  return value === undefined ? undefined : structuredClone(value);
}

function finishReady(paths) {
  if (readySettled) return;
  readySettled = true;
  LEGACY_GAME_PATH = paths.legacyGamePath;
  CANONICAL_GAME_PATH = paths.canonicalGamePath;
  readyResolve(paths);
}

function failFixture(error) {
  if (!readySettled) {
    readySettled = true;
    readyReject(error);
  }
  for (const { reject } of pending.values()) reject(error);
  pending.clear();
}

function handleFrame(frame) {
  if (frame.type === "ready") {
    finishReady(frame);
    return;
  }
  if (frame.type === "event") {
    calls.events.push({ event: frame.event, payload: clone(frame.payload) });
    for (const handler of listeners.get(frame.event)?.values() ?? []) {
      handler({
        event: frame.event,
        id: 0,
        windowLabel: "main",
        payload: frame.payload,
      });
    }
    return;
  }
  if (frame.type !== "response") return;
  const request = pending.get(frame.id);
  if (!request) return;
  pending.delete(frame.id);
  if (Object.hasOwn(frame, "error")) {
    request.reject(new Error(JSON.stringify(frame.error)));
  } else {
    request.resolve(frame.result);
  }
}

const lines = createInterface({ input: child.stdout });
lines.on("line", (line) => {
  try {
    handleFrame(JSON.parse(line));
  } catch (error) {
    failFixture(new Error(`invalid Rust fixture frame: ${error}`));
  }
});
child.stderr.on("data", (chunk) => process.stderr.write(chunk));
child.once("error", failFixture);
child.once("close", (code, signal) => {
  closeResolve({ code, signal });
  if (!stopped) {
    failFixture(
      new Error(
        `Rust frontend fixture exited before shutdown (${code ?? signal})`,
      ),
    );
  }
});

function request(command, args, recordCall) {
  const id = nextRequestId++;
  if (recordCall) calls.invoke.push({ command, args: clone(args) });
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    child.stdin.write(`${JSON.stringify({ id, command, args })}\n`, (error) => {
      if (error) {
        pending.delete(id);
        reject(error);
      }
    });
  });
}

export async function invoke(command, args = {}) {
  const paths = await fixtureReady;
  void paths;
  return request(command, args, true);
}

export function listen(event, handler) {
  const id = nextRequestId++;
  const eventListeners = listeners.get(event) ?? new Map();
  eventListeners.set(id, handler);
  listeners.set(event, eventListeners);
  return Promise.resolve(() => {
    eventListeners.delete(id);
    if (eventListeners.size === 0) listeners.delete(event);
  });
}

export async function shutdownFixture() {
  if (stopped) return;
  stopped = true;
  if (child.exitCode === null) {
    await request("__shutdown", {}, false).catch(() => {});
    await Promise.race([
      fixtureClosed,
      new Promise((resolve) => setTimeout(resolve, 1_000)),
    ]);
    if (child.exitCode === null) child.kill();
  }
}

export { fixtureReady };
