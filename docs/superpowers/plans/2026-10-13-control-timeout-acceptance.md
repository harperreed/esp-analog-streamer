# Control-read deadline and cancellation acceptance

This records the independent device observations for the October 11 control-recovery
plan. Doctor Biz authorized board playback and checked Stop. It does not enable
automatic playback restart or resolve pqqq's ownership protocol gap.

## Probe

Use a throwaway source patch, archived with its exact image. After at least
15 seconds observing the same owned intent, arm one description read per boot.
Send the real HTTP request to the selected speaker, then stall only the control
thread until its original absolute deadline plus 50 ms. Resume the unchanged
DeadlineReader and recovery code. No fabricated response, error or state;
capture, media transport and retry budgets retain their ordinary implementation.

This injects a local control-thread scheduling stall. It is not packet loss or
an AP outage. The sleep deliberately prevents this one read from completing on
time, even if a real response is already buffered. Record that distinction.

## Acceptance

1. Fresh diagnostic boot, verified image/settings/network lease, selected Sonos
   STOPPED, idle listeners. One authorized Play must establish ownership within
   this boot. A URI may repeat across boots; inequality is not ownership proof.
2. Retention trial: fault reached once; genuine deadline failure; Degraded and
   Recovering followed by Playing with original URI/intent/network generation.
   Require continuing media progress and bounded listener use. No recovery
   Play, SetURI or Stop. End with explicit checked Stop and confirmed cleanup.
3. Fresh diagnostic boot, repeat preconditions. Cancellation trial: explicit
   checked Stop accepted during the stall. After the read returns, no late
   observation may restore delivery. Require remote STOPPED and idle listeners.
4. Both trials retain failures and raw UART/API receipts. Strict capture
   counters must pass, including startup. Quiet ADC, payload and position do
   not prove source-exact or audible continuity. No workstation audio, mixer,
   volume or access-control changes; no AP or hosted-auth dependency.
5. Remove the temporary patch, verify original source bytes, restore the exact
   ordinary image, and verify stopped MP3/Medium, recognition/autoplay off,
   settings unchanged, idle listeners and remote STOPPED. Reap owned processes.

Parent owns canonical checks, build, hardware, review and commit. Sol 6.1 workers
own the disjoint temporary firmware patch and host driver. Host checks cover
the real driver's event/identity/parser boundaries; independent review precedes
hardware. The existing timeout tests and canonical check remain required.

No whole-ticket closure follows from this bounded control-only gate. The
original interruption matrix, safe playback restart and foreign-source tests
remain required. If a trial fails, retain its result and diagnose the actual
failure before selecting another run.

## Acquisition checkpoint

The diagnostic image passed all 27 canonical stages in `run.YiikcIHL`.
Two retention acquisitions remain failed in their original receipts. The first
stopped before injection because the driver dereferenced nullable network
metadata. Four actual localhost HTTP regressions now cover bounded fresh reads;
missing metadata cannot substitute for proof of the current lease.

The second reached the deadline and natural recovery, then its UART drain
stopped after socket close but before the trailing loss export. An unchanged,
archived evaluator passes against the complete parent UART and original API
observations. This is a separate offline verdict, not a changed acquisition
result. The original listener spans the 4.600698-second stall and the full
recovery observation. No third retention acquisition is planned.

The independent cancellation observation accepted Stop during the stalled read,
then returned Interrupted and remained stopped. Its aggregate acquisition failed
an incorrect demand for a post-stall export: the actual close batch had already
been exported during the stall. The shared gate now requires a clean export
whose valid range covers the owner's close, with independent stall/state checks.
Two more fixture regressions pass; final host count is 27. No more hardware
acquisitions followed. Both complete retained observations pass offline checks;
all three original acquisition failures remain intact.

Source is restored byte for byte. Five firmware stages pass in `run.M3NzhK5v`;
ordinary ELF `1d2105b` matches the saved image exactly and is flashed. Final boot
`a217c3442545de892c0ce8d7b061428e` is stopped MP3/Medium, autoplay/recognition off,
listeners idle and remote STOPPED/OK. All owned processes ended. See October 13
control-timeout evidence for hashes, reviews and limits. The bounded control-only
observation is complete; pqqq's full recovery/takeover/interruption gate is not.
