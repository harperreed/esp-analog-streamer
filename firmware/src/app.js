// ABOUTME: Shared playback and settings controls backed by the board Sonos API.
// ABOUTME: Polling confirms saved settings and keeps playback state separate from requested actions.
const $ = (id) => document.getElementById(id);
const set = (id, property, value) => { const node = $(id); if (node) node[property] = value; };
let sonosSnapshot = null;
let sonosBusy = false;
let sonosEdited = false;
let sonosPending = null;
let sonosVersion = 0;

function renderSonos(s, observed = true) {
  sonosSnapshot = s;
  if (sonosPending && observed) {
    const confirmed = sonosPending.action === 'select'
      ? s.config.player_id === sonosPending.player_id
      : sonosPending.action === 'buffer'
        ? s.config.buffer_preset === sonosPending.preset
        : sonosPending.action === 'format'
          ? s.config.output_format === sonosPending.format
          : s.config.autoplay === sonosPending.enabled;
    const failed = s.last_error && s.last_error !== sonosPending.previousError;
    if (confirmed || failed || Date.now() >= sonosPending.until) {
      set('sonos-action-status', 'textContent', confirmed ? 'Setting saved.' : (s.last_error || 'Setting not confirmed; check the destination and retry.'));
      sonosPending = null;
      sonosEdited = false;
    }
  }
  const select = $('sonos-destination');
  if (select && document.activeElement !== select) {
    const wanted = sonosPending?.player_id || (sonosEdited ? select.value : s.config.player_id) || '';
    const options = [{ player_id: '', name: 'Choose a destination' }, ...s.groups];
    if (wanted && !options.some(g => g.player_id === wanted)) {
      options.push({ player_id: wanted, name: 'Selected destination (unavailable)' });
    }
    select.replaceChildren(...options.map(g => {
      const option = document.createElement('option');
      option.value = g.player_id;
      option.textContent = g.name;
      return option;
    }));
    select.value = wanted;
  }
  set('sonos-format', 'value', sonosPending?.format || s.config.output_format || 'flac');
  set('sonos-format', 'disabled', sonosBusy || Boolean(sonosPending));
  set('sonos-format-mp3', 'disabled', !s.buffer_available);
  const activeFormat = (s.active_format || 'flac').toUpperCase();
  set('sonos-format-note', 'textContent', !s.buffer_available
    ? `Active: ${activeFormat}. MP3 unavailable while recognition is enabled; select FLAC to play.`
    : s.config.output_format && s.config.output_format !== s.active_format
      ? `Active: ${activeFormat}. Selected format applies on next Play.`
      : `Active: ${activeFormat}. Changes during playback apply on next Play.`);
  set('sonos-autoplay', 'checked', s.config.autoplay);
  set('sonos-buffer', 'value', sonosPending?.preset || s.config.buffer_preset);
  set('sonos-buffer', 'disabled', sonosBusy || Boolean(sonosPending) || !s.buffer_available);
  set('sonos-buffer-note', 'textContent', s.buffer_available
    ? 'Applies on next Play.' : 'Startup buffer unavailable while recognition is enabled.');
  set('sonos-status', 'textContent', s.state.replaceAll('_', ' '));
  set('sonos-error', 'textContent', s.last_error || '');
  const active = s.groups.find(g => g.player_id === s.active_player_id);
  set('sonos-active', 'textContent', s.active_player_id ? `Active destination: ${active?.name || 'previously selected destination'}` : '');
  set('sonos-destination', 'disabled', sonosBusy || Boolean(sonosPending));
  set('sonos-discover', 'disabled', sonosBusy);
  set('sonos-select', 'disabled', sonosBusy || Boolean(sonosPending) || !select?.value || select?.value === s.config.player_id);
  set('sonos-play', 'disabled', sonosBusy || Boolean(sonosPending) || !s.config.player_id || select?.value !== s.config.player_id);
  set('sonos-stop', 'disabled', sonosBusy || (!s.active_player_id && s.state === 'stopped'));
  set('sonos-autoplay', 'disabled', sonosBusy || Boolean(sonosPending));
}

async function sendSonos(action) {
  if (sonosBusy) return false;
  sonosBusy = true;
  ++sonosVersion;
  if (sonosSnapshot) renderSonos(sonosSnapshot, false);
  set('sonos-action-status', 'textContent', 'Sending request…');
  try {
    const response = await fetch('/api/sonos', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      // Cancellation can wait for a bounded five-second Sonos send before acceptance.
      body: JSON.stringify(action), signal: AbortSignal.timeout(8000),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || 'Sonos request failed.');
    if (action.action === 'select' || action.action === 'autoplay' || action.action === 'buffer' || action.action === 'format') {
      sonosPending = { ...action, previousError: sonosSnapshot?.last_error, until: Date.now() + 10000 };
    }
    set('sonos-action-status', 'textContent', 'Request queued; waiting for the controller.');
    return true;
  } catch (error) {
    const timedOut = error.name === 'TimeoutError' || error.name === 'AbortError';
    set('sonos-action-status', 'textContent', timedOut
      ? 'Request timed out; outcome unknown. Check status before retrying.'
      : error.message || 'Sonos request failed.');
    return false;
  } finally {
    sonosBusy = false;
    ++sonosVersion;
    if (sonosSnapshot) renderSonos(sonosSnapshot, false);
  }
}

async function refreshSonos() {
  const version = sonosVersion;
  try {
    const response = await fetch('/api/sonos', { cache: 'no-store', signal: AbortSignal.timeout(2500) });
    if (!response.ok) throw new Error('Sonos status unavailable');
    const snapshot = await response.json();
    if (!sonosBusy && version === sonosVersion) renderSonos(snapshot);
  } catch (error) {
    if (version === sonosVersion) set('sonos-error', 'textContent', 'Sonos status unavailable; retrying…');
  }
}

$('sonos-destination')?.addEventListener('change', () => {
  sonosEdited = true;
  if (sonosSnapshot) renderSonos(sonosSnapshot, false);
});
$('sonos-destination')?.addEventListener('blur', () => {
  if (sonosSnapshot) renderSonos(sonosSnapshot, false);
});
$('sonos-discover')?.addEventListener('click', () => sendSonos({ action: 'discover' }));
$('sonos-select')?.addEventListener('click', () => sendSonos({ action: 'select', player_id: $('sonos-destination').value }));
$('sonos-play')?.addEventListener('click', () => sendSonos({ action: 'play' }));
$('sonos-stop')?.addEventListener('click', () => sendSonos({ action: 'stop' }));
$('sonos-format')?.addEventListener('change', () => sendSonos({ action: 'format', format: $('sonos-format').value }));
$('sonos-buffer')?.addEventListener('change', () => sendSonos({ action: 'buffer', preset: $('sonos-buffer').value }));
$('sonos-autoplay')?.addEventListener('change', () => sendSonos({ action: 'autoplay', enabled: $('sonos-autoplay').checked }));
// Input gain: one request, confirmed by reading the status back; the select is disabled meanwhile.
let inputBusy = false;
async function refreshInput() {
  const select = $('input-gain');
  if (!select || inputBusy) return;
  try {
    const response = await fetch('/api/input', { cache: 'no-store', signal: AbortSignal.timeout(2500) });
    if (!response.ok) throw new Error('Input gain unavailable');
    const status = await response.json();
    if (!inputBusy && document.activeElement !== select) select.value = String(status.gain_db);
    select.disabled = false;
  } catch (error) {
    select.disabled = true;
  }
}
async function sendInput(gainDb) {
  const select = $('input-gain');
  if (!select || inputBusy) return;
  inputBusy = true;
  select.disabled = true;
  set('input-gain-note', 'textContent', 'Applying…');
  try {
    const response = await fetch('/api/input', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ gain_db: gainDb }), signal: AbortSignal.timeout(5000),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || 'Input gain request failed.');
    select.value = String(result.gain_db);
    set('input-gain-note', 'textContent', `Input gain ${result.gain_db} dB applied and saved. Check the level on the Playback page.`);
  } catch (error) {
    set('input-gain-note', 'textContent', error.message || 'Input gain request failed.');
  } finally {
    inputBusy = false;
    select.disabled = false;
  }
}
$('input-gain')?.addEventListener('change', () => sendInput(Number($('input-gain').value)));
if ($('input-gain')) { refreshInput(); setInterval(refreshInput, 5000); }
// The URLs come from Shazam's response: only https ones reach src or href, so no javascript: links.
const https = (u) => typeof u === 'string' && /^https:\/\//.test(u);
async function refresh() {
  try {
    const s = await (await fetch('/api/state', { cache: 'no-store', signal: AbortSignal.timeout(2500) })).json();
    const paused = s.rate_limited_until ? ` · paused until ${s.rate_limited_until}` : '';
    // A board built without Shazam runs no listen policy: its state and "Nothing playing." would mislead.
    const off = s.shazam_enabled === false;
    const label = off ? 'Shazam is off' : s.state.replace('_', ' ');
    $('status').textContent = `${label} · ${s.level_dbfs.toFixed(1)} dBFS${paused}`;
    const n = s.now;
    $('now').hidden = !n;
    $('idle').hidden = Boolean(n) || off;
    if (n) {
      $('title').textContent = n.title;
      $('artist').textContent = n.artist;
      $('album').textContent = n.album || '';
      if (https(n.cover_url)) { $('cover').src = n.cover_url; } else { $('cover').removeAttribute('src'); }
      $('link').hidden = !https(n.shazam_url);
      if (https(n.shazam_url)) { $('link').href = n.shazam_url; }
    }
    $('history').replaceChildren(...s.history.map((h) => {
      const li = document.createElement('li');
      li.textContent = `${h.title} — ${h.artist} (${h.identified_at.replace('T', ' ').replace('Z', ' UTC')})`;
      return li;
    }));
  } catch (e) {
    $('status').textContent = 'board unreachable, retrying…';
  }
}
if ($('status')) {
  refresh();
  setInterval(refresh, 3000);
}
refreshSonos();
setInterval(refreshSonos, 3000);
