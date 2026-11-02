#!/usr/bin/env python3
# ABOUTME: Drives one ownership-safe resume probe cycle on the real board and Sonos: Play, the probe image's
# ABOUTME: once-per-boot listener close (or an operator app Stop), bounded observation, checked Stop; verdicts are pure.
import argparse
import hashlib
import ipaddress
import json
import os
import re
import select
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from xml.etree import ElementTree as ET

from encoder_fault_check import listener_end_category, listener_event_us, read_uart_lines
from sonos_http_check import (CleanupDeadline, NoRedirect, Trial, cleanup_attempt, confirmed_cleanup_uri, require,
                              stop_owned_source)

REPO = Path(__file__).resolve().parent.parent
SCHEMES = ('plain', 'broadcast')
PLAY_SECONDS, HOLD_SECONDS, CLOSE_WAIT_SECONDS, CLOSE_OBSERVE_SECONDS, STOP_OBSERVE_SECONDS = 30, 20, 40, 90, 60
APP_STOP_OBSERVE_SECONDS, OPERATOR_SECONDS, STOP_WAIT_SECONDS, BOOT_SECONDS = 60, 120, 15, 90
MAIN_SECONDS, CLEANUP_SECONDS = 480, 45
UART_LIMIT = 4 * 1024 * 1024
ANSI = re.compile(r'\x1b\[[0-9;]*m')
# event_us is matched loosely here so the shared validating parser rejects a malformed value instead of
# the line silently going unparsed.
JOIN = re.compile(r'stream: (\S+) joined (/stream\.\w+) \((\d+) of \d+\) event_us=\S+ owner_id=(\d+) sonos_intent=(\d+) role=(\w+)')
LEFT = re.compile(r'stream: (\S+) left after [0-9.]+ s: (.*?) listener_stack_free_bytes=\d+ event_us=\S+')
CLOSED = re.compile(r'stream: (\S+) socket closed in (\d+) ms; releasing slot \((\d+) held\) event_us=\S+ owner_id=(\d+) sonos_intent=(\d+)')
PASSIVE_FIELDS = ('phase', 'owner_id', 'sonos_intent', 'network_generation', 'operator_epoch', 'first_payload_us',
                  'frames_sent', 'bytes_sent')
FAULTS = re.compile(r'[^\n]*(?:Guru Meditation|CORRUPT HEAP|stack overflow|panicked at|Task watchdog got triggered'
                    r'|encoder overrun|raw overrun|encoder fell behind capture|out of memory|allocation failed'
                    r'|failed to allocate|ENOMEM)[^\n]*', re.I)


def flag(text):
    require(text in ('true', 'false'), 'probe receipt boolean must be true or false')
    return text == 'true'


def tagged_identity(uri, host):
    """Return (scheme, network_generation, sonos_intent) for our tagged MP3 stream URL on host; refuse anything else.

    Sonos may echo a different scheme than the board sent, so the scheme is recorded, not required.
    """
    require('://' in uri, 'tagged stream URL needs an authority')
    url = urllib.parse.urlsplit(uri)
    require(url.hostname == host and url.port == 8000 and url.path == '/stream.mp3' and not url.username
            and not url.password and not url.fragment, 'unexpected tagged stream URL')
    fields = urllib.parse.parse_qs(url.query, keep_blank_values=True)
    for name in ('network_generation', 'sonos_intent'):
        values = fields.get(name, [])
        require(len(values) == 1 and re.fullmatch('[0-9]+', values[0]) and 0 < int(values[0]) <= 2**64 - 1,
                'invalid ' + name)
    return url.scheme, int(fields['network_generation'][0]), int(fields['sonos_intent'][0])


def tagged_match(sent, observed):
    """Same tagged URL ignoring the scheme (host, port, path and query exact), as the probe image's same_tagged_url."""
    sent, observed = sent.split('://', 1), observed.split('://', 1)
    return len(sent) == len(observed) == 2 and sent[1] == observed[1]


def listener_events(lines, path='/stream.mp3'):
    """Parse the firmware's stream join/left/closed lines and the probe image's receipts into dicts."""
    events = []
    for raw in lines:
        line = ANSI.sub('', raw).rstrip('\r')
        join, left, closed = JOIN.search(line), LEFT.search(line), CLOSED.search(line)
        if join and join.group(2) == path:
            events.append(dict(kind='join', peer=join.group(1), host=join.group(1).rsplit(':', 1)[0], path=path,
                               held=int(join.group(3)), event_us=listener_event_us(line), owner_id=int(join.group(4)),
                               sonos_intent=int(join.group(5)), role=join.group(6)))
        elif left:
            events.append(dict(kind='left', peer=left.group(1), host=left.group(1).rsplit(':', 1)[0],
                               event_us=listener_event_us(line), reason=left.group(2),
                               category=listener_end_category(left.group(2))))
        elif closed:
            events.append(dict(kind='closed', peer=closed.group(1), host=closed.group(1).rsplit(':', 1)[0],
                               event_us=listener_event_us(line), elapsed_ms=int(closed.group(2)),
                               held=int(closed.group(3)), owner_id=int(closed.group(4)), sonos_intent=int(closed.group(5))))
        elif 'passive-media-close:' in line:
            fields = dict(re.findall(r'\b([a-z_]+)=([0-9]+)\b', line))
            require(all(name in fields for name in PASSIVE_FIELDS), 'incomplete passive-media-close receipt')
            events.append(dict(kind='passive-close', event_us=listener_event_us(line),
                               **{name: int(fields[name]) for name in PASSIVE_FIELDS}))
        elif 'probe-broadcast:' in line:
            requested = re.search(r'probe-broadcast: play requested broadcast=(\S+)', line)
            enabled = re.search(r'probe-broadcast: enabled=(\S+) uri=(\S*)', line)
            echoed = re.search(r'probe-broadcast: echoed uri=(\S*)', line)
            if requested:
                events.append(dict(kind='play-requested', broadcast=flag(requested.group(1))))
            elif enabled:
                events.append(dict(kind='broadcast', enabled=flag(enabled.group(1)), uri=enabled.group(2)))
            elif echoed:
                events.append(dict(kind='echoed', uri=echoed.group(1)))
            else:
                raise ValueError('unrecognized probe-broadcast receipt')
    return events


def cycle_verdict(samples, uart_events, owned_uri, intent, generation, speaker, close=None, app_stop=None, checked_stop=None):
    """Pure per-cycle verdict on the device clock. Each marker carries device_us; each sample carries device_us.

    Pass for A: the failure-stop shows a status other than OK and the Stops show OK, on our unchanged URI.
    Pass for B: Sonos re-requests the tagged URL after the close (a tagged UART join on this intent with the
    network generation still current) and reports PLAYING again; count and timing are recorded either way.
    """
    valid = [row for row in samples if 'error' not in row]

    def after(marker):
        return [row for row in valid if row['device_us'] > marker['device_us']]

    def ours(row):
        return owned_uri is not None and tagged_match(owned_uri, row['current_uri'])

    def delay(row, marker):
        return (row['device_us'] - marker['device_us']) / 1e6

    def stopped_status(marker):
        if marker is None:
            return None
        for row in after(marker):
            if row['transport_state'] == 'STOPPED':
                return dict(status=row['transport_status'], state='STOPPED', uri_ours=ours(row), uri=row['current_uri'],
                            delay_seconds=delay(row, marker))
        return None

    verdict = dict(status_after_close=None, status_after_app_stop=stopped_status(app_stop),
                   status_after_stop=stopped_status(checked_stop), uri_retained_after_close=None, uris_after_close=None,
                   readmissions_after_close=None, playing_again=None, owner_departure=None, pass_a=None, pass_b=None,
                   sample_errors=len(samples) - len(valid),
                   scheme_observed=owned_uri.split('://', 1)[0] if owned_uri and '://' in owned_uri else None)
    stop_ok = verdict['status_after_stop'] is not None and verdict['status_after_stop']['status'] == 'OK' \
        and verdict['status_after_stop']['uri_ours']
    if close is not None:
        window = [row for row in after(close) if checked_stop is None or row['device_us'] <= checked_stop['device_us']]
        for row in window:
            if row['transport_status'] != 'OK':
                verdict['status_after_close'] = dict(status=row['transport_status'], state=row['transport_state'],
                                                     uri_ours=ours(row), uri=row['current_uri'], delay_seconds=delay(row, close))
                break
        verdict['uris_after_close'] = list(dict.fromkeys(row['current_uri'] for row in window))
        if any(not ours(row) for row in window):
            verdict['uri_retained_after_close'] = False
        elif any(row['transport_state'] == 'STOPPED' for row in window):
            verdict['uri_retained_after_close'] = True
        joins = [event for event in uart_events if event['kind'] == 'join' and event['host'] == speaker
                 and event['event_us'] > close['device_us']]
        tagged = [event for event in joins if event['sonos_intent'] == intent]
        generation_current = None
        if tagged:
            witnesses = [row for row in valid if row['device_us'] > tagged[0]['event_us']
                         and row.get('network_generation') is not None]
            generation_current = bool(witnesses) and witnesses[0]['network_generation'] == generation
        verdict['readmissions_after_close'] = dict(
            count=len(joins), first_delay_seconds=(joins[0]['event_us'] - close['device_us']) / 1e6 if joins else None,
            intent_match=all(event['sonos_intent'] == intent for event in joins) if joins else None,
            generation_current=generation_current, intents=[event['sonos_intent'] for event in joins])
        # Sonos keeps reporting PLAYING from its buffer right after the close, so a resume needs either an
        # intervening non-PLAYING sample or a sample taken after a tagged re-admission.
        seen_non_playing = False
        for row in window:
            if row['transport_state'] != 'PLAYING':
                seen_non_playing = True
                continue
            after_readmission = bool(tagged) and row['device_us'] > tagged[0]['event_us']
            if ours(row) and (seen_non_playing or after_readmission):
                verdict['playing_again'] = dict(delay_seconds=delay(row, close), device_us=row['device_us'],
                                                after_readmission=after_readmission)
                break
        closes = [event for event in uart_events if event['kind'] == 'closed' and event['host'] == speaker
                  and event['sonos_intent'] == intent and event['event_us'] >= close['device_us']]
        if closes:
            owner = closes[0]
            lefts = [event for event in uart_events if event['kind'] == 'left' and event['peer'] == owner['peer']
                     and event['event_us'] <= owner['event_us']]
            verdict['owner_departure'] = dict(event_us=owner['event_us'], departure_us=lefts[-1]['event_us'] if lefts else None,
                                              owner_id=owner['owner_id'], reason=lefts[-1]['reason'] if lefts else None,
                                              delay_seconds=(owner['event_us'] - close['device_us']) / 1e6)
        failure = verdict['status_after_close']
        verdict['pass_a'] = bool(failure and failure['state'] == 'STOPPED' and failure['uri_ours']
                                 and verdict['uri_retained_after_close'] is True and stop_ok)
        verdict['pass_b'] = bool(tagged and verdict['readmissions_after_close']['intent_match'] and generation_current
                                 and verdict['playing_again'] and verdict['playing_again']['after_readmission'])
    elif app_stop is not None:
        app = verdict['status_after_app_stop']
        verdict['pass_a'] = bool(app and app['status'] == 'OK' and app['uri_ours'] and stop_ok)
    return verdict


def await_operator(stream, seconds):
    """Bounded wait for one operator line on stream; None when nothing arrives in time."""
    ready, _, _ = select.select([stream], [], [], max(0, seconds))
    if not ready:
        return None
    line = stream.readline()
    return line.rstrip('\n') if line else None


class ResumeProbe(Trial):
    def __init__(self, args):
        super().__init__(args)
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect)
        self.deadline = time.monotonic() + MAIN_SECONDS
        self.intent = self.generation = self.speaker = self.serial = None
        self.samples = []
        self.uart_events = []
        self.uart_lines_seen = 0
        self.handles = []
        self.markers = dict(play=None, close=None, app_stop=None, checked_stop=None)
        self.result = dict(passed=False, cycle=args.cycle, scheme=args.scheme, cleanup_errors=[], sample_errors=[],
                           uart_errors=[], sonos_software_version='not fetched', broadcast_receipts=[], verdict=None,
                           limitations=[
                               'SOAP observations and the checked Stop are sequential, not atomic against other controllers.',
                               'The listener close is the probe image\'s once-per-boot injection; the driver records its receipt and the owner departure, not how the socket closed.',
                               'Re-admission counts come from UART joins; no join does not prove no HTTP/TCP attempt (no AP or packet capture).',
                               'The status discriminator is measured Sonos behavior on the recorded software version, not a contract.',
                               'ELF hash and boot token are device reports, not secure-boot attestation.',
                               'One cycle per run and per boot; pass_a and pass_b are per-cycle evidence, and the cross-cycle verdict belongs to the evidence pair.'])

    def stream_host(self):
        return urllib.parse.urlsplit('//' + self.args.host).hostname

    def bracket(self, operation):
        """Run operation between two device-clock reads so UART events can be ordered against it."""
        before = self.attest()
        sent = time.monotonic()
        response = operation()
        after = self.attest()
        return dict(sent_monotonic=sent, device_us=before['served_device_us'], after_device_us=after['served_device_us'],
                    response=response)

    def board_action(self, path, name):
        self.attest()
        status = self.api()
        require(status['config']['player_id'] == self.before['config']['player_id'], 'selected destination changed')
        require(status['config']['autoplay'] is False, 'autoplay changed')
        return self.api(path, body=dict(action=name))

    def poll_uart(self):
        path = self.args.out / 'uart.log'
        try:
            require(not path.exists() or path.stat().st_size <= UART_LIMIT, 'UART exceeds 4 MiB')
            lines = read_uart_lines(path)
            require(len(lines) >= self.uart_lines_seen, 'UART log shrank')
            for event in listener_events(lines[self.uart_lines_seen:]):
                self.uart_events.append(event)
                self.event('uart-listener', listener=event)
            self.uart_lines_seen = len(lines)
        except Exception as error:
            self.result['uart_errors'].append(repr(error))

    def receipts(self, *kinds):
        return [event for event in self.uart_events if event['kind'] in kinds]

    def sample(self, phase):
        row = dict(sample_started_monotonic=time.monotonic(), phase=phase)
        try:
            metrics = self.attest()
            row['device_us'] = metrics['served_device_us']
            row['listeners_active'] = sum(listener['state'] != 'idle' for listener in metrics['listeners'])
            row['network_generation'] = None if metrics.get('network') is None else metrics['network']['generation']
            board = self.api()
            row['board_state'] = board['state']
            require(board['config']['player_id'] == self.before['config']['player_id']
                    and board['config']['autoplay'] is False, 'selected destination or autoplay changed')
            row['current_uri'] = self.soap('GetMediaInfo')['CurrentURI']
            transport = self.soap('GetTransportInfo')
            row['transport_state'] = transport['CurrentTransportState']
            row['transport_status'] = transport['CurrentTransportStatus']
            position = self.soap('GetPositionInfo')
            row['track_uri'] = position['TrackURI']
            row['rel_time'] = position['RelTime']
        except Exception as error:
            row['error'] = repr(error)
            self.result['sample_errors'].append(dict(phase=phase, error=repr(error)))
        finally:
            self.poll_uart()
            self.event('sample', **row)
            self.samples.append(row)
        return row

    def tick(self, phase):
        started = time.monotonic()
        row = self.sample(phase)
        time.sleep(max(0, min(1 - (time.monotonic() - started), self.deadline - time.monotonic())))
        return row

    def observe(self, phase, seconds):
        until = time.monotonic() + seconds
        while time.monotonic() < min(until, self.deadline):
            self.tick(phase)
            if self.args.cycle == 'app-stop':
                require(not self.passive_closes(), 'passive close fired during an app-stop cycle; use an image without close.patch')
        require(time.monotonic() >= until, phase + ' observation cut short by the run deadline')

    def passive_closes(self):
        return [event for event in self.receipts('passive-close') if event['phase'] == 1 and event['sonos_intent'] == self.intent]

    def check_settings(self, status, metrics):
        require(status['config']['autoplay'] is False, 'autoplay must be off')
        require(self.api('/api/state')['shazam_enabled'] is False, 'recognition must be off')
        if metrics.get('settings') is None:
            self.event('metadata-gap', missing=['settings'], served_device_us=metrics['served_device_us'])
        else:
            require(metrics['settings']['recognition_enabled'] is False, 'recognition must be off')

    def software_version(self):
        node = ET.fromstring(self.request(self.base)).find('.//{*}softwareVersion')
        return node.text if node is not None and node.text else 'not present in device description'

    def start_serial(self):
        raw = (self.args.out / 'uart.log').open('wb')
        errors = (self.args.out / 'serial.log').open('wb')
        self.handles += [raw, errors]
        # scripts/serial-log resets the board on open; the authoritative boot starts afterward.
        self.serial = subprocess.Popen([str(REPO / 'scripts/serial-log'), str(int(self.deadline - time.monotonic()) + 60)],
                                       stdout=raw, stderr=errors, start_new_session=True,
                                       env=dict(os.environ, ESPFLASH_PORT=self.args.serial))
        self.event('serial-log', pid=self.serial.pid, port=self.args.serial)

    def await_board(self, pre_open):
        until = min(self.deadline, time.monotonic() + BOOT_SECONDS)
        while True:
            require(self.serial.poll() is None, 'UART collector exited during boot; see serial.log')
            try:
                self.before = self.preflight()
                metrics = self.attest()
                break
            except (OSError, urllib.error.URLError, ValueError, KeyError):
                require(time.monotonic() < until, 'board not ready within 90 s of the serial reset')
                time.sleep(1)
        self.boot = metrics['boot_token']
        # One injection per boot: every cycle needs a boot that no earlier cycle used.
        require(self.boot != pre_open['boot'], 'serial open did not reset the board; boot token unchanged')
        require(self.boot != self.args.previous_boot, 'boot token equals the previous cycle\'s boot')
        require(self.before['config'] == pre_open['config'], 'configuration changed across the serial reset')
        self.check_settings(self.before, metrics)
        self.result.update(before=self.before, identity=dict(image=self.image, boot=self.boot),
                           previous_boot=self.args.previous_boot, pre_reset_boot=pre_open['boot'])

    def play(self):
        broadcast = self.args.scheme == 'broadcast'
        path = '/api/sonos?broadcast=1' if broadcast else '/api/sonos'
        self.play_sent = True
        self.markers['play'] = self.bracket(lambda: self.board_action(path, 'play'))
        self.event('play', path=path, **self.markers['play'])
        require(self.markers['play']['response'].get('accepted') is True, 'Play rejected')
        until = min(self.deadline, time.monotonic() + PLAY_SECONDS)
        host = self.stream_host()
        while self.owned_uri is None:
            row = self.tick('play-wait')
            self.result['broadcast_receipts'] = self.receipts('play-requested', 'broadcast', 'echoed')
            enabled = [event['enabled'] for event in self.receipts('broadcast')]
            require(broadcast or not any(enabled), 'broadcast receipt observed on a plain cycle')
            if 'error' not in row and row['board_state'] == 'playing' and row['transport_state'] == 'PLAYING' \
                    and row['track_uri'] == row['current_uri'] and (not broadcast or any(enabled)):
                try:
                    scheme, generation, intent = tagged_identity(row['current_uri'], host)
                except ValueError:
                    continue  # Sonos may still echo the previous source until the board installs ours.
                # Tags repeat across boots, so ownership needs this boot's admission of the same intent.
                joins = [event for event in self.receipts('join') if event['host'] == self.speaker
                         and event['sonos_intent'] == intent and event['event_us'] >= self.markers['play']['device_us']]
                if joins:
                    self.owned_uri, self.intent, self.generation = row['current_uri'], intent, generation
                    self.result.update(owned_uri=self.owned_uri, intent=intent, network_generation=generation,
                                       scheme_observed=scheme, owner_join=joins[0])
            require(self.owned_uri is not None or time.monotonic() < until,
                    'PLAYING on our tagged URI with a current-boot admission not observed within 30 s')

    def await_close(self):
        """The probe image closes the owning listener by itself ~10 s after its first payload; bracket its receipt."""
        until = min(self.deadline, time.monotonic() + CLOSE_WAIT_SECONDS)
        while True:
            self.tick('await-close')
            receipts = self.passive_closes()
            if receipts:
                marker = dict(device_us=receipts[0]['event_us'], phase=1, receipt=receipts[0], observed_monotonic=time.monotonic())
                self.markers['close'] = marker
                self.event('passive-close', **marker)
                return marker
            if time.monotonic() >= until:
                raise TimeoutError('passive-media-close phase=1 receipt for this intent not observed within the close budget')

    def checked_stop(self):
        confirmed_cleanup_uri(self.owned_uri, self.owned_uri)
        self.coordinator()
        confirmed_cleanup_uri(self.owned_uri, self.soap('GetMediaInfo')['CurrentURI'])
        marker = self.bracket(lambda: self.board_action('/api/sonos', 'stop'))
        self.markers['checked_stop'] = marker
        self.event('checked-stop', **marker)
        # Local intent revocation can precede the worker's remote SOAP Stop: poll the same speaker.
        until = min(self.deadline, time.monotonic() + STOP_WAIT_SECONDS)
        while True:
            row = self.tick('stop-wait')
            if 'error' not in row:
                require(row['current_uri'] == self.owned_uri, 'current URI changed during Stop completion')
                if row['board_state'] == 'stopped' and row['transport_state'] == 'STOPPED':
                    break
            if time.monotonic() >= until:
                raise TimeoutError('remote STOPPED not observed within the Stop budget')
        self.play_sent = False
        return marker

    def app_stop(self):
        before = self.attest()
        print('Press Stop in the Sonos app now, then press Enter here.', flush=True)
        marker = dict(sent_monotonic=time.monotonic(), device_us=before['served_device_us'])
        self.event('operator-prompt', **marker)
        until = min(self.deadline, marker['sent_monotonic'] + OPERATOR_SECONDS)
        line = None
        while line is None:
            self.tick('app-stop-wait')
            line = await_operator(sys.stdin, .05)
            require(line is not None or time.monotonic() < until, 'operator did not confirm the app Stop in time')
        marker.update(confirmed_monotonic=time.monotonic(), after_device_us=self.attest()['served_device_us'], operator=line)
        self.markers['app_stop'] = marker
        self.event('operator-confirmed', **marker)
        return marker

    def run(self):
        pre_open = self.preflight()
        pre_open['boot'] = self.attest()['boot_token']
        self.check_settings(pre_open, self.attest())
        self.start_serial()
        self.await_board(pre_open)
        self.discover()
        self.identify()
        self.result['sonos_software_version'] = self.software_version()
        remote = self.soap('GetTransportInfo')
        require(remote['CurrentTransportState'] == 'STOPPED', 'speaker must start STOPPED')
        self.result['initial_remote'] = remote
        self.play()
        if self.args.cycle == 'close':
            self.await_close()
            self.observe('after-close', CLOSE_OBSERVE_SECONDS)
            self.checked_stop()
            self.observe('after-stop', STOP_OBSERVE_SECONDS)
        else:
            self.observe('hold', HOLD_SECONDS)
            self.app_stop()
            self.observe('after-app-stop', APP_STOP_OBSERVE_SECONDS)
            self.checked_stop()
            self.observe('after-stop', 10)
        self.result['observation_complete'] = True

    def verdict(self):
        markers = {name: self.markers[name] for name in ('close', 'app_stop', 'checked_stop')}
        return cycle_verdict(self.samples, self.uart_events, self.owned_uri, self.intent, self.generation, self.speaker, **markers)

    def final_check(self):
        status = self.preflight()
        metrics = self.attest()
        require(all(listener['state'] == 'idle' for listener in metrics['listeners']), 'listeners remain active after Stop')
        self.result['after'] = dict(board=status, metrics=metrics)

    def cleanup(self):
        started = time.monotonic()
        end = started + CLEANUP_SECONDS
        self.deadline = end - 3
        errors = self.result['cleanup_errors']
        candidate = bool(self.result.get('observation_complete'))
        self.result['passed'] = False
        self.result['cleanup_complete'] = False

        def alarm(signum, frame):
            raise CleanupDeadline('45-second cleanup alarm')
        signal.signal(signal.SIGALRM, alarm)
        signal.setitimer(signal.ITIMER_REAL, CLEANUP_SECONDS)

        def attempt(name, operation):
            return cleanup_attempt(errors, name, self.deadline, operation)
        try:
            self.result['verdict'] = self.verdict()
            (self.args.out / 'result.json').write_text(json.dumps(self.result, indent=2) + '\n')
            if self.before is not None and self.boot is not None:
                # Stop only what this run proved it owns; an unconfirmed Play never authorizes a Stop.
                if self.play_sent:
                    attempt('checked-stop', self.checked_stop)
                attempt('final-check', self.final_check)
        except (CleanupDeadline, InterruptedError) as error:
            errors.append('cleanup-interrupted: ' + repr(error))
        except Exception as error:
            errors.append('cleanup-failed: ' + repr(error))
        finally:
            try:
                stop_owned_source(self.serial, end)
            except Exception as error:
                errors.append('serial-finalize: ' + repr(error))
            for handle in self.handles:
                handle.close()
            try:
                self.poll_uart()
                raw = (self.args.out / 'uart.log').read_bytes() if (self.args.out / 'uart.log').exists() else b''
                faults = FAULTS.findall(raw.decode(errors='replace'))
                self.result.update(uart_sha256=hashlib.sha256(raw).hexdigest(), uart_faults=faults,
                                   uart_events=self.uart_events, markers=self.markers, samples=len(self.samples),
                                   verdict=self.verdict(), cleanup_elapsed_seconds=time.monotonic() - started,
                                   cleanup_complete=True)
                self.result['passed'] = candidate and not (errors or self.result['sample_errors']
                                                           or self.result['uart_errors'] or faults)
                pending = self.args.out / 'result.pending.json'
                pending.write_text(json.dumps(self.result, indent=2) + '\n')
                pending.replace(self.args.out / 'result.json')
            except (CleanupDeadline, InterruptedError, OSError) as error:
                errors.append('receipt-finalize: ' + repr(error))
                self.result['passed'] = False
                self.result['cleanup_complete'] = False
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=(
        'Ownership-safe Sonos resume probe, one cycle per run and per boot (<=480s plus <=45s cleanup). Resets the '
        'board over scripts/serial-log, requires stopped MP3/Medium with autoplay and recognition off, then Plays '
        '(--scheme broadcast adds ?broadcast=1 to the Play), waits for the probe image\'s once-per-boot listener '
        'close (or prompts the operator to press Stop in the Sonos app), observes, issues a checked Stop and observes '
        'again. Samples board state, read-only SOAP state/status/URIs and UART admissions every second.'))
    parser.add_argument('host', help='Numeric board IPv4')
    parser.add_argument('--serial', required=True, help='UART device for scripts/serial-log (resets the board once)')
    parser.add_argument('--image', required=True, type=Path, help='Exact running ELF, checked against device metrics')
    parser.add_argument('--out', required=True, type=Path, help='New evidence directory; must not exist')
    parser.add_argument('--cycle', required=True, choices=('close', 'app-stop'),
                        help='close: probe image closes the listener itself; app-stop: operator presses Stop in the Sonos app (needs an image without close.patch)')
    parser.add_argument('--scheme', required=True, choices=SCHEMES,
                        help='plain: ordinary Play; broadcast: Play with ?broadcast=1 so the probe image sends x-rincon-mp3radio')
    parser.add_argument('--previous-boot', help='Boot token of the previous cycle; this cycle must run on a different boot')
    args = parser.parse_args(argv)
    ipaddress.IPv4Address(args.host)
    require(args.previous_boot is None or re.fullmatch('[0-9a-f]{32}', args.previous_boot), 'invalid previous boot token')
    return args


def main(argv=None):
    args = parse_args(argv)
    os.umask(0o077)
    probe = ResumeProbe(args)

    def interrupt(signum, frame):
        raise InterruptedError(f'signal {signum}')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGALRM):
        signal.signal(sig, interrupt)
    signal.setitimer(signal.ITIMER_REAL, MAIN_SECONDS)
    try:
        probe.run()
    except BaseException as error:
        probe.result['error'] = repr(error)
    finally:
        probe.cleanup()
    verdict = probe.result.get('verdict') or {}
    print(json.dumps(dict(passed=probe.result['passed'], cycle=args.cycle, scheme=args.scheme, boot=probe.boot,
                          pass_a=verdict.get('pass_a'), pass_b=verdict.get('pass_b'), error=probe.result.get('error'),
                          cleanup_errors=probe.result['cleanup_errors'], evidence=str(args.out))))
    return 0 if probe.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
