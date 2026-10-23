# ABOUTME: Records read-only board and selected Sonos state around a standalone benchmark.
# ABOUTME: Requires the ordinary image, stopped playback, idle listeners and preserved settings.
import argparse
import hashlib
import json
from pathlib import Path
import urllib.request
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', choices=['before', 'after'])
    args = parser.parse_args()
    out = Path(__file__).resolve().parent
    prefix = args.phase

    def get(url, name):
        data = urllib.request.urlopen(url, timeout=5).read()
        (out / f'{prefix}-{name}').write_bytes(data)
        return data

    values = {name: json.loads(get('http://192.168.23.217/api/' + name, name + '.json'))
              for name in ['state', 'sonos', 'metrics']}
    state, sonos, metrics = (values[name] for name in ['state', 'sonos', 'metrics'])
    expected = hashlib.sha256((out / 'ordinary.elf').read_bytes()).hexdigest()
    assert metrics['build_elf_sha256'] == expected
    assert sonos['state'] == 'stopped'
    assert all(row['state'] == 'idle' for row in metrics['listeners'])
    assert state['shazam_enabled'] is False
    assert sonos['config']['autoplay'] is False
    assert metrics['settings'] is not None
    assert metrics['settings']['active_format'] == metrics['settings']['next_play_format'] == 'mp3'
    assert metrics['settings']['next_play_buffer'] == 'med'
    before = json.loads((out / 'initial-sonos.json').read_text())
    assert sonos['config'] == before['config']
    prior_metrics = json.loads((out / 'initial-metrics.json').read_text())
    assert metrics['settings'] == prior_metrics['settings']
    selected = sonos['config']['player_id']
    discovery = before
    if not discovery['groups']:
        discovery = json.loads((out.parent / 'oct13-mp3-normalization/before-sonos.json').read_text())
    peers = [group for group in discovery['groups'] if group['player_id'] == selected]
    assert len(peers) == 1
    base = 'http://' + peers[0]['host'] + ':1400'
    description = ET.fromstring(get(base + '/xml/device_description.xml', 'device.xml'))
    assert description.find('.//{*}device/{*}UDN').text == 'uuid:' + selected
    service_type = 'urn:schemas-upnp-org:service:AVTransport:1'
    services = [service for service in description.findall('.//{*}service')
                if service.find('{*}serviceType').text == service_type]
    assert len(services) == 1
    control = services[0].find('{*}controlURL').text
    assert control.startswith('/') and not control.startswith('//')
    body = ('<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>'
            f'<u:GetTransportInfo xmlns:u="{service_type}"><InstanceID>0</InstanceID>'
            '</u:GetTransportInfo></s:Body></s:Envelope>')
    request = urllib.request.Request(base + control, data=body.encode(), headers={
        'Content-Type': 'text/xml; charset="utf-8"',
        'SOAPAction': f'"{service_type}#GetTransportInfo"'})
    transport = ET.fromstring(get(request, 'transport.xml'))
    remote = {key: transport.find('.//{*}' + key).text
              for key in ['CurrentTransportState', 'CurrentTransportStatus']}
    assert remote == {'CurrentTransportState': 'STOPPED', 'CurrentTransportStatus': 'OK'}
    result = {'status': 'pass', 'phase': prefix, 'build_elf_sha256': expected,
              'boot_token': metrics['boot_token'], 'board_state': sonos['state'],
              'listeners_idle': True, 'settings_preserved': True, 'remote_transport': remote}
    (out / f'{prefix}-verification.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
