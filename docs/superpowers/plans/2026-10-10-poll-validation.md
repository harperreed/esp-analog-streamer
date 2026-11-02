# Validate stream poll departures (Kata 6hjs)

Validated: three real-socket tests, negative controls, independent review and all
21 canonical stages passed. No production retry-policy change was justified.
See docs/test-poll-departure.md for evidence and target limits.

Scope: characterize the existing retry-send policy, changing it only if a real
failure demonstrates a defect. Sender instrumentation in 0d9d014 now distinguishes
error readiness from writable readiness. No invalid-descriptor race, new socket
backend, or generic networking abstraction is needed.

## Evidence and semantics

The [POSIX poll specification](https://pubs.opengroup.org/onlinepubs/9799919799/functions/poll.html)
reports error and hangup conditions without requesting those bits. Its documented
hangup/writable relationship must not substitute for checking the actual target.
[POSIX send](https://pubs.opengroup.org/onlinepubs/000095399/functions/send.html)
distinguishes local acceptance from delivery and reports local connection errors.

Pinned IDF 5.5.5 lwIP sockets.c, lwip_pollscan around lines 2298–2317, checks
sendevent and errevent independently, so writable and error bits can coexist.
Invalid sockets set POLLNVAL and return an error. The platform newlib header
defines the event values; generic lwIP fallback values differ. Trace lwip_send,
netconn_write_partly and errno mapping before deciding any behavior change.

The diagnostic hardware run 20261010T174257Z already recorded a final error-ready
poll and one send error for each bounded FLAC receiver when the host ended its
capture. Retain the final departure/close records and exact image identity before
using that as target evidence. It is not an injected orderly-close experiment.

## Implementation and tests

Add a small host integration test file using the production DeadlineWriter and
real TCP sockets. Keep ordinary backpressure/deadline tests. Exercise peer
write-half-close while its read side still receives exact payload, full close,
and forced reset during a pressured send. Verify bounded completion, original
OS error/descriptor flags, accepted prefixes and no repeated error-ready spin.
Use actual socket buffers, synchronization and bounded joins; do not depend on
an unsafe cross-thread raw-descriptor close. A test-only invalid descriptor must
never enter a borrowed TcpStream.

Use actual Slot ownership through the worker error/drop path where useful,
but do not claim that host std Drop models the ESP-IDF socket_close function.
Review ListenerConnection's close-before-slot-release path and use saved real
target close evidence separately. Preserve valid residual writes and let send
retain useful errno context. Do not clear SO_ERROR just to classify an event.

Document each real observed event/result, POSIX versus pinned target limits,
and whether retry-send is sufficient. A test-only validation task may start
green because no production bug is alleged; use a temporary negative assertion
control to establish that the relevant result is actually checked. Restore it
before final verification. Run targeted tests, independent review and scripts/check.

Ownership: one worker owns the new test and a concise evidence document; existing
production files require a demonstrated failure before edits. Gate Slot tests
out of listener-loom as needed. Parent owns all hardware, canonical integration,
commits and Kata closure. No playback or volume changes belong to this task.
