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
      : sonosPending.action === 'enable'
        ? s.config.enabled === sonosPending.enabled
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
      ? `Active: ${activeFormat}. The selected format applies once nothing is listening, or on the next Play.`
      : `Active: ${activeFormat}. A change applies once nothing is listening, or on the next Play.`);
  set('sonos-enabled', 'checked', sonosPending?.action === 'enable' ? sonosPending.enabled : Boolean(s.config.enabled));
  set('sonos-enabled', 'disabled', sonosBusy || Boolean(sonosPending));
  const section = $('sonos-section');
  if (section) section.hidden = !s.config.enabled;
  // The stream URL follows the active format; the player's src only changes when the URL does, so playback is not reset.
  if (typeof location !== 'undefined') {
    const streamUrl = `${location.protocol}//${location.hostname}:8000/stream.${s.active_format || 'mp3'}`;
    const link = $('stream-link');
    if (link && link.href !== streamUrl) { link.href = streamUrl; link.textContent = streamUrl; }
    const player = $('player');
    if (player && player.getAttribute('src') !== streamUrl) player.setAttribute('src', streamUrl);
  }
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
    if (action.action === 'select' || action.action === 'autoplay' || action.action === 'buffer' || action.action === 'format' || action.action === 'enable') {
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
$('sonos-enabled')?.addEventListener('change', () => sendSonos({ action: 'enable', enabled: $('sonos-enabled').checked }));
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
// System page: facts from three read-only endpoints, rendered as text. Numbers get units; nothing is interpreted.
const kb = (bytes) => (bytes == null ? '?' : `${(bytes / 1024).toFixed(1)} KB`);
const seconds = (us) => {
  const s = Math.floor(us / 1e6);
  return s >= 3600 ? `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min` : s >= 60 ? `${Math.floor(s / 60)} min ${s % 60} s` : `${s} s`;
};
function renderSystem(metrics, ota, input) {
  const m = metrics || {};
  const c = m.capture || {};
  const e = m.encoder || {};
  const settings = m.settings || {};
  set('system-image', 'textContent', m.build_elf_sha256 ? `${m.build_elf_sha256.slice(0, 16)}…` : '?');
  set('system-boot', 'textContent', m.boot_token ? `${m.boot_token.slice(0, 8)}…` : '?');
  set('system-uptime', 'textContent', m.served_device_us != null ? seconds(m.served_device_us) : '?');
  set('system-slot', 'textContent', ota ? `${ota.running} (${ota.running_state.replaceAll('_', ' ')}${ota.validated ? ', validated' : ''}); next update goes to ${ota.update_slot}, ${kb(ota.update_slot_bytes)}` : '?');
  set('system-ota', 'textContent', ota ? (ota.enabled ? 'enabled (POST /api/ota with the token)' : 'disabled: no ota_token in cfg.toml') : '?');
  set('system-gain', 'textContent', input ? `${input.gain_db} dB (0 to ${input.max_db} in ${input.step_db} dB steps)` : '?');
  set('system-format', 'textContent', settings.active_format ? `${settings.active_format.toUpperCase()}${settings.recognition_enabled ? ', recognition on' : ', recognition off'}` : '?');
  set('system-internal', 'textContent', c.internal_free != null ? `${kb(c.internal_free)} free, largest block ${kb(c.internal_largest)}, window low ${kb(c.internal_min_sum)}` : '?');
  set('system-psram', 'textContent', c.psram_free != null ? `${kb(c.psram_free)} free, largest block ${kb(c.psram_largest)}` : '?');
  const stacks = [];
  if (m.http_stack_free_bytes != null) stacks.push(`http ${m.http_stack_free_bytes} B`);
  if (m.listener_loss?.metrics_stack_free_bytes != null) stacks.push(`metrics ${m.listener_loss.metrics_stack_free_bytes} B`);
  if (m.last_listener_departure?.listener_stack_free_bytes != null) stacks.push(`listener ${m.last_listener_departure.listener_stack_free_bytes} B`);
  set('system-stacks', 'textContent', stacks.length ? stacks.join(' · ') : 'not reported yet');
  set('system-frames', 'textContent', c.read_frames != null ? `${c.read_frames.toLocaleString()} read` : '?');
  set('system-faults', 'textContent', c.queue_overflows != null
    ? `${c.queue_overflows} queue overflows · ${c.software_dropped_frames} dropped · ${c.read_errors} read errors · ${c.restarts} restarts · ${c.discontinuities} discontinuities`
    : '?');
  set('system-encoder', 'textContent', e.encoded_frames != null ? `${e.encoded_frames} frames this window · ${e.resets} resets · worst block ${(e.encode_max_us / 1000).toFixed(1)} ms` : '?');
  const listeners = $('system-listeners');
  if (listeners) {
    listeners.replaceChildren(...(m.listeners || []).map((l, i) => {
      const li = document.createElement('li');
      li.textContent = l.state === 'idle' || l.state === 'unavailable'
        ? `slot ${i + 1}: ${l.state}`
        : `slot ${i + 1}: ${l.state} · ${l.media_age_us != null ? (l.media_age_us / 1000).toFixed(0) : '?'} ms behind live · ${l.skips ?? 0} skips (${l.frames_skipped ?? 0} frames)`;
      return li;
    }));
  }
  const d = m.last_listener_departure;
  set('system-departure', 'textContent', d?.stats ? `last departure: ${(d.stats.elapsed_us / 1e6).toFixed(1)} s connected, ${d.stats.frames_sent} frames sent, ${d.stats.skips} skips` : '');
  const n = m.network;
  set('system-network', 'textContent', n ? `${n.connected ? 'connected' : 'disconnected'} · generation ${n.generation} · ${n.disconnects} disconnects since boot` : '?');
}
async function refreshSystem() {
  const get = async (url) => {
    const response = await fetch(url, { cache: 'no-store', signal: AbortSignal.timeout(4000) });
    if (!response.ok) throw new Error(url);
    return response.json();
  };
  try {
    const [metrics, ota, input] = await Promise.all([get('/api/metrics'), get('/api/ota').catch(() => null), get('/api/input').catch(() => null)]);
    renderSystem(metrics, ota, input);
    set('system-status', 'textContent', `updated ${new Date().toLocaleTimeString()}`);
  } catch (error) {
    set('system-status', 'textContent', 'board unreachable, retrying…');
  }
}
if ($('system-image')) { refreshSystem(); setInterval(refreshSystem, 5000); }
// The URLs come from Shazam's response: only https ones reach src or href, so no javascript: links.
const https = (u) => typeof u === 'string' && /^https:\/\//.test(u);
async function refresh() {
  try {
    const s = await (await fetch('/api/state', { cache: 'no-store', signal: AbortSignal.timeout(2500) })).json();
    const paused = s.rate_limited_until ? ` · paused until ${s.rate_limited_until}` : '';
    // A board built without Shazam runs no listen policy: its state and "Nothing playing." would mislead.
    const off = s.shazam_enabled === false;
    // The meter speaks for itself: audio against the quiet threshold, whether or not recognition runs.
    const audio = s.audio_present ? 'audio playing' : 'quiet';
    const label = off ? `${audio} · recognition off` : `${audio} · ${s.state.replace('_', ' ')}`;
    $('status').textContent = `${label} · ${s.level_dbfs.toFixed(1)} dBFS${paused}`;
    const n = s.now;
    $('now').hidden = !n;
    $('idle').hidden = Boolean(n) || off;
    const history = $('history-section');
    if (history) history.hidden = off;
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
