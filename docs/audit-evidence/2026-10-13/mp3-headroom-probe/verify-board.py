# ABOUTME: Verifies the corrected image and preserved stopped settings without playback writes.
# ABOUTME: Revalidates a preflight Sonos address by its UDN when fresh discovery is empty.
import json
from pathlib import Path
import urllib.request
import xml.etree.ElementTree as ET

OUT = Path(__file__).resolve().parent
def get(url, name):
    data = urllib.request.urlopen(url, timeout=5).read()
    (OUT/name).write_bytes(data)
    return data

values = {name:json.loads(get('http://192.168.23.217/api/'+name,'after-'+name+'.json'))
          for name in ['state','sonos','metrics']}
m, s = values['metrics'], values['sonos']
before = json.loads((OUT/'before-sonos.json').read_text())
assert m['build_elf_sha256'] == json.loads((OUT/'check.json').read_text())['elf_sha256']
assert s['config'] == before['config']
assert s['state'] == 'stopped' and all(x['state']=='idle' for x in m['listeners'])
assert values['state']['shazam_enabled'] is False
selected = s['config']['player_id']
discovery = before if before['groups'] else json.loads((OUT.parent/'oct13-mp3-normalization/before-sonos.json').read_text())
peers = [g for g in discovery['groups'] if g['player_id']==selected]
assert len(peers)==1
base = 'http://'+peers[0]['host']+':1400'
description = ET.fromstring(get(base+'/xml/device_description.xml','after-device.xml'))
udn = description.find('.//{*}device/{*}UDN').text
assert udn == 'uuid:'+selected, (udn,selected)
typ = 'urn:schemas-upnp-org:service:AVTransport:1'
services = [x for x in description.findall('.//{*}service') if x.find('{*}serviceType').text==typ]
assert len(services)==1
control = services[0].find('{*}controlURL').text
assert control.startswith('/') and not control.startswith('//')
envelope = f'<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:GetTransportInfo xmlns:u="{typ}"><InstanceID>0</InstanceID></u:GetTransportInfo></s:Body></s:Envelope>'
request = urllib.request.Request(base+control,data=envelope.encode(),headers={
    'Content-Type':'text/xml; charset="utf-8"','SOAPAction':f'"{typ}#GetTransportInfo"'})
tree = ET.fromstring(get(request,'after-transport.xml'))
transport = {k:tree.find('.//{*}'+k).text for k in ['CurrentTransportState','CurrentTransportStatus']}
assert transport == {'CurrentTransportState':'STOPPED','CurrentTransportStatus':'OK'}
report = {'boot_token':m['boot_token'],'build_elf_sha256':m['build_elf_sha256'],
          'settings_unchanged':True,'state':s['state'],'listeners_idle':True,
          'board_groups_count':len(s['groups']),'sonos_udn_revalidated':True,
          'remote_transport':transport}
(OUT/'final-verification.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
