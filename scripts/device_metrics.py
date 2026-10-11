# ABOUTME: Polls one bounded numeric firmware snapshot and exports loopback Prometheus gauges.
# ABOUTME: Preserves source ages and never derives boot continuity or capture counter rates.
import argparse
import http.client
import ipaddress
import json
import math
import socket
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlsplit

SETTINGS = ('active_format', 'next_play_format', 'next_play_buffer', 'buffer_available', 'autoplay', 'recognition_enabled', 'pga_gain')

FAMILIES = ('capture', 'encoder', 'listener_loss', 'encoder_loss', 'media_progress', 'last_listener_departure')
STAGE_KEYS = {
    'accepted': ('generation', 'frames', 'observed_device_us'),
    'raw': ('generation', 'committed_frames', 'publication_device_us'),
    'encoded': ('operator_epoch', 'generation', 'sequence', 'raw_generation', 'source_start', 'source_end', 'oldest_accepted_us', 'raw_publication_us', 'production_device_us'),
}
DEPARTURE_STATS = ('elapsed_us', 'payload_elapsed_us', 'frames_sent', 'bytes_sent', 'frame_write_wall_total_us', 'frame_write_wall_max_us', 'frame_read_wait_wall_total_us', 'task_runtime_us', 'runtime_valid', 'block_samples', 'media')
NETWORK = ('generation', 'connected', 'disconnects')
CAPTURE_U32 = ('read_frames callbacks queue_overflows software_dropped_frames read_errors restarts '
               'supervisor_restarts overflow_restarts settling_discarded_frames discontinuities '
               'health_restart_requests read_service_delay_estimate_max_us raw_work_max_us '
               'meter_work_max_us store_work_max_us raw_read_max_us').split()
CAPTURE_BYTES = 'internal_free internal_largest internal_min_sum psram_free psram_largest psram_min_sum'.split()
WINDOW_U64 = ('window_start_us window_end_us telemetry_epoch telemetry_window encoded_frames resets '
              'encode_max_us raw_read_frames raw_read_overruns raw_read_retries encode_wall_total_us '
              'raw_wait_total_us raw_wait_max_us encoded_lock_wait_wall_total_us encoded_push_held_wall_total_us').split()
CAPTURE_METRICS = {
    'read_frames': ('read_pcm_frames', 'I2S PCM frames; wrapping u32.'),
    'queue_overflows': ('queue_overflows', 'Queue overflows; wrapping u32.'),
    'software_dropped_frames': ('software_dropped_pcm_frames', 'Dropped PCM frames; wrapping u32.'),
    'read_errors': ('read_errors', 'Read errors; wrapping u32.'),
    'restarts': ('restarts', 'Restarts; wrapping u32.'),
    'internal_free': ('internal_free_bytes', 'Internal heap free bytes.'),
    'internal_largest': ('internal_largest_bytes', 'Largest internal free block.'),
    'internal_min_sum': ('internal_minimum_sum_bytes', 'Independent internal region lows summed.'),
    'psram_min_sum': ('psram_minimum_sum_bytes', 'Independent PSRAM region lows summed.'),
    'psram_free': ('psram_free_bytes', 'PSRAM free bytes.'),
}


def integer(value, maximum=(1 << 64) - 1):
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError('invalid numeric snapshot')


def fields(item, names, maximum=(1 << 64) - 1):
    for name in names: integer(item[name], maximum)


def boolean(value):
    if type(value) is not bool: raise ValueError('invalid numeric snapshot')


def stage_validate(item):
    observed = item['observed_device_us']
    integer(observed); integer(item['operator_epoch'], (1 << 32) - 1)
    if item['health'] not in ('healthy', 'reset_requested', 'recovering', 'terminal'):
        raise ValueError('invalid media health')
    for name, keys in STAGE_KEYS.items():
        part = item[name]
        if part is None: continue
        if type(part) is not dict: raise ValueError('invalid media stage')
        for key in keys:
            if key in ('observed_device_us', 'publication_device_us') and part[key] is None: continue
            integer(part[key], (1 << 32) - 1 if key == 'operator_epoch' else (1 << 64) - 1)
        if name == 'encoded':
            if not part['source_start'] <= part['source_end'] or not part['oldest_accepted_us'] <= part['raw_publication_us'] <= part['production_device_us'] <= observed:
                raise ValueError('invalid encoded provenance')
        else:
            timestamp = part['observed_device_us' if name == 'accepted' else 'publication_device_us']
            if timestamp is not None and timestamp > observed: raise ValueError('future stage timestamp')
    # Accepted and raw observations use separate reads; capture may advance between them.
    raw, encoded = item['raw'], item['encoded']
    backlog = item['backlog_frames']
    if backlog is not None:
        integer(backlog)
        if raw is None or encoded is None or raw['generation'] != encoded['raw_generation'] or item['operator_epoch'] != encoded['operator_epoch'] or encoded['source_end'] > raw['committed_frames'] or backlog != raw['committed_frames'] - encoded['source_end']:
            raise ValueError('invalid media backlog')


def departure_validate(item):
    integer(item['observed_device_us']); integer(item['listener_stack_free_bytes'], (1 << 32) - 1)
    stats = item['stats']
    if stats is None: return
    if type(stats) is not dict: raise ValueError('invalid departure stats')
    fields(stats, [key for key in DEPARTURE_STATS if key not in ('task_runtime_us', 'runtime_valid', 'media')])
    boolean(stats['runtime_valid'])
    if stats['task_runtime_us'] is not None: integer(stats['task_runtime_us'], (1 << 32) - 1)
    if not stats['runtime_valid'] and stats['task_runtime_us'] is not None: raise ValueError('invalid departure runtime')
    if stats['payload_elapsed_us'] > stats['elapsed_us'] or stats['frame_write_wall_max_us'] > stats['frame_write_wall_total_us']:
        raise ValueError('invalid departure durations')
    media = stats['media']
    if media is not None:
        if type(media) is not dict: raise ValueError('invalid departure media')
        fields(media, ['last_age_us', 'max_age_us', 'queue_frames_max'])
        if media['last_age_us'] > media['max_age_us']: raise ValueError('invalid departure age')


def validate(value):
    try:
        if type(value) is not dict or type(value['schema_version']) is not int or value['schema_version'] != 1:
            raise ValueError('unsupported snapshot schema')
        integer(value['served_device_us'])
        for name, length in [('boot_token', 32), ('build_elf_sha256', 64)]:
            identity = value.get(name)
            if identity is not None and (type(identity) is not str or len(identity) != length or any(c not in '0123456789abcdef' for c in identity)):
                raise ValueError('invalid identity metadata')
        settings = value.get('settings')
        if settings is not None:
            if type(settings) is not dict or set(settings) != set(SETTINGS):
                raise ValueError('invalid settings metadata')
            for key in ('active_format', 'next_play_format'):
                if settings[key] not in ('mp3', 'flac'): raise ValueError('invalid format metadata')
            if settings['next_play_buffer'] not in ('low', 'med', 'high'):
                raise ValueError('invalid buffer metadata')
            for key in ('buffer_available', 'autoplay', 'recognition_enabled'): boolean(settings[key])
            integer(settings['pga_gain'], 255)
        network = value.get('network')
        if network is not None:
            if type(network) is not dict: raise ValueError('invalid network metadata')
            fields(network, ['generation', 'disconnects'], (1 << 32) - 1); boolean(network['connected'])
        if value.get('http_stack_free_bytes') is not None:
            integer(value['http_stack_free_bytes'], (1 << 32) - 1)
        served = value['served_device_us']
        listeners = value['listeners']
        if type(listeners) is not list or len(listeners) != 2:
            raise ValueError('invalid listener slots')
        for item in listeners:
            if type(item) is not dict: raise ValueError('invalid listener observation')
            integer(item['generation'], (1 << 32) - 1); integer(item['sample_us'])
            state = item['state']
            if state not in ('unavailable', 'idle', 'waiting', 'writing', 'ready'):
                raise ValueError('invalid listener state')
            if item['sample_us'] > served: raise ValueError('invalid listener timestamp')
            if state in ('writing', 'ready'):
                if item['generation'] == 0: raise ValueError('invalid active generation')
                integer(item['media_age_us']); integer(item['write_us'])
            elif item['media_age_us'] is not None or item['write_us'] is not None:
                raise ValueError('invalid inactive listener values')
            if state == 'waiting' and item['generation'] == 0:
                raise ValueError('invalid active generation')
        for family in FAMILIES:
            item = value.get(family) if family in ('media_progress', 'last_listener_departure') else value[family]
            if item is None: continue
            if type(item) is not dict: raise ValueError('invalid family')
            if family == 'capture':
                fields(item, ['capture_begin_us', 'capture_end_us'])
                fields(item, CAPTURE_U32, (1 << 32) - 1); fields(item, CAPTURE_BYTES)
                begin, end = item['capture_begin_us'], item['capture_end_us']
            elif family == 'encoder':
                fields(item, ['telemetry_epoch', 'telemetry_window'])
                if item['kind'] == 'window':
                    fields(item, WINDOW_U64)
                    fields(item, ['block_samples', 'listeners_start', 'listeners_end'])
                    boolean(item['runtime_valid'])
                    for key in ['stack_free_bytes', 'task_runtime_us']:
                        if item[key] is not None: integer(item[key], (1 << 32) - 1)
                    if not item['runtime_valid'] and item['task_runtime_us'] is not None:
                        raise ValueError('invalid runtime')
                    begin, end = item['window_start_us'], item['window_end_us']
                elif item['kind'] in ('boundary', 'failure'):
                    integer(item['event_us']); begin = end = item['event_us']
                    if item['kind'] == 'boundary' and item['reason'] not in ('startup', 'overrun', 'switched', 'stopped'):
                        raise ValueError('invalid boundary')
                else: raise ValueError('invalid encoder kind')
            elif family in ('media_progress', 'last_listener_departure'):
                (stage_validate if family == 'media_progress' else departure_validate)(item)
                begin = end = item['observed_device_us']
            else:
                integer(item['observed_device_us']); integer(item['dropped'], (1 << 32) - 1)
                boolean(item['loss_occurred'])
                if item['metrics_stack_free_bytes'] is not None: integer(item['metrics_stack_free_bytes'], (1 << 32) - 1)
                begin = end = item['observed_device_us']
            if not begin <= end <= served: raise ValueError('invalid source timestamp')
        return value
    except (KeyError, TypeError) as error:
        raise ValueError('incomplete numeric snapshot') from error


def target(url):
    parsed = urlsplit(url)
    try:
        ipaddress.ip_address(parsed.hostname)
        port = parsed.port or 80
    except (ValueError, TypeError): raise ValueError('target must use a literal IP address') from None
    if parsed.scheme != 'http' or parsed.username is not None or parsed.password is not None or parsed.path != '/api/metrics' or parsed.query or parsed.fragment:
        raise ValueError('target must be http://IP[:port]/api/metrics without credentials')
    return parsed.hostname, port


def fetch(url, timeout):
    host, port = target(url)
    deadline = time.monotonic() + timeout
    connection = http.client.HTTPConnection(host, port, timeout=timeout)
    timer = None
    try:
        connection.connect()
        # A single deadline closes the socket even when a peer drips header/body bytes.
        sock = connection.sock
        def expire():
            try: sock.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            sock.close()
        timer = threading.Timer(max(0, deadline - time.monotonic()), expire)
        timer.daemon = True; timer.start()
        connection.request('GET', '/api/metrics', headers={'Accept': 'application/json', 'Connection': 'close'})
        response = connection.getresponse()
        if response.status != 200: raise ValueError('metrics HTTP status unavailable')
        body = response.read(4097)
        if time.monotonic() >= deadline: raise TimeoutError('metrics deadline exceeded')
        if len(body) > 4096: raise ValueError('metrics body too large')
        try: value = json.loads(body)
        except (ValueError, UnicodeError): raise ValueError('invalid metrics JSON') from None
        return validate(value)
    finally:
        if timer is not None:
            timer.cancel(); timer.join()
        connection.close()


class Collector:
    def __init__(self, stale=30, device_id="line-in"):
        self.device_id = device_id
        self.stale = stale
        self.value = None
        self.received = None
        self.lock = threading.Lock()

    def accept(self, value, now):
        validate(value)
        owned = {key: value.get(key) for key in ('schema_version', 'served_device_us', 'boot_token', 'build_elf_sha256', 'http_stack_free_bytes')}
        owned['listeners'] = [{key: item[key] for key in ('generation', 'state', 'sample_us', 'media_age_us', 'write_us')} for item in value['listeners']]
        owned['network'] = None if value.get('network') is None else {key: value['network'][key] for key in NETWORK}
        owned['settings'] = None if value.get('settings') is None else dict(value['settings'])
        for family in FAMILIES:
            item = value.get(family) if family in ('media_progress', 'last_listener_departure') else value[family]
            if item is None: owned[family] = None; continue
            if family == 'capture': keys = ['capture_begin_us', 'capture_end_us'] + CAPTURE_U32 + CAPTURE_BYTES
            elif family == 'encoder':
                keys = (['kind'] + WINDOW_U64 + ['block_samples', 'listeners_start', 'listeners_end', 'stack_free_bytes', 'task_runtime_us', 'runtime_valid'] if item['kind'] == 'window' else
                        ['kind', 'event_us', 'telemetry_epoch', 'telemetry_window'] + (['reason'] if item['kind'] == 'boundary' else []))
            elif family == 'media_progress':
                owned[family] = {key: item[key] for key in ('observed_device_us', 'operator_epoch', 'health', 'backlog_frames')}
                for name, names in STAGE_KEYS.items():
                    owned[family][name] = None if item[name] is None else {key: item[name][key] for key in names}
                continue
            elif family == 'last_listener_departure':
                owned[family] = {key: item[key] for key in ('observed_device_us', 'listener_stack_free_bytes')}
                stats = item['stats']
                owned[family]['stats'] = None if stats is None else {key: stats[key] for key in DEPARTURE_STATS if key != 'media'}
                if stats is not None:
                    owned[family]['stats']['media'] = None if stats['media'] is None else {key: stats['media'][key] for key in ('last_age_us', 'max_age_us', 'queue_frames_max')}
                continue
            else: keys = ['observed_device_us', 'dropped', 'loss_occurred', 'metrics_stack_free_bytes']
            owned[family] = {key: item[key] for key in keys}
        value = owned
        with self.lock:
            if self.value is not None and value['served_device_us'] == self.value['served_device_us'] and value.get('boot_token') == self.value.get('boot_token'):
                return
            self.value, self.received = value, now

    def identity(self, now):
        with self.lock: value, received = self.value, self.received
        age = None if received is None else max(0, now - received)
        available = age is not None and age <= self.stale
        result = dict(device_id=self.device_id, available=available, age_seconds=age)
        for key in ('boot_token', 'build_elf_sha256', 'settings'):
            result[key] = None if value is None else value.get(key)
        if result['settings'] is not None: result['settings'] = dict(result['settings'])
        return result

    def render(self, now):
        with self.lock: value, received = self.value, self.received
        output = []
        def gauge(name, help_text, number, family=None, kind=None, slot=None, state=None):
            if not any(line.startswith('# TYPE ' + name + ' ') for line in output):
                output.append(f'# HELP {name} {help_text}')
                output.append(f'# TYPE {name} gauge')
            label = '' if family is None else '{family="' + family + '"}'
            if kind is not None: label = '{kind="' + kind + '"}'
            if slot is not None: label = '{slot="' + slot + '"' + (',state="' + state + '"' if state else '') + '}'
            output.append(f'{name}{label} {number}')
        host_age = None if received is None else max(0, now - received)
        available = host_age is not None and host_age <= self.stale
        gauge('device_metrics_response_available', 'Validated response fresh.', int(available))
        if host_age is not None: gauge('device_metrics_response_age_seconds', 'Validated response host age.', host_age)
        for kind, key in [('boot', 'boot_token'), ('build', 'build_elf_sha256'), ('settings', 'settings')]:
            gauge('device_metrics_identity_available', 'Fresh identity metadata present.', int(available and value.get(key) is not None), kind=kind)
        if available and value.get('network') is not None:
            for key, help_text in [('generation', 'Network generation; not disconnect count.'), ('connected', 'Network connection flag.'), ('disconnects', 'Raw wrapping u32 disconnect count.')]:
                gauge('device_network_' + key, help_text, int(value['network'][key]))
        if available and value.get('http_stack_free_bytes') is not None:
            gauge('device_http_stack_free_bytes', 'HTTP task stack free watermark bytes.', value['http_stack_free_bytes'])
        for index in range(2):
            slot = str(index + 1)
            item = None if value is None else value['listeners'][index]
            age = None if item is None else (value['served_device_us'] - item['sample_us']) / 1e6 + host_age
            fresh = available and item is not None and item['state'] != 'unavailable' and age <= self.stale
            gauge('device_listener_available', 'Fresh known slot observation.', int(fresh), slot=slot)
            if age is not None: gauge('device_listener_sample_age_seconds', 'Source sample age plus host time.', age, slot=slot)
            if not fresh: continue
            active = item['state'] != 'idle'
            gauge('device_listener_active', 'Slot occupied; zero means idle.', int(active), slot=slot)
            if active: gauge('device_listener_generation', 'Slot generation; no derivatives.', item['generation'], slot=slot)
            if item['state'] in ('writing', 'ready'):
                gauge('device_listener_media_age_seconds', 'Writing sampled; ready recorded age.', item['media_age_us'] / 1e6, slot=slot, state=item['state'])
                gauge('device_listener_write_seconds', 'Payload write wall time.', item['write_us'] / 1e6, slot=slot, state=item['state'])
        for family in FAMILIES:
            item = None if value is None else value[family]
            age = None
            if item is not None:
                key = ('capture_end_us' if family == 'capture' else
                       ('window_end_us' if item.get('kind') == 'window' else 'event_us') if family == 'encoder' else 'observed_device_us')
                age = (value['served_device_us'] - item[key]) / 1e6 + host_age
            fresh = available and age is not None and age <= self.stale
            gauge('device_metrics_family_available', 'Source and response fresh.', int(fresh), family)
            if age is not None: gauge('device_metrics_family_age_seconds', 'Source age plus host time.', age, family)
            if not fresh: continue
            if family == 'capture':
                for key, (suffix, help_text) in CAPTURE_METRICS.items(): gauge('device_capture_' + suffix, help_text, item[key])
            elif family == 'encoder' and item['kind'] == 'window':
                gauge('device_encoder_window_codec_frames', 'Window codec frames.', item['encoded_frames'])
                gauge('device_encoder_window_seconds', 'Encoder window duration.', (item['window_end_us'] - item['window_start_us']) / 1e6)
                gauge('device_encoder_encode_max_seconds', 'Window max encode time.', item['encode_max_us'] / 1e6)
                for key, suffix, help_text in [('raw_read_frames', 'window_raw_pcm_frames', 'Window raw PCM frames.'), ('raw_read_overruns', 'window_raw_overruns', 'Window raw overruns.'), ('resets', 'window_resets', 'Window encoder resets.')]:
                    gauge('device_encoder_' + suffix, help_text, item[key])
                if item['stack_free_bytes'] is not None:
                    gauge('device_encoder_stack_free_bytes', 'Encoder stack free watermark.', item['stack_free_bytes'])
                gauge('device_encoder_runtime_valid', 'Task runtime measured.', int(item['runtime_valid']))
                if item['task_runtime_us'] is not None: gauge('device_encoder_task_runtime_seconds', 'Window task runtime incl interrupts.', item['task_runtime_us'] / 1e6)
            elif family == 'media_progress':
                def source_fresh(timestamp):
                    return timestamp is not None and (value['served_device_us'] - timestamp) / 1e6 + host_age <= self.stale
                accepted, raw, encoded = (item[name] for name in ('accepted', 'raw', 'encoded'))
                if accepted is not None and source_fresh(accepted['observed_device_us']):
                    gauge('device_media_accepted_pcm_frames', 'Generation accepted PCM frames.', accepted['frames'])
                raw_fresh = raw is not None and source_fresh(raw['publication_device_us'])
                if raw_fresh: gauge('device_media_published_pcm_frames', 'Generation committed PCM frames.', raw['committed_frames'])
                encoded_fresh = encoded is not None and source_fresh(encoded['production_device_us']) and encoded['operator_epoch'] == item['operator_epoch'] and raw is not None and encoded['raw_generation'] == raw['generation']
                if encoded_fresh:
                    gauge('device_media_encoded_sequence', 'Generation encoded sequence.', encoded['sequence'])
                    gauge('device_media_encoded_age_seconds', 'Encoded production age.', (value['served_device_us'] - encoded['production_device_us']) / 1e6 + host_age)
                if item['backlog_frames'] is not None and raw_fresh and encoded_fresh:
                    gauge('device_media_backlog_pcm_frames', 'Matching raw minus encoded PCM frames.', item['backlog_frames'])
            elif family == 'last_listener_departure':
                gauge('device_last_listener_stack_free_bytes', 'Completed listener stack free watermark.', item['listener_stack_free_bytes'])
                stats = item['stats']
                if stats is not None:
                    gauge('device_last_listener_bytes_sent', 'Completed listener bytes sent.', stats['bytes_sent'])
                    gauge('device_last_listener_frame_write_max_seconds', 'Completed listener max frame write time.', stats['frame_write_wall_max_us'] / 1e6)
                    if stats['media'] is not None: gauge('device_last_listener_media_age_seconds', 'Completed listener recorded media age.', stats['media']['last_age_us'] / 1e6)
            elif family.endswith('_loss'):
                if item['metrics_stack_free_bytes'] is not None:
                    gauge('device_metrics_owner_stack_free_bytes', 'Loss owner stack free watermark.', item['metrics_stack_free_bytes'], family)
                gauge('device_telemetry_dropped', 'Receipt queue drops; wrapping u32.', item['dropped'], family)
                gauge('device_telemetry_loss_occurred', 'Receipt reports loss.', int(item['loss_occurred']), family)
        return '\n'.join(output) + '\n'


def device_id(text):
    if not 1 <= len(text) <= 32 or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-' for c in text):
        raise ValueError('device ID must be 1..32 ASCII letters, digits, _, . or -')
    return text


def positive(text):
    value = float(text)
    if not math.isfinite(value) or value <= 0: raise argparse.ArgumentTypeError('must be finite and positive')
    return value


class MetricsHandler(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self):
        if self.path == '/metrics':
            body = self.server.cache.render(time.monotonic()).encode('ascii')
            content_type = 'text/plain; version=0.0.4'
            limit = 12288
        elif self.path == '/identity':
            body = json.dumps(self.server.cache.identity(time.monotonic()), allow_nan=False).encode('ascii')
            content_type = 'application/json'
            limit = 4096
        else: self.send_error(404); return
        if len(body) > limit: self.send_error(503); return
        self.send_response(200); self.send_header('Content-Type', content_type)
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)


class MetricsServer(HTTPServer):
    def __init__(self, cache, port, request_seconds=2):
        self.cache = cache
        self.request_seconds = request_seconds
        super().__init__(('127.0.0.1', port), MetricsHandler)

    def handle_error(self, request, address):
        print('metrics scrape unavailable', file=sys.stderr)

    def process_request(self, request, address):
        # One active scrape and one joined deadline helper bound work and lifetime.
        def expire():
            try: request.shutdown(socket.SHUT_RDWR)
            except OSError: pass
        timer = threading.Timer(self.request_seconds, expire)
        timer.daemon = True
        request.settimeout(self.request_seconds)
        timer.start()
        try: self.finish_request(request, address)
        finally:
            timer.cancel(); timer.join()
            self.shutdown_request(request)


def run(url, cache, server, poll_seconds, timeout_seconds, stop=None):
    stop = stop if stop is not None else threading.Event()
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        while not stop.is_set():
            started = time.monotonic()
            try: cache.accept(fetch(url, timeout_seconds), time.monotonic())
            except (ValueError, OSError, http.client.HTTPException):
                print('metrics poll unavailable', file=sys.stderr, flush=True)
            stop.wait(max(0, poll_seconds - (time.monotonic() - started)))
    except KeyboardInterrupt: pass
    finally:
        server.shutdown(); worker.join(); server.server_close()


def main():
    parser = argparse.ArgumentParser(description='Poll one numeric /api/metrics endpoint; serve Prometheus only on 127.0.0.1. No UART, proxies, redirects or daemon installation.')
    parser.add_argument('--target', required=True, help='http://literal-IP[:port]/api/metrics; no credentials')
    parser.add_argument('--device-id', default='line-in', help='operator device ID: 1..32 ASCII letters/digits/_.-')
    parser.add_argument('--port', type=int, default=4816, help='loopback exporter port (default: 4816)')
    parser.add_argument('--poll-seconds', type=positive, default=5)
    parser.add_argument('--timeout-seconds', type=positive, default=2)
    args = parser.parse_args()
    try:
        target(args.target)
        device_id(args.device_id)
    except ValueError as error: parser.error(str(error))
    if not 1 <= args.port <= 65535: parser.error('port must be 1..65535')
    cache = Collector(device_id=args.device_id)
    server = MetricsServer(cache, args.port)
    run(args.target, cache, server, args.poll_seconds, args.timeout_seconds)


if __name__ == '__main__': main()
