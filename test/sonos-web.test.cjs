// ABOUTME: Unit tests for Sonos control rendering and request handling in the shipped page.
// ABOUTME: A minimal DOM exercises UI decisions; these are not browser or playback E2E tests.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function page(name = 'index') {
  const nodes = new Map();
  const node = () => ({ textContent: '', value: '', checked: false, disabled: false,
    children: [], addEventListener() {}, removeAttribute() {},
    replaceChildren(...children) { this.children = children; },
  });
  const html = fs.readFileSync(`firmware/src/${name}.html`, 'utf8');
  for (const match of html.matchAll(/id="([^"]+)"/g)) nodes.set(match[1], node());
  const document = { activeElement: null, getElementById(id) {
    return nodes.get(id) || null;
  }, createElement: node };
  const sandbox = { document, AbortSignal, setInterval() {}, Date,
    fetch: () => new Promise(() => {}),
  };
  vm.createContext(sandbox);
  vm.runInContext(fs.readFileSync('firmware/src/app.js', 'utf8'), sandbox);
  return { sandbox, document, get: (id) => document.getElementById(id), run: (code) => vm.runInContext(code, sandbox) };
}
function snapshot(overrides = {}) {
  return { config: { player_id: 'player-1', autoplay: false },
    groups: [{ player_id: 'player-1', name: '<img src=x onerror=bad()>' }],
    active_player_id: 'player-1', state: 'buffering', last_error: null, ...overrides };
}

test('renders actual state, group names and errors as text; autoplay starts off', () => {
  const p = page();
  p.sandbox.renderSonos(snapshot({ last_error: '<script>failure</script>' }));
  assert.equal(p.get('sonos-status').textContent, 'buffering');
  assert.equal(p.get('sonos-error').textContent, '<script>failure</script>');
  assert.equal(p.get('sonos-destination').children[1].textContent, '<img src=x onerror=bad()>');
  assert.equal(p.get('sonos-autoplay'), null);
});

test('polling preserves focused dropdown contents and edited selection', () => {
  const p = page();
  p.sandbox.renderSonos(snapshot());
  const select = p.get('sonos-destination');
  const options = select.children;
  p.document.activeElement = select;
  select.value = 'candidate';
  p.sandbox.renderSonos(snapshot({ groups: [] }));
  assert.equal(select.children, options);
  assert.equal(select.value, 'candidate');
  assert.equal(p.get('sonos-play').disabled, true);
});

test('retains persisted destination when discovery cannot see it', () => {
  const p = page();
  p.sandbox.renderSonos(snapshot({ groups: [], active_player_id: null, state: 'stopped' }));
  assert.equal(p.get('sonos-destination').value, 'player-1');
  assert.match(p.get('sonos-destination').children[1].textContent, /unavailable/);
  assert.equal(p.get('sonos-stop').disabled, true);
});

test('accepted POST stays queued and never fabricates playing state', async () => {
  const p = page();
  p.sandbox.renderSonos(snapshot());
  p.sandbox.fetch = async () => ({ ok: true, json: async () => ({ accepted: true }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'play' }), true);
  assert.equal(p.get('sonos-status').textContent, 'buffering');
  assert.match(p.get('sonos-action-status').textContent, /queued/i);
});

test('failed POST shows its error and restores confirmed autoplay setting', async () => {
  const p = page('settings');
  p.sandbox.renderSonos(snapshot());
  p.get('sonos-autoplay').checked = true;
  p.sandbox.fetch = async () => ({ ok: false, json: async () => ({ error: 'Queue full' }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'autoplay', enabled: true }), false);
  assert.equal(p.get('sonos-action-status').textContent, 'Queue full');
  assert.equal(p.get('sonos-autoplay').checked, false);
});

test('an old controller error cannot cancel a newly queued selection', async () => {
  const p = page();
  p.sandbox.renderSonos(snapshot({ last_error: 'Earlier attempt failed' }));
  p.sandbox.fetch = async () => ({ ok: true, json: async () => ({ accepted: true }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'select', player_id: 'player-2' }), true);
  assert.match(p.get('sonos-action-status').textContent, /queued/i);
  assert.equal(p.get('sonos-select').disabled, true);
  assert.equal(p.get('sonos-play').disabled, true);
  p.sandbox.renderSonos(snapshot({ last_error: 'Earlier attempt failed' }));
  assert.match(p.get('sonos-action-status').textContent, /queued/i);
  p.sandbox.renderSonos(snapshot({ config: { player_id: 'player-2', autoplay: false } }));
  assert.equal(p.get('sonos-action-status').textContent, 'Setting saved.');
});

test('buffer choice waits for saved settings and preserves Stop', async () => {
  const p = page('settings');
  const initial = snapshot({ buffer_available: true, config: { player_id: 'player-1', autoplay: true, buffer_preset: 'med' } });
  p.sandbox.renderSonos(initial);
  assert.equal(p.get('sonos-buffer').value, 'med');
  assert.equal(p.get('sonos-buffer').disabled, false);
  p.get('sonos-buffer').value = 'high';
  p.sandbox.fetch = async () => ({ ok: true, json: async () => ({ accepted: true }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'buffer', preset: 'high' }), true);
  p.sandbox.renderSonos(initial);
  assert.match(p.get('sonos-action-status').textContent, /queued/i);
  assert.equal(p.get('sonos-buffer').disabled, true);
  const playback = page();
  playback.sandbox.renderSonos(initial);
  assert.equal(playback.get('sonos-stop').disabled, false);
  assert.equal(p.get('sonos-play'), null);
  p.sandbox.renderSonos({ ...initial, config: { ...initial.config, buffer_preset: 'high' } });
  assert.equal(p.get('sonos-action-status').textContent, 'Setting saved.');
  assert.equal(p.get('sonos-buffer').value, 'high');
  assert.equal(p.get('sonos-autoplay').checked, true);
  assert.equal(p.get('sonos-status').textContent, 'buffering');
});

test('buffer failure restores confirmed choice and recognition disables presets', async () => {
  const p = page('settings');
  const initial = snapshot({ buffer_available: true, config: { player_id: 'player-1', autoplay: false, buffer_preset: 'low' } });
  p.sandbox.renderSonos(initial);
  p.get('sonos-buffer').value = 'high';
  p.sandbox.fetch = async () => ({ ok: false, json: async () => ({ error: 'Queue full' }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'buffer', preset: 'high' }), false);
  assert.equal(p.get('sonos-buffer').value, 'low');
  assert.equal(p.get('sonos-action-status').textContent, 'Queue full');
  p.sandbox.renderSonos({ ...initial, buffer_available: false });
  assert.equal(p.get('sonos-buffer').disabled, true);
  assert.match(p.get('sonos-buffer-note').textContent, /recognition/i);
});

test('format confirms saved choice independently of active playback and buffer', async () => {
  const p = page('settings');
  const initial = snapshot({ active_format: 'mp3', buffer_available: true, config: { player_id: 'player-1', autoplay: false, buffer_preset: 'high', output_format: 'mp3' } });
  p.sandbox.renderSonos(initial);
  assert.equal(p.get('sonos-format').value, 'mp3');
  p.sandbox.fetch = async () => ({ ok: true, json: async () => ({ accepted: true }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'format', format: 'flac' }), true);
  p.sandbox.renderSonos(initial);
  assert.match(p.get('sonos-action-status').textContent, /queued/i);
  p.sandbox.renderSonos({ ...initial, config: { ...initial.config, output_format: 'flac' } });
  assert.equal(p.get('sonos-action-status').textContent, 'Setting saved.');
  assert.match(p.get('sonos-format-note').textContent, /MP3.*next Play/i);
  assert.equal(p.get('sonos-buffer').value, 'high');
});

test('settings owns controls and both pages use one board-local script', () => {
  const main = fs.readFileSync('firmware/src/index.html', 'utf8');
  const settings = fs.readFileSync('firmware/src/settings.html', 'utf8');
  assert.match(main, /href="\/settings"/);
  assert.match(settings, /href="\/"/);
  for (const html of [main, settings]) assert.match(html, /src="\/app.js"/);
  assert.doesNotMatch(main, /id="sonos-(format|buffer|autoplay)"/);
  assert.doesNotMatch(settings, /id="sonos-(play|stop|destination)"/);
  const p = page('settings');
  p.sandbox.renderSonos(snapshot({ buffer_available: false, active_format: 'flac' }));
  assert.equal(p.get('sonos-format').disabled, false);
  assert.match(p.get('sonos-format-note').textContent, /recognition/);
});

test('recognition keeps FLAC recovery available when saved MP3 cannot run', async () => {
  const p = page('settings');
  const initial = snapshot({ buffer_available: false, active_format: 'flac', state: 'stopped', last_error: 'MP3 unavailable while recognition is enabled', config: { player_id: 'player-1', autoplay: false, buffer_preset: 'high', output_format: 'mp3' } });
  p.sandbox.renderSonos(initial);
  assert.equal(p.get('sonos-format').disabled, false);
  assert.equal(p.get('sonos-format').value, 'mp3');
  assert.equal(p.get('sonos-format-mp3').disabled, true);
  assert.match(p.get('sonos-format-note').textContent, /Active: FLAC.*select FLAC/i);
  p.sandbox.fetch = async () => ({ ok: true, json: async () => ({ accepted: true }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'format', format: 'flac' }), true);
  p.sandbox.renderSonos({ ...initial, last_error: null, config: { ...initial.config, output_format: 'flac' } });
  assert.equal(p.get('sonos-format').value, 'flac');
  assert.equal(p.get('sonos-buffer').value, 'high');
  assert.equal(p.get('sonos-action-status').textContent, 'Setting saved.');
});

async function withLocalApi(handler, run) {
  const server = require('node:http').createServer(handler);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  try {
    await run(base);
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

test('actual page waits for slow HTTP command acceptance beyond three seconds', async () => {
  let received = '';
  await withLocalApi((request, response) => {
    request.setEncoding('utf8');
    request.on('data', (chunk) => { received += chunk; });
    request.on('end', () => {
      const timer = setTimeout(() => {
        response.writeHead(202, { 'Content-Type': 'application/json' });
        response.end('{"accepted":true}');
      }, 3200);
      response.on('close', () => clearTimeout(timer));
    });
  }, async (base) => {
    const p = page();
    p.sandbox.renderSonos(snapshot({ state: 'playing' }));
    p.sandbox.fetch = (url, init) => fetch(new URL(url, base), init);
    assert.equal(await p.sandbox.sendSonos({ action: 'stop' }), true);
    assert.deepEqual(JSON.parse(received), { action: 'stop' });
    assert.match(p.get('sonos-action-status').textContent, /queued/i);
    assert.equal(p.get('sonos-status').textContent, 'playing');
  });
});

test('actual HTTP abort reports unknown outcome and polling can confirm later state', async () => {
  await withLocalApi((request, response) => {
    if (request.method === 'GET') {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify(snapshot({ state: 'stopped', active_player_id: null })));
    }
    // The POST intentionally has no acceptance response before this unit's short abort.
  }, async (base) => {
    const p = page();
    let acceptanceDeadline = 0;
    p.sandbox.AbortSignal = { timeout(ms) {
      acceptanceDeadline = ms;
      return AbortSignal.timeout(100);
    } };
    p.sandbox.renderSonos(snapshot({ state: 'playing' }));
    p.sandbox.fetch = (url, init) => fetch(new URL(url, base), init);
    assert.equal(await p.sandbox.sendSonos({ action: 'stop' }), false);
    assert.ok(acceptanceDeadline >= 7000, 'guarded send needs five seconds plus acceptance margin');
    assert.match(p.get('sonos-action-status').textContent, /outcome unknown.*check status/i);
    p.sandbox.AbortSignal = AbortSignal;
    await p.sandbox.refreshSonos();
    assert.equal(p.get('sonos-status').textContent, 'stopped');
    assert.equal(p.get('sonos-stop').disabled, true);
  });
});
