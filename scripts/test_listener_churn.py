# ABOUTME: Checks the listener-churn verdict, metrics sampling, rejudge and argument handling on the host.
# ABOUTME: Uses dict fixtures, real framehash files and a subprocess of the module; never contacts a board.
import contextlib
import hashlib
import io
import json
import os
import signal
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import device_metrics as dm
import listener_churn as churn

SCRIPTS = Path(__file__).resolve().parent
FRAMEHASH_HEADER = ['#format: frame checksums', '#version: 2', '#hash: SHA256', '#software: Lavf62.3.100', '#tb 0: 1/48000',
                    '#media_type 0: audio', '#codec_id 0: pcm_s32le', '#sample_rate 0: 48000', '#channel_layout_name 0: stereo',
                    '#stream#, dts,        pts, duration,     size, hash']


def snapshot(states=('idle', 'idle'), largest=47_104, free=120_000, overflows=0, boot_token='a' * 32):
    listeners = [dict(generation=1 if state != 'idle' else 0, state=state, sample_us=10,
                      media_age_us=5 if state in ('writing', 'ready') else None,
                      write_us=0 if state in ('writing', 'ready') else None) for state in states]
    capture = dict.fromkeys(dm.CAPTURE_U32, 0)
    capture.update(queue_overflows=overflows, read_frames=480_000, capture_begin_us=0, capture_end_us=10)
    capture.update(internal_free=free, internal_largest=largest, internal_min_sum=free - 1000,
                   psram_free=3_000_000, psram_largest=2_000_000, psram_min_sum=2_900_000)
    return dict(schema_version=1, served_device_us=20, boot_token=boot_token, build_elf_sha256='b' * 64,
                settings=dict(active_format='mp3', next_play_format='mp3', next_play_buffer='med', buffer_available=True,
                              autoplay=False, recognition_enabled=False, pga_gain=0),
                network=dict(generation=3, connected=True, disconnects=0), listeners=listeners, capture=capture,
                encoder=None, listener_loss=None, encoder_loss=None)


def listener(index, n, frames=480_000, exit_code=0, ended='completed', discontinuities=0):
    return dict(label=f'cycle-{index:02d}-listener-{n}', frames=frames, expected_frames=480_000, exit_code=exit_code,
                ended=ended, discontinuities=discontinuities)


def cut(index, n, frames):
    return listener(index, n, frames, exit_code=251, ended='ffmpeg integrity failure: [error] Stream ends prematurely')


def cycle(index, largest=47_104, listeners=None, idle=True, overflows=0, error=None, boot_token='a' * 32):
    row = dict(cycle=index, listeners=listeners or [listener(index, 1), listener(index, 2)], idle=idle, idle_wait_s=0.5, error=error)
    if error is None:
        row.update(churn.metrics_sample(snapshot(largest=largest, overflows=overflows, boot_token=boot_token)))
    return row


def cycles(largests, **overrides):
    return [cycle(index, largest, **overrides) for index, largest in enumerate(largests, 1)]


def write_framehash(path, packets, gap_at=None):
    lines, pts = list(FRAMEHASH_HEADER), 0
    for number in range(packets):
        if number == gap_at:
            pts += 1152
        lines.append(f'0, {pts}, {pts}, 1152, {1152 * 8}, {"0" * 64}')
        pts += 1152
    path.write_text('\n'.join(lines) + '\n')


class MetricsSampleTests(unittest.TestCase):
    def test_sample_keeps_identity_settings_states_heap_and_loss(self):
        value = dm.validate(snapshot(states=('writing', 'idle'), overflows=3))
        sample = churn.metrics_sample(value)
        self.assertEqual((sample['boot_token'], sample['build_elf_sha256']), ('a' * 32, 'b' * 64))
        self.assertEqual(sample['settings']['active_format'], 'mp3')
        self.assertEqual(sample['listener_states'], ['writing', 'idle'])
        self.assertEqual((sample['internal_largest'], sample['internal_free'], sample['internal_min_sum']),
                         (47_104, 120_000, 119_000))
        self.assertEqual((sample['psram_largest'], sample['psram_free']), (2_000_000, 3_000_000))
        self.assertEqual(sample['loss'], dict(dict.fromkeys(churn.LOSS_KEYS, 0), queue_overflows=3))

    def test_sample_requires_a_capture_snapshot(self):
        bare = snapshot()
        bare['capture'] = None
        with self.assertRaises(churn.Failure):
            churn.metrics_sample(dm.validate(bare))

    def test_slots_idle_reads_both_states(self):
        self.assertTrue(churn.slots_idle(snapshot()))
        self.assertFalse(churn.slots_idle(snapshot(states=('writing', 'idle'))))

    def test_listener_failed_rule_matches_network_fault(self):
        self.assertFalse(churn.listener_failed(listener(1, 1)))
        for row in [listener(1, 1, frames=479_000), listener(1, 1, exit_code=-15, ended='no decoder progress for 20 seconds'),
                    listener(1, 1, discontinuities=1), listener(1, 1, ended='stopped'), listener(1, 1, discontinuities=None)]:
            self.assertTrue(churn.listener_failed(row), row)


class VerdictTests(unittest.TestCase):
    def verdict(self, rows, requested=None, final_token='a' * 32, baseline_overflows=0, baseline_largest=47_104):
        baseline = churn.metrics_sample(snapshot(overflows=baseline_overflows, largest=baseline_largest))
        final = churn.metrics_sample(snapshot(boot_token=final_token))
        return churn.verdict(baseline, rows, requested if requested is not None else len(rows), final)

    def heap_reasons(self, result, text):
        return [reason for reason in result['reasons'] if text in reason]

    def test_flat_run_passes_both_questions(self):
        result = self.verdict(cycles([47_104] * 30))
        self.assertEqual((result['passed'], result['heap_passed'], result['listeners_clean']), (True, True, True))
        self.assertEqual((result['reasons'], result['observations']), ([], []))
        self.assertEqual((result['slope_bytes_per_cycle'], result['first_move_cycle']), (0.0, None))
        self.assertEqual((result['baseline_internal_largest'], result['cycle_1_internal_largest'], result['last_internal_largest']),
                         (47_104, 47_104, 47_104))

    def test_one_step_wobble_still_passes(self):
        result = self.verdict(cycles([47_104, 45_056] + [47_104] * 28))
        self.assertTrue(result['passed'], result['reasons'])
        self.assertEqual(result['first_move_cycle'], 2)

    def test_drift_down_fails_with_slope_and_first_move(self):
        result = self.verdict(cycles([47_104] * 10 + [45_056] * 10 + [43_008] * 10))
        self.assertEqual((result['passed'], result['heap_passed'], result['listeners_clean']), (False, False, True))
        self.assertEqual(result['first_move_cycle'], 11)
        self.assertAlmostEqual(result['slope_bytes_per_cycle'], -182.247, places=2)
        self.assertTrue(self.heap_reasons(result, '47104 -> 43008'), result['reasons'])

    def test_last_within_tolerance_but_trend_down_fails(self):
        result = self.verdict(cycles([47_104] * 25 + [43_008] * 4 + [47_104]))
        self.assertFalse(result['heap_passed'])
        self.assertTrue(self.heap_reasons(result, 'trend'), result['reasons'])
        self.assertEqual(result['first_move_cycle'], 26)
        self.assertLess(result['slope_bytes_per_cycle'], 0)

    def test_drop_landing_in_cycle_1_is_visible_against_the_baseline(self):
        result = self.verdict(cycles([45_056] * 10), baseline_largest=47_104)
        self.assertTrue(result['passed'], result['reasons'])
        self.assertEqual((result['first_move_cycle'], result['baseline_internal_largest'], result['cycle_1_internal_largest']),
                         (1, 47_104, 45_056))

    def test_cut_listeners_are_observations_not_heap_failures(self):
        rows = cycles([47_104] * 10)
        rows[2] = cycle(3, listeners=[listener(3, 1), listener(3, 2, frames=479_000)])
        result = self.verdict(rows)
        self.assertEqual((result['passed'], result['heap_passed'], result['listeners_clean']), (True, True, False))
        self.assertEqual(result['reasons'], [])
        self.assertEqual([(item['cycle'], item['label'], item['frames']) for item in result['observations']],
                         [(3, 'cycle-03-listener-2', 479_000)])

    def test_exact_frames_with_a_failed_decoder_still_observed(self):
        for bad in [listener(4, 1, exit_code=-15, ended='no decoder progress for 20 seconds'), listener(4, 1, discontinuities=1)]:
            rows = cycles([47_104] * 10)
            rows[3] = cycle(4, listeners=[bad, listener(4, 2)])
            result = self.verdict(rows)
            self.assertFalse(result['listeners_clean'], bad)
            self.assertEqual([item['label'] for item in result['observations']], ['cycle-04-listener-1'])
            self.assertEqual(result['observations'][0]['ended'], bad['ended'])

    def test_real_run_shape_passes_heap_with_three_observations(self):
        # oct14-churn-1: 47104 B except 45056 B at cycles 21-22; the board cut three listeners in cycles 6 and 13.
        rows = cycles([47_104] * 20 + [45_056] * 2 + [47_104] * 8)
        rows[5] = cycle(6, listeners=[cut(6, 1, 210_816), cut(6, 2, 213_120)])
        rows[12] = cycle(13, listeners=[listener(13, 1), cut(13, 2, 321_408)])
        result = self.verdict(rows)
        self.assertEqual((result['passed'], result['heap_passed'], result['listeners_clean']), (True, True, False))
        self.assertEqual(len(result['observations']), 3)
        self.assertEqual(result['first_move_cycle'], 21)
        self.assertAlmostEqual(result['slope_bytes_per_cycle'], -10.935, places=2)

    def test_boot_token_change_fails_at_the_end_and_mid_run(self):
        result = self.verdict(cycles([47_104] * 10), final_token='c' * 32)
        self.assertFalse(result['heap_passed'])
        self.assertTrue(self.heap_reasons(result, 'boot token'), result['reasons'])
        rows = cycles([47_104] * 10)
        rows[6] = cycle(7, boot_token='d' * 32)
        result = self.verdict(rows)
        self.assertFalse(result['heap_passed'])
        self.assertTrue(self.heap_reasons(result, 'cycle 7: boot token'), result['reasons'])

    def test_loss_counters_idle_timeout_missing_cycles_and_sample_errors_fail(self):
        for rows, requested, text in [(cycles([47_104] * 10, overflows=1), 10, 'queue_overflows'),
                                      (cycles([47_104] * 10, idle=False), 10, 'idle'),
                                      (cycles([47_104] * 8), 10, '8 of 10'),
                                      (cycles([47_104] * 10, error='metrics sample unavailable: refused'), 10, 'refused')]:
            result = self.verdict(rows, requested)
            self.assertFalse(result['heap_passed'], text)
            self.assertTrue(self.heap_reasons(result, text), (text, result['reasons']))
            if text == 'idle':
                self.assertFalse(result['listeners_clean'])

    def test_rows_without_a_sample_are_skipped_by_slope_and_first_move(self):
        rows = cycles([47_104] * 10)
        rows[4] = cycle(5, error='metrics sample unavailable: timeout')
        result = self.verdict(rows)
        self.assertFalse(result['heap_passed'])
        self.assertEqual((result['slope_bytes_per_cycle'], result['first_move_cycle'], result['last_internal_largest']), (0.0, None, 47_104))

    def test_missing_final_sample_fails_but_keeps_slope(self):
        baseline = churn.metrics_sample(snapshot())
        result = churn.verdict(baseline, cycles([47_104, 45_056]), 2, None)
        self.assertFalse(result['passed'])
        self.assertEqual((result['slope_bytes_per_cycle'], result['first_move_cycle']), (-2048.0, 2))

    def test_slope_is_least_squares_over_cycle_index(self):
        self.assertEqual(churn.slope([10, 20, 30, 40]), 10.0)
        self.assertEqual(churn.slope([5]), None)
        self.assertEqual(churn.slope([]), None)

    def test_summary_line_shows_both_flags_and_observation_count(self):
        rows = cycles([47_104] * 10)
        rows[2] = cycle(3, listeners=[listener(3, 1), cut(3, 2, 1000)])
        line = churn.summary_line(self.verdict(rows), Path('run/result.json'))
        self.assertTrue(line.startswith('PASS heap_passed=True listeners_clean=False observations=1 '), line)


class RejudgeTests(unittest.TestCase):
    def synthetic_run(self, run):
        # Rows in the oct14-churn-1 shape: listeners recorded before `discontinuities` existed, loss under `loss`.
        rows = cycles([47_104] * 10)
        for row in rows:
            for item in row['listeners']:
                del item['discontinuities']
                write_framehash(run / f"{item['label']}.framehash", 417)
        write_framehash(run / 'cycle-04-listener-2.framehash', 417, gap_at=200)
        baseline = churn.metrics_sample(snapshot())
        result = dict(passed=True, error=None, host='192.168.23.217', cycles_requested=10, seconds=10.0, baseline=baseline,
                      final=baseline, cycles=rows, verdict=dict(passed=True), log_directory=str(run))
        (run / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        (run / 'events.jsonl').write_text('')

    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def test_rejudge_recomputes_discontinuities_from_retained_framehash_without_touching_result(self):
        with tempfile.TemporaryDirectory() as directory, contextlib.redirect_stdout(io.StringIO()):
            run = Path(directory) / 'run'
            run.mkdir()
            self.synthetic_run(run)
            before = sorted(path.name for path in run.iterdir())
            source = self.digest(run / 'result.json')
            self.assertEqual(churn.main(['--rejudge', str(run)]), 0)
            self.assertEqual(sorted(path.name for path in run.iterdir()), sorted(before + ['rejudge.json']))
            self.assertEqual(self.digest(run / 'result.json'), source)
            written = json.loads((run / 'rejudge.json').read_text())
            self.assertEqual(written['source_result_sha256'], source)
            self.assertEqual(written['harness_sha256'], self.digest(SCRIPTS / 'listener_churn.py'))
            self.assertIn(written['harness_dirty'], (True, False, None))
            self.assertEqual(written['tolerances'], dict(step_bytes=2048, trend_cycles=5))
            self.assertEqual(written['original'], dict(passed=True, verdict=dict(passed=True)))
            verdict = written['verdict']
            self.assertEqual((verdict['passed'], verdict['heap_passed'], verdict['listeners_clean']), (True, True, False))
            self.assertEqual([(item['cycle'], item['label']) for item in verdict['observations']], [(4, 'cycle-04-listener-2')])
            self.assertEqual(written['cycles'][3]['listeners'][1]['discontinuities'], 1)
            self.assertEqual(written['cycles'][3]['listeners'][0]['discontinuities'], 0)

    def test_rejudge_fails_closed_on_a_missing_framehash(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / 'run'
            run.mkdir()
            self.synthetic_run(run)
            (run / 'cycle-02-listener-1.framehash').unlink()
            written = churn.rejudge(run)
            self.assertIsNone(written['cycles'][1]['listeners'][0]['discontinuities'])
            self.assertIn('framehash', written['cycles'][1]['listeners'][0]['framehash_error'])
            self.assertEqual([item['label'] for item in written['verdict']['observations']], ['cycle-02-listener-1', 'cycle-04-listener-2'])


class ArgumentTests(unittest.TestCase):
    def test_defaults_and_budget(self):
        args = churn.parse_args(['192.168.23.217', '--out', 'churn-out'])
        self.assertEqual((args.cycles, args.seconds, args.stream_port, args.state_port), (30, 10.0, 8000, 80))
        self.assertEqual(args.out, Path('churn-out'))
        self.assertEqual(churn.budget_seconds(30, 10.0), 30 * 30 + 60)

    def test_bad_arguments_exit_2_and_create_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            live = ['192.168.23.217', '--out', 'x']
            for argv in [[], ['esp-shazam.local', '--out', 'x'], ['192.168.23.217'], [*live, '--cycles', '0'], [*live, '--cycles', '9'],
                         [*live, '--seconds', '0.5'], [*live, '--stream-port', '0'], [*live, '--state-port', '70000'],
                         ['192.168.23.217', '--out', '.'], ['--rejudge', 'missing'], ['--rejudge', '.', *live],
                         ['--rejudge', '.', '--out', 'x'], ['--help']]:
                result = subprocess.run([sys.executable, '-B', str(SCRIPTS / 'listener_churn.py'), *argv], cwd=directory,
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 0 if argv == ['--help'] else 2, (argv, result.stderr))
                self.assertEqual(list(Path(directory).iterdir()), [], argv)
            self.assertIn('slots idle in /api/metrics', subprocess.run([sys.executable, '-B', str(SCRIPTS / 'listener_churn.py'), '--help'],
                                                                      capture_output=True, text=True, timeout=10).stdout)

    def test_missing_ffmpeg_is_refused_before_the_output_directory_exists(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, '-B', str(SCRIPTS / 'listener_churn.py'), '192.168.23.217', '--out', 'run'],
                                    cwd=directory, capture_output=True, text=True, timeout=10, env=dict(os.environ, PATH='/nonexistent'))
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn('ffmpeg', result.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_unreachable_host_writes_failed_result_and_restores_process_state(self):
        # 127.0.0.1 with a closed port: the baseline fetch fails, the run records it and still writes result.json.
        handlers = {number: signal.getsignal(number) for number in (signal.SIGTERM, signal.SIGHUP)}
        umask = os.umask(0o022)
        os.umask(umask)
        with tempfile.TemporaryDirectory() as directory, contextlib.redirect_stdout(io.StringIO()):
            out = Path(directory) / 'run'
            code = churn.main(['127.0.0.1', '--out', str(out), '--cycles', '10', '--state-port', '9', '--stream-port', '9'])
            self.assertEqual(code, 1)
            self.assertEqual(oct(out.stat().st_mode & 0o777), '0o700')
            result = json.loads((out / 'result.json').read_text())
            self.assertFalse(result['passed'])
            self.assertIsNotNone(result['error'])
            self.assertFalse((out / 'result.pending.json').exists())
            self.assertEqual(sorted(path.name for path in out.iterdir()), ['events.jsonl', 'result.json'])
        self.assertEqual({number: signal.getsignal(number) for number in handlers}, handlers)
        current = os.umask(umask)
        self.assertEqual(current, umask)


if __name__ == '__main__':
    unittest.main()
