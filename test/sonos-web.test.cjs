// ABOUTME: Unit tests for Sonos control rendering and request handling in the shipped page.
// ABOUTME: A minimal DOM exercises UI decisions; these are not browser or playback E2E tests.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function page() {
  const nodes = new Map();
  const node = () => ({ textContent: '', value: '', checked: false, disabled: false,
    children: [], addEventListener() {}, removeAttribute() {},
    replaceChildren(...children) { this.children = children; },
  });
  const document = { activeElement: null, getElementById(id) {
    if (!nodes.has(id)) nodes.set(id, node());
    return nodes.get(id);
  }, createElement: node };
  const sandbox = { document, AbortSignal, setInterval() {}, Date,
    fetch: () => new Promise(() => {}),
  };
  vm.createContext(sandbox);
  const html = fs.readFileSync('firmware/src/index.html', 'utf8');
  vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1], sandbox);
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
  assert.equal(p.get('sonos-autoplay').checked, false);
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
  const p = page();
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
  const p = page();
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
  assert.equal(p.get('sonos-stop').disabled, false);
  p.sandbox.renderSonos({ ...initial, config: { ...initial.config, buffer_preset: 'high' } });
  assert.equal(p.get('sonos-action-status').textContent, 'Setting saved.');
  assert.equal(p.get('sonos-buffer').value, 'high');
  assert.equal(p.get('sonos-autoplay').checked, true);
  assert.equal(p.get('sonos-status').textContent, 'buffering');
});

test('buffer failure restores confirmed choice and recognition disables presets', async () => {
  const p = page();
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
