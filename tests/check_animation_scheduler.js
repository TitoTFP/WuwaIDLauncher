const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const listeners = {};
const pending = new Set();
let nextFrame = 0;
const context = vm.createContext({
    console,
    Map,
    Math,
    Uint8Array,
    setTimeout,
    clearTimeout,
    window: {},
    document: {
        hidden: false,
        addEventListener(type, handler) { listeners[type] = handler; }
    },
    requestAnimationFrame() {
        const id = ++nextFrame;
        pending.add(id);
        return id;
    },
    cancelAnimationFrame(id) { pending.delete(id); }
});

const source = fs.readFileSync(
    path.join(__dirname, '..', 'Resources', 'Web', 'script-fx.js'),
    'utf8');
vm.runInContext(source, context);
const scheduler = vm.runInContext('LauncherAnimationScheduler', context);

scheduler.add('test', 24, () => {});
assert.equal(pending.size, 1);
scheduler.setModeEnabled(false);
assert.equal(pending.size, 0, 'off mode must cancel pending animation frame');

context.document.hidden = true;
listeners.visibilitychange();
context.document.hidden = false;
listeners.visibilitychange();
assert.equal(pending.size, 0, 'visibility restore must not wake off mode');

scheduler.setModeEnabled(true);
assert.equal(pending.size, 1);
scheduler.pause();
assert.equal(pending.size, 0);
listeners.visibilitychange();
assert.equal(pending.size, 0, 'visibility restore must not override manual pause');
scheduler.resume();
assert.equal(pending.size, 1);

console.log('animation scheduler self-check passed');
