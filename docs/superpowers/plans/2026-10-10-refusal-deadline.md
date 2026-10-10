# 8s9m prerequisite: one refusal-write budget

Doctor Biz, this brief preserves the current two-second response allowance and all socket/slot/lease ownership. It implements neither concurrent admission nor a new close/restart policy. Parent reviews this brief before dispatch; implementation starts after the current canonical-check snapshot completes. No tests, builds, tracked edits or hardware actions accompanied this brief.

## Verified implementation checkpoint

Both refusal paths now share the existing deadline writer. The real-socket
regression failed on the legacy path at 805.87 ms for a 400 ms allowance and
passes with the shared operation. All 19 listener telemetry tests and 31 writer
tests pass; independent review found no actionable issue. Parent ran all 27
canonical stages successfully with complete receipts and equal boundary
snapshots in target/check-logs/run.jurAgEJG. Fresh-eyes found no further issue.
No new firmware was flashed for this unit. Host timing cannot bound a target
lwIP call that waits internally. The broader 8s9m admission task stays open.

## Production operation and file ownership

One worker owns:

- `core/src/stream_http.rs`: add a small public domain operation `write_response(sock: &TcpStream, response: &[u8], timeout: Duration) -> io::Result<()>`. Its entire body establishes `Instant::now() + timeout`, constructs the existing `DeadlineWriter`, and calls `write_all_before` once. Document exclusive socket use and that ownership remains with the caller. No response allocation, parsing, logging, retries outside the writer, telemetry or socket closure belongs here.
- `firmware/src/stream.rs`: `send_refusal` retains its existing status rendering and logs, but replaces `set_write_timeout(...).and_then(write_all)` with `stream_http::write_response(sock, response.as_bytes(), HEAD_TIMEOUT)`. Remove the now-unused `Write` import if no other use remains. Do not move logging or claim it falls within the response-write budget.
- `firmware/src/listener_telemetry.rs`: `send_admitted_refusal` calls the same core operation with its existing `timeout` parameter and response bytes. Preserve its signature, event timestamp timing, reason/status/error conversion and publication ordering. Remove its unnecessary mutable socket binding/local `Write` import.
- `core/tests/listener_telemetry.rs`: add refusal-path regressions against the actual imported production module and retain existing success/error/ownership tests.

Keep both `HEAD_TIMEOUT` arguments at the allocation-refusal and buffered-startup-refusal call sites unchanged. No manifests, dependencies, thread policies, parser, queue, media writer, close logic or fixture changes. The shared operation is deliberately the only response-write implementation, and the ordinary firmware wrapper must call it directly.

## TDD sequence and meaningful failure

1. Add a real-socket regression to `core/tests/listener_telemetry.rs` against **existing `send_admitted_refusal`**, before production edits. Use a large ASCII response supplied through its existing `&str` argument; this is a test payload, not a changed production HTTP response. A tiny real 503 fits the send buffer and cannot establish the defect.
2. Reuse the shape of `stream_http`'s existing `small_buffered_pair`/bursty-read test: construct the client with `socket2`, set its receive buffer **before connect**, accept a real loopback socket, and fix the server send buffer. Use a response much larger than both buffers, for example 8 MiB; reader drains bounded bursts at intervals shorter than the tested write allowance. Start with the existing 400 ms write allowance and 64 KiB/300 ms burst pattern; tune only if actual red evidence proves the host needs a different pressure pattern. The 2 s production policy does not require every test to last 2 s.
3. Give the reader a real read timeout and a finite observation deadline, retain its byte count, and coordinate startup through a channel/barrier. Stop and join every worker even if an assertion fails: collect result/time first, close or shut down the server as appropriate, request reader stop, join, then assert. A finite reader horizon must eventually stop progress so the legacy blocking write returns; no detached stuck workers. Do not assume fixed buffer sizes alone prove pressure.
4. Require partial progress (`0 < received < response.len()`), a recorded timeout-like refusal response error, and completion near the whole-write allowance with a generous scheduler ceiling. For the 400 ms case, start with existing writer-test timing practice, widening only with recorded justification. **Run on the legacy implementation and retain its actual failing assertion.** If the old path passes, the workload did not reproduce resetting/progress-sensitive timeout behavior; improve the real pressure/progress pattern rather than accepting a green-only timing test or writing a fake transport.
5. Implement the shared operation and replace both callers. Rerun the exact regression green. Add a direct real-socket shared-operation case so ordinary refusals use a tested production operation, not an untested duplicate. Source review verifies the ordinary wrapper delegates; importing all ESP firmware solely for this wrapper is unnecessary.

Test payload size is essential to exercise the existing generic response-sending seam; it does not prove ordinary short 503 responses routinely stall. The regression establishes bounded application retry/writable-wait behavior under pressure.

## Preserve and extend contract checks

- Keep `admitted_refusal_sends_original_response_and_records_numeric_write_failure`: its successful exact response is the compatibility check. Strengthen assertions for existing peer/timestamp/status if useful; do not replace it with the large payload test.
- Keep `admitted_allocation_refusal_preserves_real_response_write_error`: real local `shutdown(Write)` currently produces `BrokenPipe` and a numeric OS error. Preserve allocation reason and actual response-write error separately.
- Keep `real_socket_timeout_and_refusal_reach_close_with_full_queue_and_blocked_exporter`: this exercises production `ConnectionResources`, actual socket I/O and queue pressure. Add pressure-refusal coverage here only if it adds ownership evidence rather than duplicating timing coverage. Slots/format leases remain held while response writing runs and release through existing `close_and_report`; no helper consumes/clones/closes the socket.
- Verify original descriptor flags on shared-operation success and timeout/error; existing writer unit tests already exercise restoration, so add only a focused wrapper-level assertion rather than duplicating the writer suite. Include a normal small exact response through the shared operation.
- No mocks, application test modes, raw descriptor reconstruction, or test-only alternative response implementation. Do not assert successful delivery from bytes accepted by send.

## Honest limits and review

`DeadlineWriter` already uses one elapsed-time `WriteProgress` budget across sends, interruptions and poll waits, changes the socket temporarily to nonblocking, and restores flags before returning. Reuse it unchanged. On pinned ESP-IDF/lwIP, a nonblocking `send` can still wait internally for mailbox/core processing; the budget cannot preempt that call or descriptor operations. Do not promise hard two-second wall-clock return, target scheduling bounds, bounded synchronous logging, or bounded close from a host test. A flag-restoration failure can also replace the write error, as in the existing writer; do not redesign that contract here.

Parent runs the canonical `scripts/check` on the integrated frozen state and retains per-stage evidence. Focused real-socket tests supply actual red/green output first. Independent review checks both call sites, exact response bytes, exclusive socket access, telemetry/error identity and unchanged ownership. Parent owns target builds/hardware; hold firmware integration if it would disturb the current MP3 attribution input. No flash is part of this worker unit.

8s9m remains open after this subtask: its admission concurrency policy, startup/configure-failure handling, real-board memory/stack and latency acceptance remain separate. Existing MP3 failures and all hardware gates remain unchanged.
