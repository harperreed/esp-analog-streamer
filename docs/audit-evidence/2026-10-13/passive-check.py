#!/usr/bin/env python3
# ABOUTME: Records a bounded quiet-ADC passive Sonos reconnect trial without recovery writes.
# ABOUTME: Leaves protocol acceptance to matching UART close and replacement-payload evidence.
import argparse
import ipaddress
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import urllib.parse

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
from sonos_http_check import Trial, require, stream_identity


def route(peer, interface):
    command = ['ip', '-j', 'route', 'get', peer]
    value = json.loads(subprocess.check_output(command, text=True, timeout=3))
    require(len(value) == 1 and value[0]['dev'] == interface,
            'host route differs from requested interface: ' + repr(value))
    return dict(command=command, selected_interface=interface, routes=value)


def validate_sample(status, metrics, config, network_generation, uri, host):
    require(status['config'] == config, 'saved Sonos settings changed')
    require(status['active_format'] == 'mp3', 'active encoder format changed')
    require(metrics['network']['connected'] is True, 'board network disconnected')
    require(metrics['network']['generation'] == network_generation,
            'board network generation changed')
    if uri is not None:
        stream_identity(uri, host, 'mp3', 'med')
        fields = urllib.parse.parse_qs(urllib.parse.urlsplit(uri).query)
        require(int(fields['network_generation'][0]) == network_generation,
                'owned URI network generation differs')
    listeners = metrics['listeners']
    require(len(listeners) == 2, 'unexpected listener registry size')
    require(sum(row['state'] != 'idle' for row in listeners) <= 1,
            'single-listener phase observed multiple listeners')


def confirmed_startup_uri(uri, initial_uri, host, transport, track_uri):
    if uri == initial_uri:
        return None
    try:
        intent = stream_identity(uri, host, 'mp3', 'med')
    except ValueError:
        require(uri == initial_uri, 'unexpected URI during startup')
        return None
    if transport != 'PLAYING' or track_uri != uri:
        return None
    return uri, intent


class PassiveTrial(Trial):
    def __init__(self, args):
        super().__init__(args)
        self.deadline = time.monotonic() + 110
        self.boot = args.boot_token
        self.result = dict(passed=False, protocol_verdict='provisional', cleanup_errors=[],
                           observation_errors=[], limitations=[
            'Throwaway quiet-ADC protocol probe; no audio fidelity or continuity verdict.',
            'No UART acquisition: parent must prove injected checked close after >=10 seconds of complete frames.',
            'Protocol pass requires a distinct replacement owner delivering payload for the same intent and network generation.',
            'Only one Sonos listener; no second-listener contention phase.',
            'Read-only identity checks and final board Stop are not atomic against other controllers.',
            'Device image hash/boot token are reports, not secure boot attestation.'])

    def action(self, name, **fields):
        require(name in ('discover', 'play', 'stop'), 'probe refuses settings or other writes')
        self.event('control', action=name, fields=fields)
        return super().action(name, **fields)

    def run(self):
        self.before = self.preflight()
        initial_metrics = self.attest()
        self.result['before'] = self.before
        self.result['identity'] = dict(image=self.image, boot=self.boot)
        self.network_generation = initial_metrics['network']['generation']
        validate_sample(self.before, initial_metrics, self.before['config'],
                        self.network_generation, None, self.args.host)
        require(all(row['state'] == 'idle' for row in initial_metrics['listeners']),
                'fresh boot must start with no media listeners')
        self.event('host-route', peer=self.args.host,
                   receipt=route(self.args.host, self.args.interface))
        self.discover()
        self.identify()
        self.result['selected_identity'] = dict(player_id=self.before['config']['player_id'],
                                                speaker=self.speaker)
        self.event('host-route', peer=self.speaker,
                   receipt=route(self.speaker, self.args.interface))
        require(self.soap('GetTransportInfo')['CurrentTransportState'] == 'STOPPED',
                'selected Sonos must start STOPPED')
        initial_uri = self.soap('GetMediaInfo')['CurrentURI']
        self.result['initial_current_uri'] = initial_uri
        # This is the one operator-authorized initial Play, never a recovery Play.
        self.play_sent = True
        accepted = self.action('play')
        require(accepted.get('accepted') is True, 'initial Play was not accepted')
        start = time.monotonic()
        self.result['play_monotonic'] = start
        self.result['postplay_seconds'] = 60
        self.result['samples'] = 0
        next_sample = start
        while time.monotonic() < start + 60:
            row = dict(sample_started_monotonic=time.monotonic(), since_play=time.monotonic() - start)
            try:
                row['board'] = self.api()
                row['metrics'] = self.attest()
                self.coordinator()
                row['media'] = self.soap('GetMediaInfo')
                row['transport'] = self.soap('GetTransportInfo')
                row['position'] = self.soap('GetPositionInfo')
                uri = row['media']['CurrentURI']
                if self.owned_uri is None:
                    require(time.monotonic() < start + 20,
                            'exact owned track was not PLAYING within startup deadline')
                    confirmed = confirmed_startup_uri(
                        uri, initial_uri, self.args.host,
                        row['transport']['CurrentTransportState'], row['position']['TrackURI'])
                    if confirmed is not None:
                        validate_sample(row['board'], row['metrics'], self.before['config'],
                                        self.network_generation, uri, self.args.host)
                        # Pending media publication cannot authorize final Stop.
                        self.owned_uri, intent = confirmed
                        self.result.update(owned_uri=uri, intent=intent,
                                           network_generation=self.network_generation)
                    else:
                        row['startup_pending'] = True
                else:
                    require(uri == self.owned_uri, 'selected Sonos current URI changed')
                validate_sample(row['board'], row['metrics'], self.before['config'],
                                self.network_generation, self.owned_uri, self.args.host)
                if row['board']['state'] == 'error':
                    raise ValueError('board controller reported error: ' + repr(row['board']))
                require(self.api('/api/state')['shazam_enabled'] is False,
                        'recognition changed')
            except InterruptedError:
                raise
            except Exception as error:
                row['error'] = repr(error)
                self.result['observation_errors'].append(dict(since_play=row['since_play'], error=repr(error)))
            finally:
                self.event('snapshot', **row)
                self.result['samples'] += 1
            next_sample += 1
            time.sleep(max(0, min(next_sample, start + 60) - time.monotonic()))
        require(self.owned_uri is not None, 'initial Play never published confirmed owned URI')
        self.event('host-route-final', peer=self.args.host,
                   receipt=route(self.args.host, self.args.interface))
        self.event('host-route-final', peer=self.speaker,
                   receipt=route(self.speaker, self.args.interface))
        self.result['observation_complete'] = True

    def cleanup(self):
        self.deadline = time.monotonic() + 25
        signal.setitimer(signal.ITIMER_REAL, 25)
        self.result['cleanup_complete'] = False
        try:
            if self.play_sent:
                self.stop()
            if self.before is not None:
                final = self.preflight()
                require(final['config'] == self.before['config'], 'original settings differ after cleanup')
                self.attest()
                self.result['after'] = final
            self.result['cleanup_complete'] = True
        except BaseException as error:
            self.result['cleanup_errors'].append(repr(error))
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            self.result['ready_for_uart_review'] = bool(
                self.result.get('observation_complete') and self.result['cleanup_complete']
                and not self.result['observation_errors'] and not self.result.get('error'))
            # An offline owner/close/payload analysis is always needed before acceptance.
            if not self.result['ready_for_uart_review']:
                self.result['protocol_verdict'] = 'failed_or_incomplete'
            (self.args.out / 'result.json').write_text(json.dumps(self.result, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description='THROWAWAY: quiet-ADC, one-listener, passive media reconnect evidence. One initial Play, read-only snapshots ~1 Hz for 60s, checked final Stop. No recovery commands, source, mixer, volume or UART access. Main <=110s; cleanup <=25s. Never returns a protocol pass without parent UART review.')
    parser.add_argument('host', help='Numeric board IPv4')
    parser.add_argument('--image', required=True, type=Path, help='Exact running diagnostic ELF')
    parser.add_argument('--boot-token', required=True, help='Fresh boot token from parent acquisition')
    parser.add_argument('--interface', required=True, help='Expected actual host route device for board and Sonos')
    parser.add_argument('--out', required=True, type=Path, help='New private evidence directory')
    args = parser.parse_args()
    ipaddress.IPv4Address(args.host)
    require(re.fullmatch('[0-9a-f]{32}', args.boot_token), 'invalid expected boot token')
    os.umask(0o077)
    trial = PassiveTrial(args)
    def interrupt(signum, frame):
        raise InterruptedError(f'signal {signum}')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGALRM):
        signal.signal(sig, interrupt)
    signal.setitimer(signal.ITIMER_REAL, 110)
    try:
        trial.run()
    except BaseException as error:
        trial.result['error'] = repr(error)
    finally:
        trial.cleanup()
    print(json.dumps(dict(protocol_verdict=trial.result['protocol_verdict'],
                          ready_for_uart_review=trial.result['ready_for_uart_review'],
                          cleanup_errors=trial.result['cleanup_errors'], evidence=str(args.out))))
    return 0 if trial.result['ready_for_uart_review'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
