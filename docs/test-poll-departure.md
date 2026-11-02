# Deadline writer departures

Kata 6hjs validates the existing retry-send policy; it does not start from a
confirmed production defect. Tests use the production `DeadlineWriter`, real
loopback TCP sockets, small socket buffers and actual `Slot` ownership. No
syscall mocks, descriptor close races or `SO_ERROR` reads are involved.

Run `cargo test -p esp-shazam-core --test poll_departure -- --nocapture` from
the repository root. The target skips `listener-loom`, whose atomics require a
Loom model rather than ordinary socket threads.

The half-close test waits for read EOF after the peer closes only its write
half, then checks all 65,536 response bytes. It catches treating read EOF as
permission to discard a still-valid outbound response. The orderly full-close
test allows a locally accepted prefix before requiring a send errno and a
bounded return. Local acceptance does not prove remote delivery.

The reset test first fills a real socket until a 50 ms write deadline expires,
checks positive accepted bytes and actual WouldBlock/poll activity, then checks
a received 1,024-byte prefix. A zero linger timeout forces peer reset during
the following large write. It checks original `ECONNRESET`, flag restoration in
both original blocking modes, one final send error and at most one error-ready
poll. A delayed worker may see the reset directly in send; a sleep cannot prove
that it entered poll first. Printed diagnostics record actual branch coverage.
Channel receive timeouts bound worker waits; the peer reader has a socket timeout.
The actual `Slot` remains held through socket drop, then becomes available.
This exercises the ownership contract, not firmware close implementation.

## POSIX and the pinned target

[POSIX poll](https://pubs.opengroup.org/onlinepubs/9799919799/functions/poll.html)
reports error and hangup conditions without requiring them in the event mask.
Its hangup/writable relationship does not determine lwIP behavior.
[POSIX send](https://pubs.opengroup.org/onlinepubs/000095399/functions/send.html)
reports local connection errors; successful send is local acceptance.

Inspection used the repository's pinned
`firmware/.embuild/espressif/esp-idf/v5.5.5` source. In lwIP `sockets.c`,
`lwip_pollscan` independently checks `sendevent` and `errevent`, so POLLOUT and
POLLERR can coexist. It does not set POLLHUP in this scan. An invalid socket
sets POLLNVAL and returns -1. The platform `components/newlib/platform_include/sys/poll.h`
defines POLLNVAL as 128; generic lwIP fallback values are not this ABI.
No invalid borrowed `TcpStream` is constructed to exercise that case.

`lwip_send` calls `netconn_write_partly`, maps the returned lwIP error through
`err_to_errno`, and returns written bytes only for ERR_OK. The vector write
path honors the connection's nonblocking state. `api/err.c` maps ERR_RST to
ECONNRESET. Retrying send after error readiness retains that useful OS error
rather than inventing a classification or consuming SO_ERROR.

## Retained device evidence

The diagnostic run `20261010T174257Z`, image commit `0d9d014`, ELF SHA-256
`5ee637452567a4ef37af6de913a27c8a502d4f8b9b5c1a41fca8ef9fde3dbf67`,
recorded both FLAC departures in
`target/runtime-codec/matched/20261010T174257Z/uart.log` lines 149–158.
Each peer recorded one error-ready poll, one send error, zero
ready-then-WouldBlock events and `Connection reset by peer (os error 104)`.
Close completed in 19 ms and 1 ms, with two and one slots held respectively
before release. Those were bounded receiver cleanup departures, not an
injected orderly-half-close experiment.

Firmware `ListenerConnection::drop` calls `socket_close::close` before its
`Slot` field drops. A failed close keeps ownership through restart. Host std
Drop does not prove that ESP-IDF close path; the saved target close records
are separate evidence.

Host results and final verification are recorded in the task report. The
remaining limit is platform scope: host FIN/reset behavior plus pinned source
and observed target reset departures cannot prove every target schedule or an
injected target orderly half-close. No production change is justified unless
these checks expose a failure.
