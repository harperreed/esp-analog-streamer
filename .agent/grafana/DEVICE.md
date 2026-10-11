# ESP32 device observations

Import `device-dashboard.json` as a separate dashboard. Its UID is
`esp32-device-health`; it uses Prometheus datasource UID `prometheus` and does not
replace Navigator telemetry. This is partial amqe work: import tests do not prove
live device health or the original acceptance criteria.

The artifact uses Grafana's Classic dashboard JSON and schema version 39, as
specified in the [Grafana 10.4.19 migrator source](https://github.com/grafana/grafana/blob/v10.4.19/public/app/features/dashboard/state/DashboardMigrator.ts#L74).
[Current Grafana documentation](https://grafana.com/docs/grafana/latest/visualizations/dashboards/build-dashboards/view-dashboard-json-model/)
describes Classic JSON import/export and the 24-column grid. No optional panel
plugins are required. Existing Compose images use mutable `latest` tags; this
artifact does not pin or change that deployment. A private Grafana 13.2.3 lab has provisioned the artifact; rendered browser
verification remains an integration check.

## Collector and independent local validation

From the repo root, with an authorized numeric endpoint available:

```sh
scripts/device-metrics --target http://DEVICE_LITERAL_IP/api/metrics --device-id line-in
```

Replace `DEVICE_LITERAL_IP` with the board's literal IP. The collector accepts an
optional device port in the URL, binds only `127.0.0.1:4816`, exposes `/metrics` and `/identity`,
polls every five seconds, limits each request to two seconds, and uses a
30-second freshness limit. `--port`, `--poll-seconds`, and `--timeout-seconds`
override those host settings. Poll frequency does not change firmware sampling.
The device JSON body limit is 4096 bytes; host Prometheus exposition is bounded at 8192 bytes. Do not use `/api/state`, UART, or an audio route as a metrics source.

For independent validation, use separate temporary state/config directories and
rootless host processes. Check selected ports are free. Example Prometheus
configuration for such a process:

```yaml
scrape_configs:
  - job_name: esp32-device
    scrape_interval: 5s
    static_configs:
      - targets: ['127.0.0.1:4816']
```

Start that Prometheus with its own config/storage and an explicit loopback web
listen address, for example `127.0.0.1:19090`. In a separate Grafana process bound
to `127.0.0.1`, add a Prometheus datasource with UID `prometheus` and URL
`http://127.0.0.1:19090`. Use fresh Grafana state and a chosen admin password;
leave the existing services and provisioning untouched. In Grafana's dashboard
import UI, upload `device-dashboard.json` and save it as a separate dashboard.
A fresh instance avoids colliding with the existing datasource UID.

The existing Linux Docker setup cannot reach a host loopback-only exporter via
`host.docker.internal`: host gateway addresses are different from loopback.
Do not change collector bindings or existing Compose networking to make this
example work. Host processes share loopback. Existing Docker socket permission
or deployment/auth policy changes require their own review; these instructions
neither start services nor establish deployment acceptance.

## Reading the panels

Response status and age describe validated HTTP responses. Family status and age
independently describe capture, encoder, listener-loss, encoder-loss, media-progress
and last-listener-departure samples.
Status is an instant query: 1 is green and available, 0 red and unavailable, and no data means
no scrape/status measurement. Age is seconds and stays absent before a valid
sample exists. A fresh response can contain a stale family. When a family expires,
source series disappear; graphs leave gaps and do not fill them with zero.
Historical points remain historical observations, not current health.

Capture read PCM frames count reads before settling and drops. They do not measure
publication. Capture faults and drops are raw wrapping u32 count gauges. Boot
continuity does not authorize derivatives: no host rate, increase or delta is inferred, even with boot metadata. Encoder
progress and runtime are gauges for the last measured window. Codec frames per
second divides window frames by positive window seconds; it is not a PCM rate or
an audio real-time ratio. Runtime includes charged interrupts and remains absent
when invalid. Memory panels show sampled free bytes/largest internal block and sums of independent region low-water values. Those sums are not simultaneous heap minima. Encoder and metrics-owner stack panels show measured free watermark bytes; zero is valid, null stays unavailable. Metrics-owner series split by loss receipt family describe the same task at separate observation times. Encoder raw reads, raw overruns and resets describe the last measured window. Loss flags and dropped counts are split by fixed `family`
labels; loss invalidates a complete event census.

Media panels distinguish accepted PCM frames, committed raw PCM frames and the
latest encoded sequence. These are generation-scoped gauges, not rates. Accepted
and raw observations need their own fresh timestamps. Encoded sequence and age
require a fresh production timestamp with matching raw generation and operator
epoch. Encoded age adds elapsed host acquisition time to the recorded device age.
Backlog subtracts encoded source-end frames from raw committed frames only when
both receipts are fresh and their generations match. Missing guards leave gaps.

Last-listener panels show the latest **completed** listener receipt, not live
individual listeners. Bytes sent, maximum whole-frame write time, recorded last
frame media age and measured stack watermark expire with that family's freshness.
The recorded listener media age stays fixed; it does not grow with host time.
Network connected state, raw wrapping disconnect count and generation token come
from the latest fresh response when present. Generation is an invalidation token,
not a disconnect count. Missing optional network observations stay unavailable.

Unavailable measures stay unavailable: live per-listener lag/write duration,
full live task-stack headroom and measured active buffer settings. No identity is invented from hostname or telemetry epoch. Compare
revisions only with externally recorded matched workload, config and input
provenance. No credentials, peers, private song text or audio belong in metrics.

## Identity and settings metadata

`device_metrics_identity_available{kind="boot|build|settings"}` reports presence
within response freshness, using fixed labels only. Its age is the response age;
there is no separate identity age metric. Read `http://127.0.0.1:4816/identity`
for `device_id`, `available`, `age_seconds`, `boot_token`, `build_elf_sha256` and
`settings`. The default operator alias is `line-in`; `--device-id` selects an
alias of 1–32 ASCII letters/digits or `_.-`. It is not a hardware identity or an
authentication claim. Boot token and build ELF hash come from device metadata.
Stale metadata can remain in that JSON with `available=false`; check freshness
before using it. Values never become Prometheus labels.

Settings contain only `active_format`, `next_play_format`, `next_play_buffer`,
`buffer_available`, `autoplay`, `recognition_enabled` and `pga_gain`.
Next-Play selections do not attest the active stream's buffer. Keep revision
comparison receipts with matched effective workload and input provenance; a
metadata hash alone is not an overhead or playback acceptance test.

## Verification still required

`uv run python -m unittest discover -s scripts -p test_device_dashboard.py`
checks import-critical structure, datasource references, useful query/unit pairs,
independent freshness/identity queries, media and completed-listener query units,
availability colors and missing-data behavior. It does not run Grafana.
The repository canonical `scripts/check`, real Grafana import/render/query, live
device scrape, a controlled source-to-panel event, collector/source staleness
checks, and matched collector-off/on overhead measurements are separate gates.
Keep amqe open until its original acceptance is met; local synthetic data cannot
stand in for the live device event.
