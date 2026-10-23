These fixtures derive from retained real Sonos receipts, not invented schemas.

`device.xml` comes from `target/kata-hardening/sonos-http/input/device-description.xml`, fetched by the parent from the selected coordinator's `/xml/device_description.xml` on 2026-10-11. It retains the root/embedded-device structure, service types and control paths. Nonessential metadata was removed. The root identity becomes RINCON_TEST and the embedded identities retain their `_MS`/`_MR` suffixes.

The SOAP fixtures come from `target/kata-hardening/flac-stereo/sonos-soap-corrected/`: `2-GetZoneGroupState.xml`, `14-GetMediaInfo.xml`, `18-GetTransportInfo.xml`, `17-Play.xml`, `94-Stop.xml`, and `15-SetAVTransportURI.xml`. Topology retains group/member structure and identity/location fields; other member attributes were removed. Identities, group ID and addresses were replaced with test values. Media URI was replaced and its metadata removed. Command/action namespaces and response fields preserve the receipts. The transport receipt reports TRANSITIONING.

The retained `sonos-description.xml` is the SSDP SpeakerGroup description and has no usable AVTransport service; it is deliberately not the source for the production UPnP service lookup. `sonos-services.json` and `sonos-soap.py` establish the actual device-description base and request fields.
