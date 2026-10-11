#!/usr/bin/env python3
# ABOUTME: Exercises real board HTTP Sonos controls with bounded front-jack audio and UART evidence.
# ABOUTME: Observes SOAP without writes, attests image/boot, and restores idle MP3/Medium.
import argparse
import hashlib
import ipaddress
import json
import re
import signal
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import wave
from pathlib import Path
from xml.etree import ElementTree as ET

LIMIT = 131072
FIXTURE_HASH = '027accd2d3a6b52bb7a80587c7e567ce85509aa02e7cff0f0ae7a447216959b6'


def require(value, message):
    if not value:
        raise ValueError(message)


def same_peer(base, reference):
    url = urllib.parse.urljoin(base, reference)
    origin, target = map(urllib.parse.urlsplit, (base, url))
    ipaddress.IPv4Address(target.hostname)
    require(target.scheme == origin.scheme == 'http' and target.hostname == origin.hostname
            and target.port == origin.port == 1400 and not target.username and not target.password
            and not target.fragment, 'service endpoint left selected numeric peer')
    return url


def stream_identity(uri, host, fmt, buffer):
    url = urllib.parse.urlsplit(uri)
    fields = urllib.parse.parse_qs(url.query, keep_blank_values=True)
    require(url.scheme == 'http' and url.hostname == host and url.port == 8000
            and url.path == '/stream.' + fmt and not url.username and not url.password
            and not url.fragment, 'unexpected native stream URL')
    require(fields.get('buffer') == [buffer], 'stream buffer differs')
    for name in ('network_generation', 'sonos_intent'):
        values = fields.get(name, [])
        require(len(values) == 1 and re.fullmatch('[0-9]+', values[0])
                and 0 < int(values[0]) <= 2**64 - 1, 'invalid ' + name)
    return int(fields['sonos_intent'][0])


def verify_attestation(value, image, boot=None):
    require(value['build_elf_sha256'] == image, 'running ELF differs from supplied image')
    require(isinstance(value['boot_token'], str) and re.fullmatch('[0-9a-f]{32}', value['boot_token']), 'invalid boot token')
    require(boot is None or value['boot_token'] == boot, 'board rebooted during trial')


def payload_receipt(raw, host, fmt, intent):
    joins = re.findall(r'\bstream: ' + re.escape(host) + r':([0-9]+) joined /stream\.' + fmt
                       + r' \([12] of 2\).*?event_us=([0-9]+) owner_id=([0-9]+) sonos_intent='
                       + str(intent) + r'\b', raw)
    require(joins, 'matching Sonos intent has no UART join')
    receipts = []
    for port, timestamp, owner in joins:
        peer = re.escape(host + ':' + port)
        # Departure lacks owner_id. Require a unique connection and a matching owner close,
        # then use device timestamps to bind its metrics without trusting UART arrival order.
        all_joins = re.findall(r'\bstream: ' + peer + r' joined [^\n]*', raw)
        if len(all_joins) != 1:
            continue
        closes = re.findall(r'\bstream: ' + peer
                            + r' socket closed[^\n]*?event_us=([0-9]+) owner_id='
                            + owner + r' sonos_intent=' + str(intent) + r'\b', raw)
        if len(closes) != 1:
            continue
        lines = re.findall(r'\bstream-listener: peer=' + peer + r'\b[^\n]*', raw)
        for line in lines:
            fields = dict(re.findall(r'\b(frames_sent|bytes_sent|event_us)=([0-9]+)\b', line))
            delivered_at = int(fields.get('event_us', -1))
            if (int(timestamp) <= delivered_at <= int(closes[0])
                    and int(fields.get('frames_sent', 0)) > 0 and int(fields.get('bytes_sent', 0)) > 0):
                receipts.append(dict(owner_id=int(owner), event_us=int(timestamp),
                                     departure_us=delivered_at, close_us=int(closes[0]), port=int(port),
                                     frames_sent=int(fields['frames_sent']), bytes_sent=int(fields['bytes_sent'])))
    require(receipts, 'matching Sonos owner has no positive UART delivery receipt')
    return max(receipts, key=lambda row: row['bytes_sent'])


class CleanupDeadline(TimeoutError):
    """Stop control cleanup while retaining owned resource teardown."""


def cleanup_remaining(deadline, cap):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise CleanupDeadline('cleanup deadline exhausted')
    return min(cap, remaining)


def cleanup_attempt(errors, name, deadline, operation):
    cleanup_remaining(deadline, 1)
    try:
        return operation()
    except (CleanupDeadline, InterruptedError):
        raise
    except Exception as error:
        errors.append(name + ': ' + repr(error))


def confirmed_cleanup_uri(owned_uri, observed_uri):
    require(owned_uri is not None, 'uncertain Play has no confirmed owned URI; refusing Stop')
    require(observed_uri == owned_uri, 'Sonos current URI changed; refusing Stop')
    return owned_uri


def confirmed_observation_uri(owned_uri, uri, transport, track_uri, source_live):
    require(owned_uri is None or uri == owned_uri, 'Sonos current URI changed')
    require(transport == 'PLAYING', 'Sonos is not PLAYING')
    require(track_uri == uri, 'Sonos track URI differs')
    require(source_live, 'front-jack source ended during trial')
    return uri


def stop_owned_source(source, deadline):
    if source is None or source.poll() is not None:
        return
    source.terminate()
    try:
        source.wait(cleanup_remaining(deadline, 1))
    except (subprocess.TimeoutExpired, CleanupDeadline):
        source.kill()
        source.wait(cleanup_remaining(deadline, 1))


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError('HTTP redirect refused')


class Trial:
    def __init__(self, args):
        self.args = args
        args.out.mkdir(parents=True, exist_ok=False)
        self.result = dict(passed=False, cleanup_errors=[], phases=[], limitations=[
            'SOAP observations and board Stop are not atomic against other controllers.',
            'PLAYING, position and UART payload receipts do not prove audible or sample-exact continuity.',
            'This gate does not validate contention, recovery, or complete Kata 7vhq.',
            'ELF hash and boot token are device reports, not secure-boot attestation.'])
        self.deadline = time.monotonic() + 150
        self.opener = urllib.request.build_opener(NoRedirect)
        self.image = hashlib.sha256(args.image.read_bytes()).hexdigest()
        self.boot = None
        self.before = None
        self.owned_uri = None
        self.play_sent = False
        self.source = self.port = self.reader = None
        self.uart_stop = threading.Event()
        self.uart_errors = []
        self.sequence = 0

    def event(self, kind, **fields):
        self.sequence += 1
        row = dict(sequence=self.sequence, monotonic=time.monotonic(), kind=kind, **fields)
        with (self.args.out / 'events.jsonl').open('a') as log:
            log.write(json.dumps(row) + '\n')
        return self.sequence

    def request(self, url, body=None, headers=None):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('phase deadline exhausted')
        request = urllib.request.Request(url, data=body, headers=headers or {})
        try:
            response = self.opener.open(request, timeout=min(4, remaining))
        except urllib.error.HTTPError as error:
            with error:
                raw = error.read(LIMIT + 1)
                seq = self.event('http-error', url=url, status=error.code, truncated=len(raw) > LIMIT)
                (self.args.out / f'{seq:04d}-error-body').write_bytes(raw)
            raise
        with response:
            raw = response.read(LIMIT + 1)
            require(len(raw) <= LIMIT, 'HTTP body exceeds 128 KiB')
            seq = self.event('http', url=url, status=response.status, bytes=len(raw))
            (self.args.out / f'{seq:04d}-body').write_bytes(raw)
            return raw

    def api(self, path='/api/sonos', body=None):
        raw = self.request('http://' + self.args.host + path,
                           None if body is None else json.dumps(body).encode(),
                           {'Content-Type': 'application/json'})
        value = json.loads(raw)
        self.event('api', path=path, request=body, response=value)
        return value

    def action(self, name, **fields):
        self.attest()
        status = self.api()
        require(status['config']['player_id'] == self.before['config']['player_id'], 'selected destination changed')
        require(status['config']['autoplay'] is False, 'autoplay changed')
        return self.api(body=dict(action=name, **fields))

    def attest(self):
        value = self.api('/api/metrics')
        verify_attestation(value, self.image, self.boot)
        self.event('attestation', image=self.image, boot=value['boot_token'], served_device_us=value['served_device_us'])
        return value

    def preflight(self):
        status = self.api()
        config = status['config']
        require(status['state'] == 'stopped' and status['active_format'] == config['output_format'] == 'mp3'
                and config['buffer_preset'] == 'med' and config['autoplay'] is False
                and config['player_id'] and status['buffer_available'], 'requires stopped MP3/Medium, selected destination, buffers and autoplay off')
        require(self.api('/api/state')['shazam_enabled'] is False, 'recognition must be off')
        return status

    def identify(self):
        selected = self.before['config']['player_id']
        groups = [row for row in self.before['groups'] if row['player_id'] == selected and row['is_leader']]
        require(len(groups) == 1, 'selected coordinator missing/ambiguous in board discovery')
        self.speaker = str(ipaddress.IPv4Address(groups[0]['host']))
        self.base = f'http://{self.speaker}:1400/xml/device_description.xml'
        tree = ET.fromstring(self.request(self.base))
        require(tree.find('.//{*}UDN').text == 'uuid:' + selected, 'live device UDN differs')
        self.services = {}
        for service in tree.findall('.//{*}service'):
            typ = service.find('{*}serviceType').text
            if typ in ('urn:schemas-upnp-org:service:AVTransport:1', 'urn:schemas-upnp-org:service:ZoneGroupTopology:1'):
                require(typ not in self.services, 'ambiguous service')
                self.services[typ] = same_peer(self.base, service.find('{*}controlURL').text)
        require(len(self.services) == 2, 'required services missing')
        self.coordinator()

    def soap(self, name):
        # This harness deliberately has no media, volume, queue or SOAP Stop write path.
        require(name in ('GetZoneGroupState', 'GetMediaInfo', 'GetTransportInfo', 'GetPositionInfo'), 'SOAP action is not read-only')
        service = 'ZoneGroupTopology' if name == 'GetZoneGroupState' else 'AVTransport'
        typ = f'urn:schemas-upnp-org:service:{service}:1'
        fields = '' if service == 'ZoneGroupTopology' else '<InstanceID>0</InstanceID>'
        envelope = f'<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:{name} xmlns:u="{typ}">{fields}</u:{name}></s:Body></s:Envelope>'
        tree = ET.fromstring(self.request(self.services[typ], envelope.encode(),
                                          {'Content-Type': 'text/xml; charset="utf-8"', 'SOAPAction': f'"{typ}#{name}"'}))
        require(tree.find('.//{*}Fault') is None, 'SOAP Fault')
        values = {node.tag.split('}')[-1]: node.text or '' for node in tree.iter() if len(node) == 0}
        self.event('soap-' + name, values=values)
        return values

    def coordinator(self):
        tree = ET.fromstring(self.soap('GetZoneGroupState')['ZoneGroupState'])
        selected = self.before['config']['player_id']
        groups = [node for node in tree.findall('.//{*}ZoneGroup') if node.get('Coordinator') == selected]
        require(len(groups) == 1, 'selected player ceased to be coordinator')
        members = [node for node in groups[0].findall('{*}ZoneGroupMember') if node.get('UUID') == selected]
        require(len(members) == 1, 'coordinator identity ambiguous')
        same_peer(self.base, members[0].get('Location'))

    def wait(self, predicate, seconds=30):
        until = min(self.deadline, time.monotonic() + seconds)
        while time.monotonic() < until:
            value = self.api()
            require(value['state'] != 'error', 'board controller error: ' + repr(value))
            if predicate(value):
                return value
            time.sleep(.3)
        raise TimeoutError('board status did not settle')

    def settings(self, fmt, buffer):
        self.action('format', format=fmt)
        self.action('buffer', preset=buffer)
        expected = dict(self.before['config'], output_format=fmt, buffer_preset=buffer)
        return self.wait(lambda status: status['config'] == expected, 10)

    def observe_owned(self, fmt, buffer):
        self.attest()
        self.coordinator()
        uri = self.soap('GetMediaInfo')['CurrentURI']
        intent = stream_identity(uri, self.args.host, fmt, buffer)
        transport = self.soap('GetTransportInfo')['CurrentTransportState']
        track_uri = self.soap('GetPositionInfo')['TrackURI']
        source_live = self.source.poll() is None
        self.owned_uri = confirmed_observation_uri(self.owned_uri, uri, transport, track_uri, source_live)
        return intent

    def stop(self):
        confirmed_cleanup_uri(self.owned_uri, self.owned_uri)
        self.attest()
        self.coordinator()
        confirmed_cleanup_uri(self.owned_uri, self.soap('GetMediaInfo')['CurrentURI'])
        self.action('stop')
        self.wait(lambda status: status['state'] == 'stopped', 15)
        require(self.soap('GetTransportInfo')['CurrentTransportState'] == 'STOPPED', 'Sonos did not stop')
        self.play_sent = False
        self.owned_uri = None

    def capture_uart(self):
        try:
            offset = 0
            with (self.args.out / 'uart.log').open('wb') as log, (self.args.out / 'uart-arrivals.jsonl').open('w') as arrivals:
                while not self.uart_stop.is_set():
                    data = self.port.read(4096)
                    if data:
                        require(offset + len(data) <= 2 * 1024 * 1024, 'UART exceeds 2 MiB')
                        log.write(data)
                        log.flush()
                        arrivals.write(json.dumps(dict(monotonic=time.monotonic(), offset=offset, bytes=len(data))) + '\n')
                        arrivals.flush()
                        offset += len(data)
        except BaseException as error:
            self.uart_errors.append(repr(error))

    def run(self):
        import serial
        pre_open = self.preflight()
        self.attest()
        self.port = serial.Serial(port=None, baudrate=115200, timeout=.2)
        self.port.dtr = self.port.rts = False
        self.port.port = self.args.serial
        self.port.open()
        self.reader = threading.Thread(target=self.capture_uart, daemon=True)
        self.reader.start()
        # Serial open can reset the ESP32; the authoritative trial boot starts afterward.
        until = min(self.deadline, time.monotonic() + 20)
        while True:
            try:
                self.before = self.preflight()
                self.boot = self.attest()['boot_token']
                break
            except (OSError, urllib.error.URLError):
                if time.monotonic() >= until:
                    raise
                time.sleep(.5)
        require(self.before['config'] == pre_open['config'], 'configuration changed across UART open')
        self.result['before'] = self.before
        self.result['identity'] = dict(image=self.image, boot=self.boot)
        self.identify()
        require(self.soap('GetTransportInfo')['CurrentTransportState'] == 'STOPPED', 'speaker must start STOPPED')
        for control, switch, gains in [('Headphone', 'on', 0), ('Line Out', 'off', 0), ('Master', 'on', 1), ('PCM', None, 2)]:
            text = subprocess.check_output(['amixer', '-c', 'PCH', 'sget', control], text=True, timeout=4)
            self.event('mixer-read', control=control, output=text)
            channels = [line for line in text.splitlines() if re.search(r'^\s*(Front Left|Front Right|Mono): Playback', line)]
            require(channels, 'mixer channels missing')
            require(not switch or all(f'[{switch}]' in line for line in channels), 'mixer switch mismatch')
            require(not gains or len(channels) == gains and all('[0.00dB]' in line for line in channels), 'mixer gain mismatch')
        require(self.args.fixture.name == 'mono-noise-180s.wav', 'unexpected fixture name')
        with wave.open(str(self.args.fixture)) as fixture:
            require((fixture.getframerate(), fixture.getnchannels(), fixture.getsampwidth(), fixture.getnframes()) == (48000, 2, 2, 8640000), 'unexpected fixture format')
        require(hashlib.sha256(self.args.fixture.read_bytes()).hexdigest() == FIXTURE_HASH, 'fixture hash differs')
        self.result['fixture_sha256'] = FIXTURE_HASH
        with (self.args.out / 'aplay.log').open('wb') as log:
            self.source = subprocess.Popen(['aplay', '-D', 'plughw:CARD=PCH,DEV=0', str(self.args.fixture.resolve())], stdout=log, stderr=subprocess.STDOUT)
        for fmt, buffer in [('flac', 'med'), ('mp3', 'high')]:
            self.settings(fmt, buffer)
            self.wait(lambda status: status['active_format'] == fmt and status['state'] == 'stopped', 10)
            self.coordinator()
            self.play_sent = True
            self.action('play')
            status = self.wait(lambda value: value['state'] == 'playing', 30)
            require(status['active_format'] == fmt, 'active format mismatch')
            intent = self.observe_owned(fmt, buffer)
            if fmt == 'flac':
                self.settings('mp3', 'high')
                status = self.api()
                require(status['active_format'] == 'flac', 'saved format changed active encoder')
                self.observe_owned(fmt, buffer)
            positions = []
            until = time.monotonic() + 15
            while time.monotonic() < until:
                require(self.api()['state'] == 'playing', 'board left Playing')
                self.observe_owned(fmt, buffer)
                position = self.soap('GetPositionInfo')['RelTime']
                parts = position.split(':')
                require(len(parts) == 3 and all(part.isdigit() for part in parts), 'invalid Sonos position')
                hours, minutes, seconds = map(int, parts)
                require(minutes < 60 and seconds < 60, 'invalid Sonos position range')
                positions.append(hours * 3600 + minutes * 60 + seconds)
                time.sleep(.8)
            require(len(positions) >= 2 and all(b >= a for a, b in zip(positions, positions[1:]))
                    and positions[-1] - positions[0] >= 10, 'Sonos position did not progress')
            uri = self.owned_uri
            self.stop()
            until = min(self.deadline, time.monotonic() + 6)
            while True:
                raw = (self.args.out / 'uart.log').read_text(errors='replace')
                try:
                    receipt = payload_receipt(raw, self.speaker, fmt, intent)
                    break
                except ValueError:
                    if time.monotonic() >= until:
                        raise
                    time.sleep(.3)
            self.result['phases'].append(dict(format=fmt, buffer=buffer, uri=uri, intent=intent, payload=receipt, sustained_seconds=15, positions=positions))
        self.result['passed'] = True

    def cleanup(self):
        started = time.monotonic()
        end = started + 45
        # Reserve three seconds for owned resource teardown and the final local receipt.
        self.deadline = end - 3
        errors = self.result['cleanup_errors']
        candidate_passed = self.result['passed']
        self.result['passed'] = False
        self.result['cleanup_complete'] = False
        # Preserve a fail-closed receipt even if the alarm interrupts final artifact I/O.
        def alarm(signum, frame):
            raise CleanupDeadline('45-second cleanup alarm')
        signal.signal(signal.SIGALRM, alarm)
        signal.setitimer(signal.ITIMER_REAL, 45)
        def attempt(name, operation):
            return cleanup_attempt(errors, name, self.deadline, operation)
        try:
            (self.args.out / 'result.json').write_text(json.dumps(self.result, indent=2) + '\n')
            # Our source must stop even when another controller makes media cleanup unsafe.
            attempt('source-stop', lambda: stop_owned_source(self.source, self.deadline))
            if self.before is not None and self.boot is not None:
                if self.play_sent:
                    # A fresh matching board URL cannot establish which Play installed it.
                    # Never adopt a URI during cleanup following an uncertain Play response.
                    attempt('board-stop', self.stop)
                def restore():
                    require(self.api()['state'] == 'stopped', 'board not stopped; refusing settings restore')
                    self.settings('mp3', 'med')
                    self.wait(lambda status: status['active_format'] == 'mp3', 10)
                    require(self.preflight()['config'] == self.before['config'], 'configuration not restored')
                    self.attest()
                attempt('restore-idle-mp3-medium', restore)
                attempt('final-image-and-boot', self.attest)
        except (CleanupDeadline, InterruptedError) as error:
            errors.append('cleanup-interrupted: ' + repr(error))
        except Exception as error:
            errors.append('cleanup-failed: ' + repr(error))
        finally:
            # A failed control step never authorizes killing another process. This object
            # holds only the Popen created by this harness, so its kill is always scoped.
            try:
                if self.source is not None and self.source.poll() is None:
                    self.source.kill()
                    self.source.wait(cleanup_remaining(end, .5))
            except Exception as error:
                errors.append('source-finalize: ' + repr(error))
            self.uart_stop.set()
            try:
                if self.reader is not None:
                    self.reader.join(cleanup_remaining(end, 1))
                    if self.reader.is_alive():
                        errors.append('UART reader did not stop')
                if self.port is not None:
                    cleanup_remaining(end, 1)
                    self.port.close()
            except Exception as error:
                errors.append('UART-finalize: ' + repr(error))
            try:
                cleanup_remaining(end, 1)
                self.result['uart_errors'] = self.uart_errors
                raw = (self.args.out / 'uart.log').read_bytes() if (self.args.out / 'uart.log').exists() else b''
                self.result['uart_sha256'] = hashlib.sha256(raw).hexdigest()
                # Initial boot following serial open is permitted; final attestation catches later resets.
                faults = re.findall(r'[^\n]*(?:Guru Meditation|CORRUPT HEAP|stack overflow|TLS connect failed|native TLS|encoder overrun|raw overrun|encoder fell behind capture|out of memory|allocation failed|failed to allocate|ENOMEM)[^\n]*', raw.decode(errors='replace'), re.I)
                self.result['uart_faults'] = faults
                self.result['cleanup_elapsed_seconds'] = time.monotonic() - started
                self.result['cleanup_complete'] = True
                self.result['passed'] = candidate_passed and not (errors or self.uart_errors or faults)
                cleanup_remaining(end, 1)
                pending = self.args.out / 'result.pending.json'
                pending.write_text(json.dumps(self.result, indent=2) + '\n')
                cleanup_remaining(end, 1)
                pending.replace(self.args.out / 'result.json')
            except (CleanupDeadline, InterruptedError) as error:
                errors.append('receipt-finalize: ' + repr(error))
                self.result['passed'] = False
                self.result['cleanup_complete'] = False
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)


def main():
    parser = argparse.ArgumentParser(description='Real board HTTP Sonos FLAC/MP3 acceptance; front-jack only, no mixer/volume writes. Main <=150s; cleanup <=45s. Requires stopped MP3/Medium, buffers, selected coordinator, autoplay/recognition off. Evidence does not prove sample continuity or contention/recovery.')
    parser.add_argument('host', help='Numeric board IPv4')
    parser.add_argument('--serial', required=True, help='Exclusive UART device')
    parser.add_argument('--image', required=True, type=Path, help='Exact running ELF, checked against device metrics')
    parser.add_argument('--fixture', required=True, type=Path, help='Known mono-noise-180s.wav front-jack fixture')
    parser.add_argument('--out', required=True, type=Path, help='New evidence directory; must not exist')
    args = parser.parse_args()
    ipaddress.IPv4Address(args.host)
    import os
    os.umask(0o077)
    trial = Trial(args)
    def interrupt(signum, frame):
        raise InterruptedError(f'signal {signum}')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGALRM):
        signal.signal(sig, interrupt)
    signal.setitimer(signal.ITIMER_REAL, 150)
    try:
        trial.run()
    except BaseException as error:
        trial.result['error'] = repr(error)
    finally:
        trial.cleanup()
    print(json.dumps(dict(passed=trial.result['passed'], cleanup_errors=trial.result['cleanup_errors'], evidence=str(args.out))))
    return 0 if trial.result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
