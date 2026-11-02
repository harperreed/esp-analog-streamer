# Kata closure review

Doctor Biz flagged prolonged work without fewer open tickets. This review uses
the original issue bodies, not the additional gates accumulated in later plans.
The starting count was 29 open tickets; closing 8s9m, 21tp and 89g8 reduces it
to 26. Failed acquisition results remain failed;
closing a scoped implementation does not approve a streaming release.

## Work that meets its original acceptance

| Kata | Completed scope and verification | Limits retained |
| --- | --- | --- |
| 8s9m | Two permanent gated admission workers; real-socket tests cover slow, trickled, malformed, oversized and disconnected requests, saturation and worker launch failures. The production device request trial passes the corresponding head cases and full-capacity refusal. ESP32 measurements record response latency, heap and stack cost. Canonical checks pass. | Sustained MP3 delivery fails. The resource trials did not reach combined pending-head/TLS load. Neither a whole-device descriptor census nor worst-case resource safety is claimed. |
| 21tp | Fixed-size encoder/listener records drain through the low-priority exporter. Real blocked-exporter tests preserve progress and checked-close resource release; overflow is explicit. Device trials measure heap/stack and demonstrate a fresh decoder after slot reclamation. Canonical checks pass. | Device export loss and MP3 delivery failures remain real. The original issue permits explicit bounded overflow; it does not require loss-free diagnostics or successful sustained MP3 playback. |
| 89g8 | Deterministic production-graph replay, independent exact-sample oracle, omission/duplication controls, stable fixtures and failure reduction pass locally. The every-PR workflow invokes the same tests and passes actionlint. | The workflow has not executed on GitHub. This closes the replay implementation and PR integration, not hosted CI acceptance, HIL or release readiness. |

For 8s9m, retained canonical run `target/check-logs/run.uBzY2Yyr` has
`run.status=complete`, `exit-code=0`, all 27 stages passing, and all 15 admission
tests passing. The request and resource observations are recorded in
[device evidence](concurrent-admission-device.md) and
[resource evidence](concurrent-admission-resources.md). The measured idle
internal-memory difference is 4,868–4,880 bytes; two-worker minimum historical
untouched stack is 1,664/1,672 bytes. These are bounded observations, not a
causal overhead isolation or worst-case margin.

For 21tp, the same canonical run covers the current telemetry implementation.
[Default device evidence](encoder-telemetry-default-device.md) and
[stack repeat evidence](encoder-export-stack-device.md) retain failed delivery
and explicit diagnostic drops. A reclaimed connection decodes ten MP3 frames
(11,520 samples). Host tests deliberately block the exporter and fill its queue
while actual socket cleanup and encoder operations proceed. Earlier conclusions
that these delivery failures necessarily kept 21tp open added a broader gate
than the original logging-isolation requirement.

For 89g8, [replay evidence](replay.md) records the passing canonical check,
14 oracle passes and 12 intended negative-control failures. The first hosted
run remains unverified. Requiring that first run as a separate replay closure
campaign was an added gate; the CI coverage ticket m31k explicitly requires
reports produced by CI and remains open until that happens.

## Work still incomplete

The audits also checked m31k, 97bk, z5fs, n4jb, hv4x, ed4k, dp08, dctv,
cxbw, 6vct, pqqq, qm2a, 45k9, cn4m, 7vhq, ewvq and d2cb. None has evidence
for every original requirement. In particular, real Sonos TLS setup still
causes capture faults; MP3 generation resets remain unresolved; the dedicated
HIL runner and controlled network-fault path are incomplete. Long-soak and
ownership-safe recovery acceptance remain outstanding.

Do not start another dashboard or codec-placement investigation to finish these
three tickets. Future work should name one open ticket, its missing original
criterion and the bounded action that can satisfy it. Shared audio failures
belong in the delivery/capture/recovery tickets and release gates.

This review supersedes ticket-status recommendations in the linked historical
reports and plans. It does not alter their measurements or failed run verdicts.
