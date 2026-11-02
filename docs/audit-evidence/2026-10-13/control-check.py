#!/usr/bin/env python3
# ABOUTME: Acquires bounded real-board control-read deadline and cancellation evidence.
# ABOUTME: Preserves failed observations and permits only discovery, initial Play and checked Stop.
import argparse
import hashlib
import ipaddress
import json
import os
import re
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
from sonos_http_check import Trial, NoRedirect, require, stream_identity, payload_receipt
from stream_soak import parse_capture_sample, LOSS_KEYS

def current_boot_uri(uri, host, transport, track_uri, accepted, current_boot):
    if not accepted or not current_boot or transport != 'PLAYING' or track_uri != uri:
        return None
    return uri, stream_identity(uri, host, 'mp3', 'med')

def markers(raw):
    require(len(raw) <= 2 * 1024 * 1024, 'UART exceeds 2 MiB')
    # Incomplete final lines are not evidence. A later read revisits them.
    complete_bytes = raw[:raw.rfind(b'\n') + 1]
    complete = complete_bytes.decode('utf-8', errors='replace')
    resets = re.findall(r'(?m)^ets Jul 29 2019 12:21:46\r?$', complete)
    require(len(resets) == 1, 'UART must contain one exact owned ROM reset')
    candidates = re.findall(r'(?m)^.*rst:.*$', complete)
    require(len(candidates) == 1 and re.fullmatch(
        r'rst:0x[0-9a-f]+ \([^\r\n]+\),boot:0x[0-9a-f]+ \([^\r\n]+\)\r?', candidates[0]),
        'missing, malformed or additional UART reset')
    pattern = re.compile(r'control-fault: phase=(identity|armed|stall-start|stall-end|observation-error|Degraded|Recovering|Playing|Stopped|Error) '
                         r'event_us=([0-9]+) command_generation=([0-9]+) sonos_intent=([0-9]+) network_generation=([0-9]+)'
                         r'(?: queue_item_id=([A-Za-z0-9_-]+))?(?: error_kind=([A-Za-z]+))?\r?$')
    rows = []
    offset = 0
    for raw_line in complete_bytes.splitlines(keepends=True):
        line = raw_line.decode('utf-8', errors='replace')
        if 'control-fault:' in line:
            match = pattern.search(line.rstrip('\n'))
            require(match is not None, 'malformed control-fault marker at byte ' + str(offset))
            phase, stamp, command, intent, network, queue, error_kind = match.groups()
            require((phase == 'identity') == (queue is not None), 'queue identity belongs only in identity marker')
            require((phase == 'observation-error') == (error_kind is not None), 'error kind belongs only in observation-error marker')
            row = dict(phase=phase, event_us=int(stamp), command_generation=int(command),
                       sonos_intent=int(intent), network_generation=int(network), offset=offset)
            if queue is not None:
                row['queue_item_id'] = queue
            if error_kind is not None:
                row['error_kind'] = error_kind
            require(all(row[name] > 0 for name in ('event_us', 'sonos_intent', 'network_generation')),
                    'zero control-fault identity')
            require(not rows or row['event_us'] >= rows[-1]['event_us'], 'UART marker device time decreased')
            rows.append(row)
        offset += len(raw_line)
    return rows

def validate_recovery(rows, intent, generation, queue_id, mode, observation_end_us=None):
    if observation_end_us is not None:
        rows = [row for row in rows if row['event_us'] <= observation_end_us]
    phases = [row['phase'] for row in rows]
    for name in ('identity', 'armed', 'stall-start', 'stall-end', 'observation-error'):
        require(phases.count(name) == 1, 'missing/duplicate ' + name)
    identity, arm, start, end = [next(row for row in rows if row['phase'] == name)
                                 for name in ('identity', 'armed', 'stall-start', 'stall-end')]
    require(identity['queue_item_id'] == queue_id, 'published queue changed')
    require(all(row['sonos_intent'] == intent and row['network_generation'] == generation
                and row['command_generation'] == identity['command_generation'] for row in rows),
            'fault crossed intent, network lease or command generation')
    require(arm['event_us'] <= identity['event_us'] <= start['event_us'] < end['event_us'], 'fault phase order invalid')
    require(0 < end['event_us'] - start['event_us'] <= 7_000_000, 'read stall exceeded its bounded original deadline')
    error = next(row for row in rows if row['phase'] == 'observation-error')
    require(error['event_us'] >= end['event_us'] and error['error_kind'] == ('TimedOut' if mode == 'retention' else 'Interrupted'),
            'actual observation did not return expected deadline/cancellation error')
    after = [row['phase'] for row in rows if row['event_us'] >= end['event_us'] and row['phase'] not in ('stall-end', 'observation-error')]
    if mode == 'retention':
        require(after == ['Degraded', 'Recovering', 'Playing'], 'natural Degraded/Recovering/Playing sequence missing or terminal')
    else:
        require(after and all(phase == 'Stopped' for phase in after), 'cancellation restored playback or lacked final Stopped receipt')
    return True


def seconds(value):
    require(re.fullmatch(r'[0-9]+:[0-5][0-9]:[0-5][0-9]', value), 'invalid Sonos relative time')
    hours, minutes, secs = map(int, value.split(':'))
    return hours * 3600 + minutes * 60 + secs


def capture_gate(text):
    require(not any(term in text for term in ('Guru Meditation', 'panicked at', 'Task watchdog got triggered')),
            'UART contains panic/watchdog')
    samples = [parse_capture_sample(line, require_clean_startup=True)
               for line in text.splitlines() if 'audio: frames=' in line]
    require(len(samples) >= 2, 'need two capture samples for clean startup/steady-state gate')
    baseline = samples[0]
    for previous, sample in zip(samples, samples[1:]):
        require(all(sample[key] == baseline[key] for key in LOSS_KEYS), 'capture fault counter changed after startup')
        delta = (sample['frames'] - previous['frames']) & 0xffffffff
        require(0 < delta < 0x80000000, 'capture progress stopped or moved backward')
    return dict(samples=len(samples), first=baseline, last=samples[-1], clean_startup=True)


def listener_exports(text):
    exports = []
    for line in text.splitlines():
        if 'listener-telemetry:' not in line:
            continue
        values = {}
        for name, pattern in (('dropped', r'[0-9]+'), ('loss_occurred', r'true|false'), ('export_us', r'[0-9]+'),
                              ('observed_event_range_valid', r'true|false'), ('observed_event_min_us', r'[0-9]+'),
                              ('observed_event_max_us', r'[0-9]+')):
            fields = re.findall(r'\b' + name + r'=(\S+)', line)
            require(len(fields) == 1 and re.fullmatch(pattern, fields[0]), 'malformed listener telemetry ' + name)
            values[name] = fields[0]
        require(values['dropped'] == '0' and values['loss_occurred'] == 'false', 'lost listener telemetry export')
        receipt = dict(export_us=int(values['export_us']), range_valid=values['observed_event_range_valid'] == 'true',
                       first_us=int(values['observed_event_min_us']), last_us=int(values['observed_event_max_us']))
        require(not receipt['range_valid'] or receipt['first_us'] <= receipt['last_us'] <= receipt['export_us'],
                'malformed listener telemetry event range')
        exports.append(receipt)
    return exports


def close_batch_exported(exports, close_us):
    return any(row['range_valid'] and row['first_us'] <= close_us <= row['last_us']
               and row['export_us'] >= close_us for row in exports)


def cleanup_receipts_ready(raw, speaker, intent):
    # A close can precede its export-loss line in the same UART batch. Only
    # complete lines can establish either receipt; malformed/lost lines are terminal.
    text = raw[:raw.rfind(b'\n') + 1].decode('utf-8', errors='replace')
    exports = listener_exports(text)
    try:
        delivered = payload_receipt(text, speaker, 'mp3', intent)
    except ValueError:
        return False
    return close_batch_exported(exports, delivered['close_us'])


def route(peer, interface):
    command = ['ip', '-j', 'route', 'get', peer]
    value = json.loads(subprocess.check_output(command, text=True, timeout=3))
    require(len(value) == 1 and value[0]['dev'] == interface, 'route differs from requested interface: ' + repr(value))
    return dict(command=command, selected_interface=interface, routes=value)


class ControlTrial(Trial):
    def __init__(self, args):
        super().__init__(args)
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect)
        self.boot = args.boot_token
        self.deadline = time.monotonic() + 100
        self.initial_config = None
        self.initial_settings = None
        self.network_generation = None
        self.initial_play_accepted = False
        self.stopped_by_trial = False
        self.samples = []
        self.result = dict(passed=False, mode=args.mode, cleanup_errors=[], observation_errors=[], limitations=[
            'Quiet ADC protocol/deadline probe; no source-exact, decoded fidelity or uninterrupted sample claim.',
            'A control-thread stall expires a genuine description-read deadline; this is not an AP/network outage.',
            'One listener, one initial operator Play, read-only recovery and checked explicit Stop.',
            'URI/coordinator observations and explicit Stop are not atomic against other controllers.',
            'ELF hash/boot token are device reports, not secure attestation.',
            'Payload receipt plus position progress cannot prove new bytes at every instant.'])

    def attest(self):
        original_deadline = self.deadline
        retry_deadline = None
        try:
            while time.monotonic() < self.deadline:
                value = super().attest()
                network, settings = value['network'], value['settings']
                if network is not None:
                    require(network['connected'] is True and (self.network_generation is None
                            or network['generation'] == self.network_generation), 'board network lease changed')
                if settings is not None and self.initial_settings is not None:
                    require(settings == self.initial_settings, 'board capture/encoder settings changed')
                missing = [key for key in ('network', 'settings') if value[key] is None]
                if not missing:
                    return value
                # These fields use nonblocking snapshots. Preserve the gap, then
                # reacquire instead of substituting the earlier lease/settings.
                self.event('metadata-gap', missing=missing, served_device_us=value['served_device_us'])
                self.result['metadata_gaps'] = self.result.get('metadata_gaps', 0) + 1
                if retry_deadline is None:
                    retry_deadline = min(original_deadline, time.monotonic() + 2)
                    self.deadline = retry_deadline
                time.sleep(min(.5, max(0, self.deadline - time.monotonic())))
            raise TimeoutError('required metrics metadata unavailable before deadline')
        finally:
            self.deadline = original_deadline

    def action(self, name, **fields):
        require(name == 'discover' or (name == 'play' and not self.initial_play_accepted),
                'driver only permits discovery and one initial Play; use checked_stop for Stop')
        self.event('control', action=name, fields=fields)
        result = super().action(name, **fields)
        if name == 'play':
            require(result.get('accepted') is True, 'initial Play rejected')
            self.initial_play_accepted = True
        return result

    def wait(self, predicate, seconds=30):
        # Discovery/cleanup inherited paths also respect the <=2 Hz sampling cap.
        until = min(self.deadline, time.monotonic() + seconds)
        while time.monotonic() < until:
            started = time.monotonic()
            value = self.api()
            require(value['state'] != 'error', 'board controller error: ' + repr(value))
            if predicate(value):
                return value
            time.sleep(max(0, .5 - (time.monotonic() - started)))
        raise TimeoutError('board status did not settle')

    def uart(self):
        raw = self.args.uart.read_bytes()
        rows = markers(raw)
        self.result['uart_markers'] = rows
        self.result['uart_bytes_observed'] = len(raw)
        return raw, rows

    def validate(self, status, metrics):
        require(status['config'] == self.initial_config, 'saved Sonos settings changed')
        require(status['active_format'] == 'mp3', 'active encoder changed')
        require(metrics['network']['connected'] is True and metrics['network']['generation'] == self.network_generation,
                'board network lease changed')
        require(metrics['settings']['recognition_enabled'] is False, 'recognition enabled')
        require(metrics['settings'] == self.initial_settings, 'board capture/encoder settings changed')
        loss = metrics['listener_loss']
        require(loss is None or (loss['dropped'] == 0 and loss['loss_occurred'] is False),
                'listener telemetry loss invalidates owner evidence')
        require(len(metrics['listeners']) == 2 and sum(row['state'] != 'idle' for row in metrics['listeners']) <= 1,
                'single-listener phase observed multiple/unavailable listeners')

    def snapshot(self, play_started):
        row = dict(sample_started_monotonic=time.monotonic(), since_play=time.monotonic() - play_started)
        try:
            row['metrics_before'] = self.attest()
            row['board'] = self.api()
            self.validate(row['board'], row['metrics_before'])
            self.coordinator()
            row['media'] = self.soap('GetMediaInfo')
            row['transport'] = self.soap('GetTransportInfo')
            row['position'] = self.soap('GetPositionInfo')
            row['metrics_after'] = self.attest()
            self.validate(row['board'], row['metrics_after'])
            uri = row['media']['CurrentURI']
            if self.owned_uri is None:
                confirmed = current_boot_uri(uri, self.args.host, row['transport']['CurrentTransportState'],
                                             row['position']['TrackURI'], self.initial_play_accepted,
                                             row['metrics_after']['boot_token'] == self.boot)
                if confirmed is not None:
                    row['queue'] = self.api('/api/sonos/queue/v2.1/itemWindow')
                    require(row['queue']['items'][0]['track']['mediaUrl'] == uri, 'published queue URL differs')
                    fields = urllib.parse.parse_qs(urllib.parse.urlsplit(uri).query)
                    require(int(fields['network_generation'][0]) == self.network_generation, 'URL lease differs')
                    self.owned_uri, self.intent = confirmed
                    self.queue = row['queue']
                    self.result.update(owned_uri=uri, intent=self.intent, queue=self.queue)
                else:
                    row['startup_pending'] = True
            else:
                require(uri == self.owned_uri and row['position']['TrackURI'] == self.owned_uri, 'speaker source/track changed')
                if not self.stopped_by_trial:
                    row['queue'] = self.api('/api/sonos/queue/v2.1/itemWindow')
                    require(row['queue'] == self.queue, 'owned queue changed')
            require(row['board']['state'] != 'error', 'board controller reported error')
            if self.stopped_by_trial:
                require(row['board']['state'] == 'stopped' and row['transport']['CurrentTransportState'] == 'STOPPED',
                        'explicit Stop was later restored')
            row['position_seconds'] = seconds(row['position']['RelTime'])
        except Exception as error:
            row['error'] = repr(error)
            self.result['observation_errors'].append(dict(since_play=row['since_play'], error=repr(error)))
            raise
        finally:
            self.event('snapshot', **row)
            self.samples.append(row)
        return row

    def checked_stop(self, during_stall=False):
        require(self.owned_uri is not None, 'no current-boot owned URI; refusing Stop')
        metrics = self.attest()
        status = self.api()
        self.validate(status, metrics)
        self.coordinator()
        require(self.soap('GetMediaInfo')['CurrentURI'] == self.owned_uri, 'foreign current URI; refusing Stop')
        require(self.soap('GetPositionInfo')['TrackURI'] == self.owned_uri, 'foreign current track; refusing Stop')
        # Bound the command between two device clock reads. If either falls outside
        # the real stall interval the cancellation trial remains failed/incomplete.
        before = self.attest()
        if during_stall:
            _, rows = self.uart()
            require(any(row['phase'] == 'stall-start' for row in rows) and not any(row['phase'] == 'stall-end' for row in rows),
                    'read stall ended before explicit Stop could be issued')
        self.event('control', action='stop', checked_uri=self.owned_uri, during_stall=during_stall)
        response = self.api(body=dict(action='stop'))
        require(response.get('accepted') is True, 'checked explicit Stop rejected')
        after = self.attest()
        self.result['stop_receipt'] = dict(before_device_us=before['served_device_us'], after_device_us=after['served_device_us'],
                                           response=response, during_stall=during_stall)
        self.stopped_by_trial = True
        def stopped(value):
            self.attest()
            self.coordinator()
            require(self.soap('GetMediaInfo')['CurrentURI'] == self.owned_uri, 'current URI changed during Stop completion')
            transport = self.soap('GetTransportInfo')
            return value['state'] == 'stopped' and transport['CurrentTransportState'] == 'STOPPED' and transport['CurrentTransportStatus'] == 'OK'
        self.wait(stopped, 15)
        self.play_sent = False

    def setup(self):
        self.before = self.preflight()
        self.initial_config = dict(self.before['config'])
        metrics = self.attest()
        self.initial_settings = dict(metrics['settings'])
        self.network_generation = metrics['network']['generation']
        self.validate(self.before, metrics)
        require(all(row['state'] == 'idle' for row in metrics['listeners']), 'initial listeners are not idle')
        self.result.update(before=self.before, identity=dict(image=self.image, boot=self.boot), network_generation=self.network_generation)
        self.event('route', peer=self.args.host, receipt=route(self.args.host, self.args.interface))
        self.discover()
        self.identify()
        self.event('route', peer=self.speaker, receipt=route(self.speaker, self.args.interface))
        remote = self.soap('GetTransportInfo')
        require(remote['CurrentTransportState'] == 'STOPPED' and remote['CurrentTransportStatus'] == 'OK', 'selected Sonos must start STOPPED/OK')
        self.result.update(initial_remote=remote, initial_uri=self.soap('GetMediaInfo')['CurrentURI'],
                           selected_identity=dict(player_id=self.initial_config['player_id'], speaker=self.speaker))

    def run(self):
        self.setup()
        if self.args.mode in ('prepare', 'verify'):
            self.result['observation_complete'] = True
            return
        _, rows = self.uart()
        require(not rows, 'fresh boot already contains fault markers')
        self.result['preplay_device_us'] = self.attest()['served_device_us']
        self.play_sent = True
        self.action('play')
        started = time.monotonic()
        startup_deadline = min(self.deadline, started + 20)
        while self.owned_uri is None:
            self.snapshot(started)
            require(time.monotonic() < startup_deadline, 'current-boot Play did not establish ownership within 20s')
            if self.owned_uri is None:
                time.sleep(1)
        self.result['current_boot_play_receipt'] = True
        if self.args.mode == 'cancel':
            # Poll only the growing UART until the trigger. Long SOAP snapshots
            # cannot consume the five-second cancellation window.
            while True:
                require(time.monotonic() < self.deadline, 'fault was not reached before acquisition deadline')
                _, rows = self.uart()
                if any(row['phase'] == 'stall-start' for row in rows):
                    self.checked_stop(during_stall=True)
                    break
                time.sleep(.05)
            until = min(self.deadline, time.monotonic() + 12)
            while time.monotonic() < until:
                tick = time.monotonic()
                self.snapshot(started)
                self.uart()
                time.sleep(max(0, 1 - (time.monotonic() - tick)))
        else:
            recovered_at = None
            while True:
                require(time.monotonic() < self.deadline, 'fault/recovery not reached before acquisition deadline')
                tick = time.monotonic()
                self.snapshot(started)
                _, rows = self.uart()
                if any(row['phase'] == 'Playing' for row in rows):
                    recovered_at = recovered_at or time.monotonic()
                    if time.monotonic() - recovered_at >= 8:
                        break
                time.sleep(max(0, 1 - (time.monotonic() - tick)))
        self.result['observation_end_us'] = self.attest()['served_device_us']
        self.result['observation_complete'] = True

    def evaluate(self, raw, rows):
        validate_recovery(rows, self.intent, self.network_generation, self.queue['queueVersion'], self.args.mode,
                          observation_end_us=self.result['observation_end_us'])
        identity = next(row for row in rows if row['phase'] == 'identity')
        require(identity['event_us'] >= self.result['preplay_device_us'] + 15_000_000, 'fault identity precedes established current-boot Play')
        start, end = [next(row['event_us'] for row in rows if row['phase'] == name) for name in ('stall-start', 'stall-end')]
        self.result['actual_stall_duration_us'] = end - start
        valid = [row for row in self.samples if not row.get('error') and not row.get('startup_pending') and row.get('position_seconds') is not None]
        pre = [row for row in valid if row['metrics_after']['served_device_us'] < start]
        require(pre, 'no fully bracketed PLAYING/position observation before fault')
        if self.args.mode == 'retention':
            post = [row for row in valid if row['metrics_before']['served_device_us'] > end and row['board']['state'] == 'playing'
                    and row['transport']['CurrentTransportState'] == 'PLAYING']
            require(post and max(row['position_seconds'] for row in post) > max(row['position_seconds'] for row in pre),
                    'media position did not advance across recovery')
        else:
            receipt = self.result['stop_receipt']
            require(start < receipt['before_device_us'] <= receipt['after_device_us'] < end,
                    'explicit Stop was not bracketed wholly inside the active read stall')
            require(valid[-1]['metrics_before']['served_device_us'] >= end + 10_000_000,
                    'less than ten seconds of stopped observations after stall completion')
        text = raw.decode('utf-8', errors='replace')
        self.result['capture_gate'] = capture_gate(text)
        exports = listener_exports(text)
        require(exports, 'missing listener telemetry export')
        delivered = payload_receipt(text, self.speaker, 'mp3', self.intent)
        require(close_batch_exported(exports, delivered['close_us']), 'no loss-free telemetry export covers the matching owner close batch')
        require(delivered['event_us'] < start and delivered['frames_sent'] > 0 and delivered['bytes_sent'] > 0,
                'original media owner had no positive delivery')
        if self.args.mode == 'retention':
            require(end < self.result['observation_end_us'] <= delivered['departure_us'] <= delivered['close_us'],
                    'original media owner did not span the entire recovery observation')
        else:
            require(receipt['before_device_us'] <= delivered['close_us'] <= self.result['after']['metrics']['served_device_us'],
                    'original media owner close was not bounded after explicit Stop')
        joins = re.findall(r'\bstream: ' + re.escape(self.speaker) + r':[0-9]+ joined /stream\.mp3[^\n]*sonos_intent='
                           + str(self.intent) + r'\b', raw.decode('utf-8', errors='replace'))
        require(len(joins) == 1, 'original intent acquired a replacement listener')
        self.result['payload_receipt'] = delivered

    def cleanup(self):
        self.deadline = time.monotonic() + 25
        signal.setitimer(signal.ITIMER_REAL, 25)
        self.result['cleanup_complete'] = False
        try:
            if self.play_sent:
                self.checked_stop()
            if self.before is not None:
                final = self.preflight()
                metrics = self.attest()
                self.validate(final, metrics)
                require(all(row['state'] == 'idle' for row in metrics['listeners']), 'listeners remain active after Stop')
                self.result['after'] = dict(board=final, metrics=metrics)
                self.event('route-final', peer=self.args.host, receipt=route(self.args.host, self.args.interface))
                self.event('route-final', peer=self.speaker, receipt=route(self.speaker, self.args.interface))
            self.result['cleanup_complete'] = True
            if self.result.get('observation_complete') and not self.result.get('error') and not self.result['observation_errors']:
                if self.args.mode in ('retention', 'cancel'):
                    drain_until = min(self.deadline, time.monotonic() + 3)
                    while True:
                        raw, rows = self.uart()
                        ends = [row['event_us'] for row in rows if row['phase'] == 'stall-end']
                        require(len(ends) == 1, 'missing/duplicate stall-end during cleanup')
                        if cleanup_receipts_ready(raw, self.speaker, self.intent):
                            break
                        require(time.monotonic() < drain_until, 'matching checked-close payload and loss-free close-batch receipts did not drain within 3s')
                        time.sleep(.1)
                    (self.args.out / 'uart-observed.log').write_bytes(raw)
                    self.result['uart_sha256'] = hashlib.sha256(raw).hexdigest()
                    self.evaluate(raw, rows)
                self.result['passed'] = True
        except BaseException as error:
            self.result['cleanup_errors'].append(repr(error))
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            self.result['samples'] = len(self.samples)
            # Keep the latest raw UART even when parsing or cleanup failed.
            if self.args.uart is not None and not (self.args.out / 'uart-observed.log').exists():
                try:
                    (self.args.out / 'uart-observed.log').write_bytes(self.args.uart.read_bytes())
                except OSError as error:
                    self.result['cleanup_errors'].append('retain UART: ' + repr(error))
                    self.result['passed'] = False
            (self.args.out / 'result.json').write_text(json.dumps(self.result, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description='THROWAWAY real board control-read deadline gate: <=100s acquisition + <=25s cleanup. Only discovery, one initial Play and checked explicit Stop; no source/mixer/volume/UART device access. prepare/verify inspect stopped MP3/Medium/off and discover the actual coordinator. retention/cancel require parent-owned fresh-reset growing UART and matching ELF/boot. No AP-loss or fidelity claim.')
    parser.add_argument('host', help='Numeric board IPv4')
    parser.add_argument('--mode', choices=('prepare', 'verify', 'retention', 'cancel'), required=True)
    parser.add_argument('--image', required=True, type=Path, help='Exact currently running ELF')
    parser.add_argument('--boot-token', required=True, help='Current fresh boot token from parent acquisition')
    parser.add_argument('--interface', required=True, help='Actual expected route device for board and Sonos')
    parser.add_argument('--uart', type=Path, help='Growing UART file; parent alone owns serial/reset')
    parser.add_argument('--out', required=True, type=Path, help='New private evidence directory')
    args = parser.parse_args()
    ipaddress.IPv4Address(args.host)
    require(re.fullmatch('[0-9a-f]{32}', args.boot_token), 'invalid expected boot token')
    require(args.mode in ('prepare', 'verify') or args.uart is not None, 'trial requires fresh-boot UART file')
    os.umask(0o077)
    trial = ControlTrial(args)
    def interrupt(signum, frame):
        raise InterruptedError(f'signal {signum}')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGALRM):
        signal.signal(sig, interrupt)
    signal.setitimer(signal.ITIMER_REAL, 100)
    try:
        trial.run()
    except BaseException as error:
        trial.result['error'] = repr(error)
    finally:
        trial.cleanup()
    print(json.dumps(dict(passed=trial.result['passed'], mode=args.mode, errors=trial.result.get('error'),
                          cleanup_errors=trial.result['cleanup_errors'], evidence=str(args.out))))
    return 0 if trial.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
