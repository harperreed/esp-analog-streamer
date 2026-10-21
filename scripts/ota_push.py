# ABOUTME: Pushes an app image to a board over HTTP (POST /api/ota with the shared token), then waits for the board to
# ABOUTME: come back and checks that the ELF sha it reports is the one inside the image. Host-testable helpers, main does I/O.
import argparse
import http.client
import json
import os
import re
import sys
import time
import urllib.request

HEAD_BYTES = 32 + 256
IMAGE_MAGIC = 0xE9
DESCRIPTOR_MAGIC = 0xABCD5432


class Failure(Exception):
    pass


def describe(head):
    """The app descriptor of an ESP32 image head, or a Failure naming what is wrong (the board applies the same rules)."""
    if len(head) < HEAD_BYTES:
        raise Failure('image shorter than its header')
    if head[0] != IMAGE_MAGIC:
        raise Failure('not an ESP32 app image (an ELF is not an image: use espflash save-image)')
    if int.from_bytes(head[12:14], 'little') != 0:
        raise Failure('image built for another chip')
    d = head[32:32 + 256]
    if int.from_bytes(d[0:4], 'little') != DESCRIPTOR_MAGIC:
        raise Failure('image carries no app descriptor')
    text = lambda b: b.split(b'\0', 1)[0].decode('utf-8', 'replace')
    return dict(version=text(d[16:48]), project=text(d[48:80]), idf_version=text(d[112:144]), elf_sha256=d[144:176].hex())


def token_from(explicit, environment, cfg_text):
    """The token to send: --token, else $OTA_TOKEN, else cfg.toml's ota_token; empty means nothing to send."""
    if explicit:
        return explicit
    if environment.get('OTA_TOKEN'):
        return environment['OTA_TOKEN']
    match = re.search(r'^\s*ota_token\s*=\s*"([^"]*)"', cfg_text or '', re.M)
    return match.group(1) if match else ''


def push(host, image, token, timeout):
    connection = http.client.HTTPConnection(host, 80, timeout=timeout)
    connection.request('POST', '/api/ota', body=image, headers={'Content-Type': 'application/octet-stream', 'Content-Length': str(len(image)), 'X-Ota-Token': token, 'Connection': 'close'})
    response = connection.getresponse()
    body = response.read(4096)
    connection.close()
    return response.status, body.decode('utf-8', 'replace')


def wait_for_board(host, expected_sha, deadline_s, now=time.monotonic, sleep=time.sleep, fetch=None):
    """Polls /api/metrics until the board reports `expected_sha` or the deadline passes; returns (seen_sha, seconds)."""
    if fetch is None:
        def fetch():
            with urllib.request.urlopen(f'http://{host}/api/metrics', timeout=3) as response:
                return json.load(response).get('build_elf_sha256')
    started = now()
    seen = None
    while now() - started < deadline_s:
        try:
            seen = fetch()
        except Exception:
            seen = None
        if seen == expected_sha:
            return seen, round(now() - started, 1)
        sleep(2)
    return seen, round(now() - started, 1)


def main(argv=None):
    parser = argparse.ArgumentParser(prog='scripts/ota-push', description='Push an app image (espflash save-image output) to a board over HTTP and confirm it boots into it.')
    parser.add_argument('host', help='board address')
    parser.add_argument('image', help='app image file (.bin from espflash save-image, not the ELF)')
    parser.add_argument('--token', help='upload token (default: $OTA_TOKEN, else ota_token in firmware/cfg.toml)')
    parser.add_argument('--cfg', default='firmware/cfg.toml', help='cfg.toml to read ota_token from')
    parser.add_argument('--wait', type=float, default=120.0, help='seconds to wait for the board to come back (default 120)')
    parser.add_argument('--timeout', type=float, default=180.0, help='upload socket timeout in seconds')
    args = parser.parse_args(argv)
    image = open(args.image, 'rb').read()
    try:
        description = describe(image[:HEAD_BYTES])
    except Failure as error:
        print(f'FAIL  {error}', file=sys.stderr)
        return 2
    cfg_text = open(args.cfg).read() if os.path.exists(args.cfg) else ''
    token = token_from(args.token, os.environ, cfg_text)
    if not token:
        print('FAIL  no token: pass --token, set OTA_TOKEN, or set ota_token in cfg.toml', file=sys.stderr)
        return 2
    print(f"pushing {len(image)} bytes, version {description['version']}, elf {description['elf_sha256'][:16]}… to {args.host}")
    status, body = push(args.host, image, token, args.timeout)
    print(f'board answered {status}: {body.strip()}')
    if status != 202:
        return 1
    seen, seconds = wait_for_board(args.host, description['elf_sha256'], args.wait)
    if seen == description['elf_sha256']:
        print(f'OK    board is running elf {seen[:16]}… after {seconds} s')
        return 0
    print(f'FAIL  after {seconds} s the board reports elf {str(seen)[:16]}…, not the pushed image (rollback or still booting)', file=sys.stderr)
    return 1


if __name__ == '__main__':
    sys.exit(main())
