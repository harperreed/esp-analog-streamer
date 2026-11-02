#!/usr/bin/env python3
# ABOUTME: Summarizes private MP3 prototype receipts without publishing identities or raw UART.
# ABOUTME: Run after hardware trials; rerun to include newly completed acceptance/network receipts.
import argparse
import hashlib
import json
from pathlib import Path
import re

ROOT = Path.cwd()
HARDWARE = ROOT / 'target/mp3-stream/hardware'
DROP = re.compile(r'host|peer|(?:^|_)ip(?:$|_)|mac|uuid|(?:^queue$|queue_(?:id|name))|identity|destination|(?:^source$|source_(?:file|path))|path|uri|url|hashes|directory|uart_log', re.I)
PAIRS = re.compile(r'\b([a-z][a-z0-9_]*)=(-?\d+(?:\.\d+)?|true|false)\b')

def sanitize_error(value):
    value = re.sub(r'https?://\S+', '[redacted-url]', str(value))
    value = re.sub(r'(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])', '[redacted-ip]', value)
    value = re.sub(r'\b(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}\b', '[redacted-mac]', value)
    value = re.sub(r'\b(?:uuid:|RINCON_)[\w:-]+', '[redacted-identity]', value, flags=re.I)
    value = re.sub(r'(?<!\w)/(?:[^\s\]\[\"\']+)', '[redacted-path]', value)
    return value[:2000]

def numeric(value):
    if value is None or isinstance(value, (bool, int, float)):
        return value
    if isinstance(value, list):
        return [item for child in value if (item := numeric(child)) is not None]
    if isinstance(value, dict):
        result = {key: item for key, child in value.items()
                if re.fullmatch(r'[A-Za-z][A-Za-z0-9_-]*', key) and not DROP.search(key)
                and (item := numeric(child)) is not None}
        if isinstance(value.get('decoder_warnings'), list):
            result.pop('decoder_warnings', None)
            result['decoder_warning_count'] = len(value['decoder_warnings'])
            result['duration_estimation_warning_count'] = sum('Estimating duration from bitrate' in warning for warning in value['decoder_warnings'])
        for key in ['error', 'decode_error', 'telemetry_error']:
            if value.get(key):
                result[key] = sanitize_error(value[key])
        return result
    return None

def verdict(data):
    command_failure = any(isinstance(value, int) and not isinstance(value, bool)
        and value != 0 and ("-" in key or key in ("capture", "sonos", "network"))
        for key, value in data.items())
    if data.get('passed') is False or data.get('error') or command_failure:
        return 'failed'
    return 'passed' if data.get('passed') is True else 'recorded'


def receipt(path):
    raw = path.read_bytes()
    data = json.loads(raw)
    result = {'private_receipt_sha256': hashlib.sha256(raw).hexdigest(), 'metrics': numeric(data)}
    result['status'] = verdict(data)
    if data.get('error'):
        result['error'] = sanitize_error(data['error'])
    source = data.get('source')
    if isinstance(source, dict):
        result['source_snapshot_hashes'] = {key: value for key, value in source.items()
            if key.endswith('_sha256') and isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value)}
    for key, value in data.items():
        if key.endswith('_sha256') and isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value):
            result[key] = value
    return result

def logs(directory):
    records = {'stream_encode': [], 'stream_listener': [], 'wifi': [], 'capture': [], 'stack_overflow': False, 'allocation_failure': False, 'failed_alloc_sizes': [], 'private_log_sha256': []}
    seen = set()
    for path in sorted(directory.rglob('*.log')):
        raw = path.read_bytes()
        # Only UART/boot logs can contribute metrics. Never copy arbitrary message text.
        if path.name not in ('uart.log', 'mp3-boot.log', 'serial.log'):
            continue
        log_hash = hashlib.sha256(raw).hexdigest()
        records['private_log_sha256'].append(log_hash)
        for line in raw.decode(errors='replace').splitlines():
            if re.search(r'stack overflow|stack canary', line, re.I):
                records['stack_overflow'] = True
            allocation = re.search(r'alloc\((\d+) bytes\) failed', line)
            if allocation:
                records['allocation_failure'] = True
                records['failed_alloc_sizes'].append(int(allocation[1]))
            stamp = re.search(r'\b[IWE] \((\d+)\)', line)
            if '::net: wifi:' in line:
                fields = {key: int(value, 16 if value.startswith('0x') else 10)
                    for key, value in re.findall(r'\b(protocol|power_save|channel|rssi|negotiated_phy)=(-?0x[0-9a-fA-F]+|-?\d+)', line)}
                if fields:
                    records['wifi'].append({'boot_ms': int(stamp[1]) if stamp else None,
                                            'private_uart_sha256': log_hash, **fields})
            marker = next((item for item in ('stream-encode:', 'stream-listener:', 'audio:')
                           if item in line and (item != 'audio:' or 'audio: frames=' in line)), None)
            if marker is None:
                continue
            stamp = re.search(r'\b[IWE] \((\d+)\)', line)
            fields = dict((key, json.loads(value)) for key, value in PAIRS.findall(line.split(marker, 1)[1]) if not DROP.search(key))
            item = {'boot_ms': int(stamp[1]) if stamp else None, 'private_uart_sha256': log_hash, **fields}
            identity = (marker, json.dumps(item, sort_keys=True))
            if identity in seen:
                continue
            seen.add(identity)
            records[{'stream-encode:': 'stream_encode', 'stream-listener:': 'stream_listener', 'audio:': 'capture'}[marker]].append(item)
    return records

def trial(name, directory, elf):
    out = {'name': name, 'status': 'not_run', 'elf_sha256': hashlib.sha256(elf.read_bytes()).hexdigest() if elf.exists() else None}
    if not directory.exists():
        return out
    primary = directory / 'result.json'
    if not primary.exists():
        primary = directory / 'initial-result.json'
    if primary.exists():
        out['runner'] = receipt(primary)
        out['elf_sha256'] = out['runner'].get('mp3_elf_sha256', out['elf_sha256'])
        out['status'] = out['runner']['status']
    out['acceptance_receipts'] = []
    for filename in ['capture/result.json', 'sonos-result.json']:
        path = directory / filename
        if path.exists():
            out['acceptance_receipts'].append({'kind': filename.split('/')[0].replace('-result.json', ''), **receipt(path)})
    summaries = sorted((directory / 'two-listener').glob('*/summary.json'))
    if summaries:
        out['soaks'] = [receipt(path) for path in summaries]
        out['status'] = out['soaks'][-1]['status']
    elif (directory / 'two-listener.log').exists():
        out['status'] = 'incomplete'
    out.update(logs(directory))
    if any(item['status'] == 'failed' for item in out['acceptance_receipts']) or out['stack_overflow']:
        out['status'] = 'failed'
    return out

def tcp_telemetry(path):
    result = {'private_receipt_sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'samples': 0, 'listeners': []}
    with path.open() as source:
        for line in source:
            data = json.loads(line)
            result['samples'] += 1
            for index, listener in enumerate(data.get('listeners', [])):
                if index == len(result['listeners']):
                    result['listeners'].append({})
                fields = {**listener.get('counters', {}), **{key: listener[key] for key in ['seconds_since_recv', 'tcp_info_length', 'app_bytes', 'recv_calls'] if key in listener}}
                for key, number in fields.items():
                    bounds = result['listeners'][index].setdefault(key, {'minimum': number, 'maximum': number, 'last': number})
                    bounds['minimum'] = min(bounds['minimum'], number)
                    bounds['maximum'] = max(bounds['maximum'], number)
                    bounds['last'] = number
    return result


def extract():
    definitions = [('initial', HARDWARE, 'mp3.elf'), ('retry', HARDWARE/'retry', 'mp3.elf'),
        ('read-diagnostic', HARDWARE/'read-diagnostic', 'read-diagnostic.elf'),
        ('inline-control', HARDWARE/'inline-control', 'inline-control.elf'),
        ('boxed-two-listener', HARDWARE/'boxed-two-listener', 'boxed.elf'),
        ('yield-eight', HARDWARE/'yield-eight', 'yield-eight.elf'),
        ('bounded-retry', HARDWARE/'bounded-retry', 'bounded-retry.elf'),
        ('stack-eight', HARDWARE/'stack-eight', 'stack-eight.elf'),
        ('priority-eight-stack-eight', HARDWARE/'priority-eight-stack-eight', 'priority-eight-stack-eight.elf'),
        ('listener-metrics', HARDWARE/'listener-metrics', 'listener-metrics.elf')]
    result = {'schema_version': 1, 'date': '2026-10-09',
        'bounds': ['Reported counters and runtime are observations, not worst-case guarantees.',
                   'Harness capture snapshots may age after failure; preserved age fields describe this limit.',
                   'Decode counts do not prove capture content continuity or Sonos receiver behavior.',
                   'Original failed trials remain separate from later diagnostics.'], 'trials': []}
    for name, directory, elf in definitions:
        if name == 'initial':
            # Root holds all later trial subdirectories; select only the initial UARTs.
            entry = trial(name, directory, HARDWARE/elf)
            own_logs = logs(HARDWARE/'two-listener')
            entry.update(own_logs)
        else:
            entry = trial(name, directory, HARDWARE/elf)
        result['trials'].append(entry)
    result['acceptance'] = {}
    for name in ['acceptance', 'network', 'transport', 'flac-regression']:
        directory = HARDWARE/name
        data = {'status': 'not_run'}
        if directory.exists():
            paths = sorted(directory.rglob('*result.json'))
            data = {'status': 'incomplete' if not paths else 'recorded', 'receipts': [{'kind': path.parent.name if path.parent.name in ('capture', 'captures', 'slow', 'tcp') else 'sonos' if path.name == 'sonos-result.json' else 'runner', **receipt(path)} for path in paths], **logs(directory)}
            if any(item['status'] == 'failed' for item in data['receipts']):
                data['status'] = 'failed'
            elif any(item['status'] == 'passed' for item in data['receipts']):
                data['status'] = 'passed'
            if name == 'transport':
                data['checks'] = {}
                for check in ['slow', 'tcp']:
                    path = directory / check / 'result.json'
                    entry = receipt(path) if path.exists() else {'status': 'not_run'}
                    if check == 'tcp' and path.exists():
                        raw = json.loads(path.read_text())
                        listeners = raw.get('listeners', [])
                        entry['whole_prefix_decode_valid'] = bool(listeners) and all(item.get('prefix_decode', {}).get('decoded_samples', 0) > 0 and not item.get('decode_error') for item in listeners)
                        entry['capture_without_eof_or_error'] = raw.get('passed', False)
                        entry['realtime_status'] = 'failed' if any(item.get('eof') or item.get('error') for item in listeners) else 'not_established'
                        entry['verdict_scope'] = 'Whole-prefix decode validity and capture completion are distinct from sustained real-time delivery.'
                        telemetry = directory / 'tcp' / 'tcp-info.jsonl'
                        if telemetry.exists():
                            entry['telemetry'] = tcp_telemetry(telemetry)
                    data['checks'][check] = entry
        result['acceptance'][name] = data
    return result

def self_test():
    secret = 'failure http://192.0.2.1/a /home/private/input.wav aa:bb:cc:dd:ee:ff uuid:secret RINCON_secret'
    cleaned = sanitize_error(secret)
    assert not any(item in cleaned for item in ['192.0.2.1', '/home/private', 'aa:bb:cc', 'uuid:secret', 'RINCON_secret'])
    assert numeric({'host': 'private', 'queue_identity': 123, 'frames': 42, 'nested': {'path': '/private', 'passed': False}}) == {'frames': 42, 'nested': {'passed': False}}
    assert numeric({'peer': 192.168, 'elapsed_us': 7}) == {'elapsed_us': 7}
    assert verdict({'capture': 1, 'config_restored': True}) == 'failed'
    assert verdict({'passed': True, 'network': 1}) == 'failed'
    assert verdict({'frames': 99, 'passed': True}) == 'passed'
    print('evidence sanitizer self-test passed')

def main():
    parser = argparse.ArgumentParser(description=__doc__ or 'Extract sanitized MP3 hardware evidence; no hardware access.')
    parser.add_argument('--root', type=Path, default=Path.cwd(), help='Repository root; defaults to current directory')
    parser.add_argument('--output', type=Path, help='Output JSON; defaults to repository audit evidence')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    global ROOT, HARDWARE
    ROOT = args.root.resolve()
    HARDWARE = ROOT / 'target/mp3-stream/hardware'
    if args.output is None:
        args.output = ROOT / 'docs/audit-evidence/2026-10-09/mp3-stream.json'
    if args.self_test:
        self_test()
        return
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(extract(), indent=2)+'\n')
    print('Sanitized evidence written; missing receipts remain not_run/incomplete.')
if __name__ == '__main__':
    main()
