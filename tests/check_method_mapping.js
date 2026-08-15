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
