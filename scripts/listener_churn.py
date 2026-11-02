# ABOUTME: Repeats two-listener MP3 churn cycles against the board and samples /api/metrics once both slots read idle.
# ABOUTME: Idle slots in /api/metrics stand in for UART checked closes; the verdict and rejudge are pure, the driver runs only from main.
import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace

from decoder_integrity import FrameHash
from device_metrics import fetch
from network_fault import board_address, settings_problem
from stream_soak import LOSS_KEYS, Tail, decoder_command, port, seconds, stop, update_decoder


class Failure(Exception):
    pass


LISTENERS = 2
IDLE_WAIT_S = 15.0
IDLE_POLL_S = 0.5
FETCH_TIMEOUT_S = 3
# One heap step of the 47,104 -> 45,056 -> 43,008 B drops seen in the oct14-interruption batches (kata gcq8).
STEP_BYTES = 2048
TREND_CYCLES = 5
HEAP_KEYS = ('internal_largest', 'internal_free', 'internal_min_sum', 'psram_largest', 'psram_free')


def cycle_count(value):
    number = int(value)
    if number < 2 * TREND_CYCLES:
        raise argparse.ArgumentTypeError(f'must be at least {2 * TREND_CYCLES}: the trend compares the first and last {TREND_CYCLES} cycles')
    return number


def budget_seconds(cycles, seconds_each):
    return cycles * (seconds_each + 20) + 60


def slots_idle(value):
    return all(item['state'] == 'idle' for item in value['listeners'])


def metrics_sample(value):
    """Identity, settings, slot states, heap snapshot and loss counters from one validated /api/metrics body."""
    capture = value['capture']
    if capture is None:
        raise Failure('capture snapshot unavailable')
    return dict(boot_token=value.get('boot_token'), build_elf_sha256=value.get('build_elf_sha256'),
                settings=value.get('settings'), listener_states=[item['state'] for item in value['listeners']],
                loss={key: capture[key] for key in LOSS_KEYS}, **{key: capture[key] for key in HEAP_KEYS})


def listener_failed(job):
    # Same rule as network_fault's continuous(): exact frames alone do not clear a decoder that exited badly or stalled.
    return (job['ended'] != 'completed' or job['exit_code'] != 0 or job['discontinuities'] != 0
            or job['frames'] != job['expected_frames'])


def slope(values):
    """Least-squares slope in bytes per cycle over cycle index; None below two points."""
    count = len(values)
    if count < 2:
        return None
    mean_x = (count + 1) / 2
    mean_y = sum(values) / count
    return sum((x - mean_x) * (y - mean_y) for x, y in enumerate(values, 1)) / sum((x - mean_x) ** 2 for x in range(1, count + 1))


def first_move_cycle(baseline, cycles):
    """First cycle whose internal_largest differs from the baseline (cycle 0); a drop landing in cycle 1 reports 1."""
    for row in cycles:
        if row.get('internal_largest') is not None and row['internal_largest'] != baseline['internal_largest']:
            return row['cycle']
    return None


def verdict(baseline, cycles, requested, final):
    """Two answers: heap_passed is gcq8's acceptance over sampled cycles; listeners_clean is every decoder exact and idle reached."""
    reasons, observations = [], []
    sampled = [row for row in cycles if row.get('internal_largest') is not None]
    largest = [row['internal_largest'] for row in sampled]
    if len(cycles) != requested:
        reasons.append(f'{len(cycles)} of {requested} cycles recorded')
    for row in cycles:
        if row['error'] is not None:
            reasons.append(f"cycle {row['cycle']}: {row['error']}")
        if not row['idle']:
            # A sample taken with a slot still busy is not an idle-heap sample.
            reasons.append(f"cycle {row['cycle']}: listener slots not idle within {IDLE_WAIT_S:g} s: {row.get('listener_states_last_poll')}")
        for job in row['listeners']:
            if listener_failed(job):
                observations.append(dict(cycle=row['cycle'], label=job['label'], frames=job['frames'], expected_frames=job['expected_frames'],
                                         exit_code=job['exit_code'], discontinuities=job['discontinuities'], ended=job['ended']))
        if row.get('loss') is not None and row['loss'] != baseline['loss']:
            changed = {key: (baseline['loss'][key], row['loss'][key]) for key in LOSS_KEYS if row['loss'][key] != baseline['loss'][key]}
            reasons.append(f"cycle {row['cycle']}: loss counters changed {changed}")
        if row.get('boot_token') is not None and row['boot_token'] != baseline['boot_token']:
            reasons.append(f"cycle {row['cycle']}: boot token changed {baseline['boot_token']} -> {row['boot_token']}")
    if final is None:
        reasons.append('final metrics sample unavailable')
    elif final['boot_token'] != baseline['boot_token']:
        reasons.append(f"boot token changed {baseline['boot_token']} -> {final['boot_token']}")
    if largest:
        if abs(largest[-1] - largest[0]) > STEP_BYTES:
            reasons.append(f'internal_largest {largest[0]} -> {largest[-1]} B after the last cycle, beyond {STEP_BYTES} B')
        first = sum(largest[:TREND_CYCLES]) / len(largest[:TREND_CYCLES])
        last = sum(largest[-TREND_CYCLES:]) / len(largest[-TREND_CYCLES:])
        if first - last > STEP_BYTES:
            reasons.append(f'internal_largest trends down: mean of first {TREND_CYCLES} cycles {first:.1f} B, last {TREND_CYCLES} {last:.1f} B')
    heap_passed = not reasons
    listeners_clean = not observations and all(row['idle'] for row in cycles)
    return dict(passed=heap_passed, heap_passed=heap_passed, listeners_clean=listeners_clean, reasons=reasons, observations=observations,
                slope_bytes_per_cycle=slope(largest), first_move_cycle=first_move_cycle(baseline, cycles),
                baseline_internal_largest=baseline['internal_largest'],
                cycle_1_internal_largest=largest[0] if largest else None, last_internal_largest=largest[-1] if largest else None,
                step_bytes=STEP_BYTES, trend_cycles=TREND_CYCLES)


def summary_line(result, path):
    if result is None:
        return f'FAIL (no baseline) {path}'
    return (f"{'PASS' if result['passed'] else 'FAIL'} heap_passed={result['heap_passed']} listeners_clean={result['listeners_clean']} "
            f"observations={len(result['observations'])} {path}")


def rejudge(directory):
    """Recompute the verdict of a retained run, recounting each listener's discontinuities from its framehash; writes rejudge.json only."""
    directory = Path(directory)
    repo = Path(__file__).resolve().parent.parent
    source = (directory / 'result.json').read_bytes()
    result = json.loads(source)
    try:
        status = subprocess.run(['git', 'status', '--porcelain', '--', 'scripts/'], cwd=repo, capture_output=True, text=True,
                                timeout=10, check=True).stdout
    except (OSError, subprocess.SubprocessError):
        status = None
    cycles = []
    for row in result['cycles']:
        listeners = []
        for job in row['listeners']:
            job = dict(job, discontinuities=None, framehash_error=None)
            accounting = FrameHash(directory / f"{job['label']}.framehash")
            try:
                accounting.update(final=True)
            except ValueError as error:
                # A discontinuity stops the count at the gap; a missing or truncated file leaves the count unknown.
                job['framehash_error'] = str(error)
            finally:
                accounting.close()
            if accounting.header or accounting.frames:
                job['discontinuities'] = accounting.discontinuities
            listeners.append(job)
        cycles.append(dict(row, listeners=listeners))
    baseline = result['baseline']
    judged = None if baseline is None else verdict(baseline, cycles, result['cycles_requested'], result['final'])
    written = dict(source_result_sha256=hashlib.sha256(source).hexdigest(),
                   harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   harness_dirty=bool(status.strip()) if status is not None else None,
                   tolerances=dict(step_bytes=STEP_BYTES, trend_cycles=TREND_CYCLES), verdict=judged,
                   original=dict(passed=result['passed'], verdict=result['verdict']), cycles=cycles)
    (directory / 'rejudge.json').write_text(json.dumps(written, indent=2) + '\n')
    return written


def argument_parser():
    parser = argparse.ArgumentParser(
        prog='scripts/listener-churn',
        description='Repeat cycles of two real MP3 decoders reading /stream.mp3 for --seconds each, wait for both listener '
                    'slots idle in /api/metrics (standing in for UART checked closes; no serial port is opened), then keep that '
                    'one metrics sample per cycle. Requires autoplay off and MP3 active; changes no settings, never resets the '
                    'board and needs no root. The verdict reports internal_largest per cycle, its least-squares slope and the '
                    'first cycle it moved from the baseline (kata gcq8); cut listeners are reported separately as observations.')
    parser.add_argument('host', nargs='?', type=board_address, help='board IPv4 address (live run)')
    parser.add_argument('--cycles', type=cycle_count, default=30, help=f'churn cycles, at least {2 * TREND_CYCLES} (default: 30)')
    parser.add_argument('--seconds', type=seconds, default=10.0, help='decode seconds per listener per cycle (default: 10)')
    parser.add_argument('--out', type=Path, help='new output directory; must not exist (live run)')
    parser.add_argument('--stream-port', type=port, default=8000, help='stream TCP port (default: 8000)')
    parser.add_argument('--state-port', type=port, default=80, help='metrics HTTP port (default: 80)')
    parser.add_argument('--rejudge', type=Path, metavar='DIR',
                        help='recompute the verdict of a retained run from DIR/result.json and its framehash files into DIR/rejudge.json; '
                             'no board; takes no live-run arguments')
    return parser


def parse_args(argv=None):
    return argument_parser().parse_args(argv)


def start_decoder(label, out, host, decoder_args, processes, handles):
    errors_path = out / f'{label}.ffmpeg.log'
    error_handle = errors_path.open('wb')
    handles.append(error_handle)
    progress, hashes = out / f'{label}.progress', out / f'{label}.framehash'
    process = subprocess.Popen(decoder_command(decoder_args, host, progress, hashes),
                               stdout=subprocess.DEVNULL, stderr=error_handle, start_new_session=True)
    processes.append(process)
    return dict(label=label, process=process, tail=Tail(progress), errors=errors_path, log_tail=Tail(errors_path),
                accounting=FrameHash(hashes), decoded_audio_frames=0, decoded_seconds=0, timeline_seconds=0,
                advanced_at=time.monotonic(), ended=None)


def poll_decoder(job, decoder_args):
    if job['ended'] is not None:
        return
    try:
        update_decoder(job, decoder_args)
    except Exception as error:
        # A nonzero exit, short decode, discontinuity or stalled progress ends this listener's record.
        job['ended'] = str(error)
        stop(job['process'])
        return
    if job['process'].poll() is not None:
        job['ended'] = 'completed'


def listener_record(job, decoder_args):
    summary = job['accounting'].summary()
    return dict(label=job['label'], frames=summary['decoded_audio_frames'], expected_frames=round(decoder_args.seconds * 48000),
                exit_code=job['process'].poll(), ended=job['ended'], discontinuities=summary['decoded_discontinuities'])


def main(argv=None):
    parser = argument_parser()
    args = parser.parse_args(argv)
    if args.rejudge is not None:
        if args.host is not None or args.out is not None:
            parser.error('--rejudge takes no live-run arguments')
        if not (args.rejudge / 'result.json').is_file():
            parser.error(f'{args.rejudge / "result.json"} is missing')
        written = rejudge(args.rejudge)
        print(summary_line(written['verdict'], args.rejudge / 'rejudge.json'), flush=True)
        return 0
    if args.host is None or args.out is None:
        parser.error('a live run needs host and --out')
    if args.out.exists():
        parser.error(f'{args.out} exists; choose a new output directory')
    if not args.out.parent.is_dir():
        parser.error(f'{args.out.parent} is not a directory')
    if shutil.which('ffmpeg') is None:
        parser.error('ffmpeg is required')

    def interrupted(_number, _frame):
        raise KeyboardInterrupt
    previous = {number: signal.signal(number, interrupted) for number in (signal.SIGTERM, signal.SIGHUP)}
    umask = os.umask(0o077)
    args.out.mkdir(mode=0o700)
    started = time.monotonic()
    budget = budget_seconds(args.cycles, args.seconds)
    deadline = started + budget
    events = (args.out / 'events.jsonl').open('w')
    metrics_url = f'http://{args.host}:{args.state_port}/api/metrics'
    decoder_args = SimpleNamespace(path='/stream.mp3', stream_port=args.stream_port, seconds=args.seconds)
    processes, handles = [], []
    result = dict(passed=False, error=None, host=args.host, stream_port=args.stream_port, state_port=args.state_port,
                  cycles_requested=args.cycles, seconds=args.seconds, budget_seconds=budget,
                  baseline=None, final=None, cycles=[], verdict=None, log_directory=str(args.out))

    def elapsed():
        return round(time.monotonic() - started, 3)

    def record(kind, **fields):
        value = dict(kind=kind, elapsed_seconds=elapsed(), **fields)
        events.write(json.dumps(value) + '\n')
        events.flush()
        print(json.dumps(value), flush=True)

    def check_deadline():
        if time.monotonic() >= deadline:
            raise Failure('overall deadline exceeded')

    def snapshot():
        return fetch(metrics_url, FETCH_TIMEOUT_S)

    def run_cycle(index):
        jobs = [start_decoder(f'cycle-{index:02d}-listener-{n}', args.out, args.host, decoder_args, processes, handles)
                for n in range(1, LISTENERS + 1)]
        record('decoders_started', cycle=index, labels=[job['label'] for job in jobs])
        try:
            while not all(job['ended'] is not None for job in jobs):
                check_deadline()
                for job in jobs:
                    poll_decoder(job, decoder_args)
                time.sleep(0.5)
        finally:
            for job in jobs:
                stop(job['process'])
                job['tail'].close()
                job['log_tail'].close()
                job['accounting'].close()
        listeners = [listener_record(job, decoder_args) for job in jobs]
        record('decoders_ended', cycle=index, listeners=listeners)
        idle_started = time.monotonic()
        idle, value, states, unavailable = False, None, None, 0
        while time.monotonic() - idle_started < IDLE_WAIT_S:
            check_deadline()
            try:
                value = snapshot()
            except Exception as error:
                unavailable += 1  # One missed poll is not evidence; the bounded wait still ends the cycle.
                record('metrics_unavailable', cycle=index, error=repr(error))
            else:
                states = [item['state'] for item in value['listeners']]
                if slots_idle(value):
                    idle = True
                    break
            time.sleep(IDLE_POLL_S)
        row = dict(cycle=index, listeners=listeners, idle=idle, idle_wait_s=round(time.monotonic() - idle_started, 3),
                   idle_polls_unavailable=unavailable, listener_states_last_poll=states, error=None)
        # The poll that saw both slots idle is the cycle's sample; only a timed-out wait needs a fresh fetch.
        try:
            if not idle:
                value = snapshot()
            row.update(metrics_sample(value))
        except Exception as error:
            row['error'] = f'metrics sample unavailable: {error!r}'
        return row

    try:
        record('start', host=args.host, stream_port=args.stream_port, state_port=args.state_port,
               cycles=args.cycles, seconds=args.seconds, budget_seconds=budget)
        baseline = metrics_sample(snapshot())
        problem = settings_problem(baseline['settings'])
        if problem is not None:
            raise Failure(problem)
        if any(state != 'idle' for state in baseline['listener_states']):
            raise Failure(f"listener slots busy before cycle 1: {baseline['listener_states']}")
        result['baseline'] = baseline
        record('baseline', **baseline)
        for index in range(1, args.cycles + 1):
            check_deadline()
            row = run_cycle(index)
            result['cycles'].append(row)
            record('cycle', **row)
        result['final'] = metrics_sample(snapshot())
        record('final', **result['final'])
    except KeyboardInterrupt:
        result['error'] = 'interrupted'
        record('failure', error=result['error'])
    except Exception as error:
        result['error'] = str(error)
        record('failure', error=result['error'])
    finally:
        for process in reversed(processes):
            try:
                stop(process)
            except Exception as error:
                result['error'] = result['error'] or f'subprocess cleanup failed: {error}'
        for handle in handles:
            handle.close()
        if result['baseline'] is not None:
            result['verdict'] = verdict(result['baseline'], result['cycles'], args.cycles, result['final'])
            result['passed'] = result['error'] is None and result['verdict']['passed']
        result['elapsed_seconds'] = elapsed()
        pending = args.out / 'result.pending.json'
        pending.write_text(json.dumps(result, indent=2) + '\n')
        os.replace(pending, args.out / 'result.json')
        events.close()
        judged = None if result['verdict'] is None else dict(result['verdict'], passed=result['passed'])
        print(summary_line(judged, args.out / 'result.json'), flush=True)
        os.umask(umask)
        for number, handler in previous.items():
            signal.signal(number, handler)
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
