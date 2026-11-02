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

test('Sonos is opt-in: the playback page hides its section until enabled and shows the stream for any player', () => {
  const p = page();
  p.sandbox.location = { protocol: 'http:', hostname: 'analog-streamer.local' };
  const player = p.get('player');
  let src = null; player.setAttribute = (k, v) => { if (k === 'src') src = v; }; player.getAttribute = (k) => (k === 'src' ? src : null);
  p.sandbox.renderSonos(snapshot({ config: { player_id: 'player-1', autoplay: false, enabled: false }, active_format: 'mp3' }));
  assert.equal(p.get('sonos-section').hidden, true);
  assert.equal(p.get('stream-link').textContent, 'http://analog-streamer.local:8000/stream.mp3');
  assert.equal(src, 'http://analog-streamer.local:8000/stream.mp3');
  p.sandbox.renderSonos(snapshot({ config: { player_id: 'player-1', autoplay: false, enabled: true }, active_format: 'flac' }));
  assert.equal(p.get('sonos-section').hidden, false);
  assert.equal(src, 'http://analog-streamer.local:8000/stream.flac');
  assert.equal(p.get('stream-link').textContent, 'http://analog-streamer.local:8000/stream.flac');
});

test('the settings page renders the Sonos switch from the saved flag and confirms an enable request', async () => {
  const p = page('settings');
  p.sandbox.renderSonos(snapshot({ config: { player_id: null, autoplay: false, enabled: false, buffer_preset: 'med', output_format: 'mp3' } }));
  assert.equal(p.get('sonos-enabled').checked, false);
  p.sandbox.fetch = async () => ({ ok: true, json: async () => ({ accepted: true }) });
  assert.equal(await p.sandbox.sendSonos({ action: 'enable', enabled: true }), true);
  assert.equal(p.get('sonos-enabled').checked, true, 'pending request shows as on');
  p.sandbox.renderSonos(snapshot({ config: { player_id: null, autoplay: false, enabled: true, buffer_preset: 'med', output_format: 'mp3' } }));
  assert.equal(p.get('sonos-action-status').textContent, 'Setting saved.');
});

test('the status line reports audio from the meter and hides history while recognition is off', async () => {
  const p = page();
  const state = { state: 'quiet', shazam_enabled: false, level_dbfs: -23.4, audio_present: true, rate_limited_until: null, now: null, history: [] };
  p.sandbox.fetch = async () => ({ json: async () => state });
  await p.sandbox.refresh();
  assert.equal(p.get('status').textContent, 'audio playing · recognition off · -23.4 dBFS');
  assert.equal(p.get('history-section').hidden, true);
  assert.equal(p.get('idle').hidden, true);
  state.audio_present = false; state.level_dbfs = -75; state.shazam_enabled = true;
  await p.sandbox.refresh();
  assert.equal(p.get('status').textContent, 'quiet · quiet · -75.0 dBFS');
  assert.equal(p.get('history-section').hidden, false);
});

test('the system page renders firmware, slots, memory, capture, listeners and network as text with units', () => {
  const p = page('system');
  const metrics = {
    build_elf_sha256: '9f0c4899a33fa045' + 'a'.repeat(48), boot_token: '8dabe006' + 'b'.repeat(24), served_device_us: 3_725_000_000,
    settings: { active_format: 'mp3', recognition_enabled: false }, http_stack_free_bytes: 5120,
    capture: { read_frames: 12345678, queue_overflows: 8, software_dropped_frames: 0, read_errors: 0, restarts: 0, discontinuities: 0,
      internal_free: 24231, internal_largest: 22528, internal_min_sum: 20000, psram_free: 2728872, psram_largest: 2359296 },
    encoder: { encoded_frames: 416, resets: 0, encode_max_us: 30805 },
    listeners: [{ state: 'writing', media_age_us: 52445, skips: 1, frames_skipped: 180 }, { state: 'idle' }],
    listener_loss: { metrics_stack_free_bytes: 1832 },
    last_listener_departure: { listener_stack_free_bytes: 1964, stats: { elapsed_us: 569617, frames_sent: 52, skips: 0 } },
    network: { generation: 2, connected: true, disconnects: 0 },
  };
  const ota = { running: 'ota_1', boot: 'ota_1', running_state: 'pending_verify', validated: false, enabled: true, update_slot: 'ota_0', update_slot_bytes: 2031616 };
  p.sandbox.renderSystem(metrics, ota, { gain_db: 24, max_db: 24, step_db: 3 });
  assert.equal(p.get('system-image').textContent, '9f0c4899a33fa045…');
  assert.equal(p.get('system-uptime').textContent, '1 h 2 min');
  assert.match(p.get('system-slot').textContent, /^ota_1 \(pending verify\); next update goes to ota_0, 1984\.0 KB$/);
  assert.match(p.get('system-ota').textContent, /^enabled/);
  assert.equal(p.get('system-gain').textContent, '24 dB (0 to 24 in 3 dB steps)');
  assert.equal(p.get('system-format').textContent, 'MP3, recognition off');
  assert.equal(p.get('system-internal').textContent, '23.7 KB free, largest block 22.0 KB, window low 19.5 KB');
  assert.equal(p.get('system-stacks').textContent, 'http 5120 B · metrics 1832 B · listener 1964 B');
  assert.equal(p.get('system-faults').textContent, '8 queue overflows · 0 dropped · 0 read errors · 0 restarts · 0 discontinuities');
  assert.equal(p.get('system-encoder').textContent, '416 frames this window · 0 resets · worst block 30.8 ms');
  assert.equal(p.get('system-listeners').children.length, 2);
  assert.equal(p.get('system-listeners').children[0].textContent, 'slot 1: writing · 52 ms behind live · 1 skips (180 frames)');
  assert.equal(p.get('system-listeners').children[1].textContent, 'slot 2: idle');
  assert.equal(p.get('system-departure').textContent, 'last departure: 0.6 s connected, 52 frames sent, 0 skips');
  assert.equal(p.get('system-network').textContent, 'connected · generation 2 · 0 disconnects since boot');
  // Missing endpoints degrade to question marks, never throw.
  p.sandbox.renderSystem({}, null, null);
  assert.equal(p.get('system-slot').textContent, '?');
  assert.equal(p.get('system-stacks').textContent, 'not reported yet');
});
