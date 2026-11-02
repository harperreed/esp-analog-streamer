# ABOUTME: Exercises the real board's MP3 Sonos queue, buffer settings and manual Stop.
# ABOUTME: Throwaway acceptance preserves the saved destination, autoplay and volume.
import json
import os
import sys
import time
import urllib.request
from pathlib import Path

base = 'http://' + os.environ['MP3_TEST_HOST']
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
result = {}


def api(path='/api/sonos', body=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base + path, data=data, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=8) as response:
        return json.load(response)


def wait_state(expected, seconds=60):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        state = api()
        if state['state'] == expected:
            return state
        time.sleep(1)
    raise AssertionError(('state timeout', expected, state['state'], state['last_error']))


def preset(value):
    api(body={'action': 'buffer', 'preset': value})
    end = time.monotonic() + 15
    while time.monotonic() < end:
        if api()['config']['buffer_preset'] == value:
            return
        time.sleep(.5)
    raise AssertionError('preset did not persist')


def queue():
    return api('/api/sonos/queue/v2.1/itemWindow')


before = api()
assert before['state'] == 'stopped'
assert before['config']['autoplay'] is False
try:
    for value in ['low', 'med', 'high', 'med']:
        preset(value)
    result['preset_writes'] = True
    api(body={'action': 'play'})
    wait_state('playing')
    first = queue()
    track = first['items'][0]['track']
    assert track['mediaUrl'].endswith('/stream.mp3?buffer=med')
    assert track['contentType'] == 'audio/mpeg', track
    result['medium_mp3_playing'] = True
    preset('high')
    for _ in range(20):
        assert queue() == first
        assert api()['state'] == 'playing'
        time.sleep(1)
    result['current_queue_immutable'] = True
    api(body={'action': 'stop'})
    wait_state('stopped', 20)
    time.sleep(5)
    api(body={'action': 'play'})
    wait_state('playing')
    second = queue()
    assert second['items'][0]['track']['mediaUrl'].endswith('/stream.mp3?buffer=high')
    assert second['queueVersion'] != first['queueVersion']
    for _ in range(20):
        assert api()['state'] == 'playing'
        time.sleep(1)
    result['next_play_high'] = True
    api(body={'action': 'stop'})
    wait_state('stopped', 20)
    for _ in range(15):
        assert api()['state'] == 'stopped'
        time.sleep(1)
    result['manual_stop_stays_stopped'] = True
except BaseException as error:
    result['error'] = repr(error)
    raise
finally:
    try:
        api(body={'action': 'stop'})
        wait_state('stopped', 20)
        preset('med')
        after = api()
        result['config_restored'] = before['config'] == after['config']
        assert result['config_restored']
    finally:
        (out / 'sonos-result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result), flush=True)
