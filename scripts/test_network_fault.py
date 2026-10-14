# ABOUTME: Checks the network-fault harness's qdisc planning, qdisc-show parsing and trial verdicts on the host.
# ABOUTME: Runs tc unprivileged for real usage and show output; never installs a qdisc or contacts a board.
import contextlib
import inspect
import io
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import network_fault as nf

SCRIPTS = Path(__file__).resolve().parent
BOARD = '192.168.23.217'

# Captured from `tc -s qdisc show dev enp5s0` on the workstation, 2026-10-13 (iproute2-6.19.0).
ENP5S0_STATS = """qdisc mq 0: root
 Sent 1234 bytes 10 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc fq_codel 0: parent :4 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
  maxpacket 0 drop_overlimit 0 new_flow_count 0 ecn_mark 0
  new_flows_len 0 old_flows_len 0
qdisc fq_codel 0: parent :3 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
 Sent 0 bytes 0 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
qdisc fq_codel 0: parent :2 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
qdisc fq_codel 0: parent :1 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
"""
ENP5S0_PLAIN = """qdisc mq 0: root
qdisc fq_codel 0: parent :4 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
qdisc fq_codel 0: parent :3 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
qdisc fq_codel 0: parent :2 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
qdisc fq_codel 0: parent :1 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64
"""


def tc_output(*argv):
    result = subprocess.run(['tc', *argv], capture_output=True, text=True, timeout=10)
    return result.stdout + result.stderr


def departure(peer, end, device_us, at_s):
    return dict(kind='departure', peer=peer, end=end, device_us=device_us, at_s=at_s)


def close(peer, device_us, at_s):
    return dict(kind='close', peer=peer, device_us=device_us, at_s=at_s)


def admission(peer, device_us, at_s):
    return dict(kind='admission', peer=peer, device_us=device_us, at_s=at_s)


def decoder(label, frames, expected, discontinuities=0, exit_code=0, ended=None, ended_s=None):
    if ended is None:
        ended = 'completed' if frames == expected and exit_code == 0 else 'cut'
    return dict(kind='decoder', label=label, frames=frames, expected_frames=expected,
                discontinuities=discontinuities, exit_code=exit_code, ended=ended, ended_s=ended_s)


def fresh(label, ended_s, ended='completed'):
    return dict(kind='fresh', label=label, ended=ended, ended_s=ended_s)


def wav(closed_s):
    return dict(kind='wav', closed_s=closed_s)


RESET = 'socket error kind=ConnectionReset raw_os_error=Some(104)'


def reset_after(peer, ended_s, device_us=90_000_000, delay=0.4):
    # The board logs the harness's own socket close as a ConnectionReset departure shortly after.
    return departure(peer, RESET, device_us, ended_s + delay)


def metrics(at_s, age=200_000, internal=100_000, psram=2_000_000, overflows=0):
    return dict(kind='metrics', at_s=at_s, media_age_us=age, internal_largest=internal, psram_largest=psram,
                capture=dict(queue_overflows=overflows, software_dropped_frames=0, read_errors=0,
                             restarts=0, discontinuities=0))


def injection(phase, at_s):
    return dict(kind='injection', phase=phase, at_s=at_s)


class QdiscPlanTests(unittest.TestCase):
    def test_privileged_prefixes_non_interactive_sudo(self):
        self.assertEqual(nf.privileged(['tc', '-V']), ['sudo', '-n', 'tc', '-V'])

    def test_install_plan_is_egress_prio_then_ifb_ingress_without_sudo(self):
        plan = nf.qdisc_plan('enp5s0', BOARD)
        self.assertEqual(plan, [
            ['tc', 'qdisc', 'replace', 'dev', 'enp5s0', 'root', 'handle', '1:', 'prio', 'bands', '4'],
            ['tc', 'qdisc', 'add', 'dev', 'enp5s0', 'parent', '1:4', 'handle', '10:', 'netem'],
            ['tc', 'filter', 'add', 'dev', 'enp5s0', 'parent', '1:', 'protocol', 'ip', 'prio', '1', 'u32',
             'match', 'ip', 'dst', BOARD + '/32', 'flowid', '1:4'],
            ['ip', 'link', 'add', 'ifb0', 'type', 'ifb'],
            ['ip', 'link', 'set', 'ifb0', 'up'],
            ['tc', 'qdisc', 'add', 'dev', 'enp5s0', 'handle', 'ffff:', 'ingress'],
            ['tc', 'filter', 'add', 'dev', 'enp5s0', 'parent', 'ffff:', 'protocol', 'ip', 'prio', '1', 'u32',
             'match', 'ip', 'src', BOARD + '/32', 'action', 'mirred', 'egress', 'redirect', 'dev', 'ifb0'],
            ['tc', 'qdisc', 'add', 'dev', 'ifb0', 'root', 'handle', '1:', 'netem'],
        ])
        self.assertTrue(all('sudo' not in argv for argv in plan))
        # The default priomap sends Linux priorities 6-7 to band 0; the netem band must be one nothing maps to.
        self.assertNotIn('1:1', ' '.join(' '.join(argv) for argv in plan))
        self.assertEqual(nf.qdisc_plan('eth1', BOARD, ifb='ifb3')[3], ['ip', 'link', 'add', 'ifb3', 'type', 'ifb'])

    def test_teardown_removes_ifb_netem_ingress_root_then_link(self):
        self.assertEqual(nf.qdisc_teardown('enp5s0', BOARD), [
            ['tc', 'qdisc', 'del', 'dev', 'ifb0', 'root'],
            ['tc', 'qdisc', 'del', 'dev', 'enp5s0', 'ingress'],
            ['tc', 'qdisc', 'del', 'dev', 'enp5s0', 'root'],
            ['ip', 'link', 'del', 'ifb0'],
        ])

    def test_plan_rejects_non_ipv4_board_and_odd_names(self):
        for bad in ('esp-shazam.local', '192.168.23.217/24', '', 'fe80::1'):
            with self.assertRaises(ValueError):
                nf.qdisc_plan('enp5s0', bad)
        for bad in ('', 'enp5s0 x', 'a' * 16, '-x'):
            with self.assertRaises(ValueError):
                nf.qdisc_plan(bad, BOARD)

    def test_netem_change_names_parent_and_builds_loss_or_rate(self):
        self.assertEqual(nf.netem_change('ifb0', 'root', loss_percent=100),
                         ['tc', 'qdisc', 'change', 'dev', 'ifb0', 'root', 'netem', 'loss', '100%'])
        self.assertEqual(nf.netem_change('enp5s0', '1:4', loss_percent=0),
                         ['tc', 'qdisc', 'change', 'dev', 'enp5s0', 'parent', '1:4', 'netem', 'loss', '0%'])
        self.assertEqual(nf.netem_change('ifb0', 'root', rate_bit_per_s=256_000),
                         ['tc', 'qdisc', 'change', 'dev', 'ifb0', 'root', 'netem', 'rate', '256000bit'])
        # Zero clears a rate: `change` is expected to keep an unspecified rate (verify at the device run).
        self.assertEqual(nf.netem_change('ifb0', 'root', rate_bit_per_s=0)[-2:], ['rate', '0bit'])
        with self.assertRaises(ValueError):
            nf.netem_change('ifb0', 'root')
        with self.assertRaises(ValueError):
            nf.netem_change('ifb0', 'root', loss_percent=101)
        with self.assertRaises(ValueError):
            nf.netem_change('ifb0', '1:x', loss_percent=1)
        self.assertFalse(hasattr(nf, 'NETEM_ACCEPTS_SEED'))

    def test_this_tc_usage_lists_loss_and_rate(self):
        if shutil.which('tc') is None:
            self.skipTest('tc not installed')
        usage = tc_output('qdisc', 'add', 'netem', 'help')
        self.assertIn('loss', usage)
        self.assertIn('rate', usage)

    def test_preinstall_refuses_non_default_trees_and_existing_ifb(self):
        self.assertIsNone(nf.preinstall_problem(ENP5S0_PLAIN, ifb_exists=False))
        self.assertIsNone(nf.preinstall_problem('qdisc fq_codel 0: root refcnt 2 limit 10240p\n', ifb_exists=False))
        self.assertIsNone(nf.preinstall_problem('qdisc pfifo_fast 0: root refcnt 2 bands 3 priomap 1 2 2 2 1 2 0 0 1 1 1 1 1 1 1 1\n', ifb_exists=False))
        self.assertIn('ifb', nf.preinstall_problem(ENP5S0_PLAIN, ifb_exists=True))
        self.assertIn('prio', nf.preinstall_problem(ENP5S0_PLAIN.replace('mq 0: root', 'prio 1: root'), ifb_exists=False))
        self.assertIn('ingress', nf.preinstall_problem(ENP5S0_PLAIN + 'qdisc ingress ffff: parent ffff:fff1 ----------------\n', ifb_exists=False))
        self.assertIn('clsact', nf.preinstall_problem(ENP5S0_PLAIN + 'qdisc clsact ffff: parent ffff:fff1\n', ifb_exists=False))
        self.assertIn('root', nf.preinstall_problem('', ifb_exists=False))

    def test_stall_and_deficit_profiles(self):
        stall = nf.stall_profile(2.5)
        self.assertEqual(stall, dict(kind='stall', seconds=2.5, egress=dict(loss_percent=100), ingress=dict(loss_percent=100)))
        deficit = nf.deficit_profile(0.8)
        self.assertEqual(deficit, dict(kind='deficit', fraction=0.8, egress=None, ingress=dict(rate_bit_per_s=256_000)))
        self.assertEqual(nf.deficit_profile(0.5, bitrate_bit_per_s=128_000)['ingress'], dict(rate_bit_per_s=64_000))
        for bad in (0, -1, float('nan')):
            with self.assertRaises(ValueError):
                nf.stall_profile(bad)
        for bad in (0, 1, 1.5, -0.2):
            with self.assertRaises(ValueError):
                nf.deficit_profile(bad)

    def test_predicted_media_age_end(self):
        self.assertAlmostEqual(nf.predicted_media_age_end_s(4, 0.5), 16.0)
        self.assertAlmostEqual(nf.predicted_media_age_end_s(4, 0.8), 40.0)
        self.assertAlmostEqual(nf.predicted_media_age_end_s(1, 0.0), 5.0)
        self.assertIsNone(nf.predicted_media_age_end_s(4, 1.0))
        self.assertIsNone(nf.predicted_media_age_end_s(4, 1.5))

    def test_end_tolerance_derives_from_null_age_bound_and_apply_time(self):
        # Unknown starting age (up to the null-trial bound) shortens the run by bound/(1-f);
        # apply time and one poll interval widen it.
        self.assertAlmostEqual(nf.end_tolerance_s(500_000, 0.5, 0.25), 1.0 + 0.25 + nf.POLL_INTERVAL_S)


class QdiscShowTests(unittest.TestCase):
    def test_parse_strips_counters_and_whitespace_from_captured_output(self):
        expected = ['qdisc mq 0: root',
                    'qdisc fq_codel 0: parent :4 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64',
                    'qdisc fq_codel 0: parent :3 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64',
                    'qdisc fq_codel 0: parent :2 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64',
                    'qdisc fq_codel 0: parent :1 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64']
        self.assertEqual(nf.parse_qdisc_show(ENP5S0_STATS), expected)
        self.assertEqual(nf.parse_qdisc_show(ENP5S0_PLAIN), expected)
        self.assertEqual(nf.parse_qdisc_show(''), [])

    def test_parse_matches_real_unprivileged_tc_on_this_machine(self):
        if shutil.which('tc') is None:
            self.skipTest('tc not installed')
        for interface in ('lo', 'enp5s0'):
            try:
                socket.if_nametoindex(interface)
            except OSError:
                continue
            plain = nf.parse_qdisc_show(tc_output('qdisc', 'show', 'dev', interface))
            stats = nf.parse_qdisc_show(tc_output('-s', 'qdisc', 'show', 'dev', interface))
            self.assertTrue(plain, interface)
            self.assertEqual(plain, stats)
            self.assertTrue(all(line.startswith('qdisc ') and (' root' in line or ' parent ' in line) for line in plain))
            self.assertEqual(sum(' root' in line for line in plain), 1)
            self.assertTrue(nf.restore_matches(tc_output('qdisc', 'show', 'dev', interface),
                                               tc_output('-s', 'qdisc', 'show', 'dev', interface)))

    def test_restore_ignores_order_and_counters_but_not_kind_or_params(self):
        reordered = '\n'.join(reversed(ENP5S0_PLAIN.splitlines())) + '\n'
        self.assertTrue(nf.restore_matches(ENP5S0_STATS, reordered))
        self.assertFalse(nf.restore_matches(ENP5S0_PLAIN, ENP5S0_PLAIN.replace('mq 0: root', 'prio 1: root')))
        self.assertFalse(nf.restore_matches(ENP5S0_PLAIN, ENP5S0_PLAIN.replace('parent :4 limit 10240p', 'parent :4 limit 1024p')))
        self.assertFalse(nf.restore_matches(ENP5S0_PLAIN, ENP5S0_PLAIN + 'qdisc ingress ffff: parent ffff:fff1 ----------------\n'))
        self.assertFalse(nf.restore_matches(ENP5S0_PLAIN, ''))


class TrialSpecTests(unittest.TestCase):
    def test_all_trials_are_named_in_plan_order(self):
        self.assertEqual(nf.TRIALS, ['null-1', 'null-2', 'stall-0.5-1', 'stall-0.5-2', 'stall-1-1', 'stall-1-2',
                                     'stall-2.5-1', 'stall-2.5-2', 'stall-4-1', 'stall-4-2',
                                     'deficit-0.8-1', 'deficit-0.5-1', 'delayed-reader-1'])

    def test_trial_spec_parses_kind_listeners_and_profile(self):
        self.assertEqual(nf.trial_spec('null-2'), dict(name='null-2', kind='null', listeners=2, seconds=60.0, profile=None))
        stall = nf.trial_spec('stall-2.5-1')
        self.assertEqual(stall['kind'], 'stall')
        self.assertEqual(stall['listeners'], 1)
        self.assertEqual(stall['profile'], nf.stall_profile(2.5))
        self.assertEqual(stall['seconds'], 20 + 2.5 + 20)
        deficit = nf.trial_spec('deficit-0.8-1')
        self.assertEqual(deficit['profile'], nf.deficit_profile(0.8))
        self.assertEqual(deficit['seconds'], 60.0)
        delayed = nf.trial_spec('delayed-reader-1')
        self.assertEqual((delayed['kind'], delayed['listeners'], delayed['profile']), ('delayed-reader', 1, None))
        for bad in ('stall-4-3', 'null', 'stall-x-1', 'deficit-1.2-1', 'burst-1'):
            with self.assertRaises(ValueError):
                nf.trial_spec(bad)

    def test_trial_names_option_keeps_plan_order_and_rejects_unknown(self):
        self.assertEqual(nf.trial_names('stall-4-1,null-1'), ['null-1', 'stall-4-1'])
        self.assertEqual(nf.trial_names('all'), nf.TRIALS)
        with self.assertRaises(ValueError):
            nf.trial_names('null-1,nope')
        with self.assertRaises(ValueError):
            nf.trial_names('null-1,null-1')


BEFORE = dict(internal_largest=100_000, psram_largest=2_000_000)
TOLERANCES = dict(age_bound_us=600_000, largest_tolerance_bytes=dict(internal_largest=4_000, psram_largest=16_000))


class TrialSetupTests(unittest.TestCase):
    def test_setup_supplies_every_key_the_verdict_reads(self):
        read = set(re.findall(r"trial(?:\[|\.get\()'(\w+)'", inspect.getsource(nf.trial_verdict)))
        self.assertTrue(read)
        self.assertLessEqual(read, set(nf.VERDICT_TRIAL_KEYS))
        for name in nf.TRIALS:
            for installed in (None, dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.25)):
                for tolerances in (None, TOLERANCES):
                    trial = nf.trial_setup(nf.trial_spec(name), BEFORE, tolerances, installed)
                    self.assertLessEqual(set(nf.VERDICT_TRIAL_KEYS), set(trial), name)

    def test_setup_predicts_from_the_unbuffered_bound_and_measured_tolerances(self):
        installed = dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.25)
        trial = nf.trial_setup(nf.trial_spec('deficit-0.5-1'), BEFORE, TOLERANCES, installed)
        # Plain /stream.mp3 is Route::Stream with no preset: MediaAge::for_stream(None, Mp3) bounds at 4 s.
        self.assertEqual(trial['media_age_bound_s'], 4.0)
        self.assertNotIn('preset', trial)
        self.assertAlmostEqual(trial['predicted_end_s'], 8.0)
        self.assertAlmostEqual(trial['end_tolerance_s'], nf.end_tolerance_s(600_000, 0.5, 0.25))
        self.assertEqual(trial['expected_frames'], 48000 * 60)
        self.assertEqual(trial['largest_before'], BEFORE)
        self.assertEqual(trial['largest_tolerance_bytes'], TOLERANCES['largest_tolerance_bytes'])
        self.assertIsNone(nf.trial_setup(nf.trial_spec('deficit-0.5-1'), BEFORE, TOLERANCES, None)['end_tolerance_s'])
        self.assertIsNone(nf.trial_setup(nf.trial_spec('deficit-0.5-1'), BEFORE, None, installed)['end_tolerance_s'])
        self.assertIsNone(nf.trial_setup(nf.trial_spec('stall-1-1'), BEFORE, None, None)['age_bound_us'])


class VerdictTests(unittest.TestCase):
    def null_trial(self, listeners=1):
        return nf.trial_setup(nf.trial_spec(f'null-{listeners}'), BEFORE, None, None)

    def stall_trial(self, seconds=4, listeners=1):
        return nf.trial_setup(nf.trial_spec(f'stall-{seconds:g}-{listeners}'), BEFORE, TOLERANCES, None)

    def deficit_trial(self, fraction=0.5):
        installed = dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.25)
        return dict(nf.trial_setup(nf.trial_spec(f'deficit-{fraction:g}-1'), BEFORE, TOLERANCES, installed),
                    predicted_end_s=16.0, end_tolerance_s=2.0)

    def test_null_requires_exact_frames_no_departures_and_flat_counters(self):
        trial = self.null_trial(2)
        good = [metrics(0), decoder('listener-1', 48000 * 60, 48000 * 60, ended_s=60.2), decoder('listener-2', 48000 * 60, 48000 * 60, ended_s=60.3),
                reset_after('p1', 60.2), reset_after('p2', 60.3), metrics(61)]
        verdict = nf.trial_verdict(trial, good)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['harness_closes'], 2)
        # A reset nowhere near a harness close is a product departure.
        self.assertFalse(nf.trial_verdict(trial, good + [departure('p3', RESET, 50_000_000, 30)])['passed'])
        self.assertFalse(nf.trial_verdict(trial, good + [reset_after('p3', 60.2, delay=nf.HARNESS_CLOSE_WINDOW_S + 0.5)])['passed'])
        self.assertFalse(nf.trial_verdict(trial, good[:2] + [metrics(60)])['passed'])
        self.assertFalse(nf.trial_verdict(trial, good + [departure('p', 'stopped reading', 1, 30)])['passed'])
        short = [metrics(0), decoder('listener-1', 48000 * 59, 48000 * 60), decoder('listener-2', 48000 * 60, 48000 * 60), metrics(60)]
        self.assertFalse(nf.trial_verdict(trial, short)['passed'])
        gap = [metrics(0), decoder('listener-1', 48000 * 60, 48000 * 60, discontinuities=1), decoder('listener-2', 48000 * 60, 48000 * 60), metrics(60)]
        self.assertFalse(nf.trial_verdict(trial, gap)['passed'])
        counters = [metrics(0), decoder('listener-1', 48000 * 60, 48000 * 60), decoder('listener-2', 48000 * 60, 48000 * 60), metrics(60, overflows=1)]
        self.assertFalse(nf.trial_verdict(trial, counters)['passed'])
        self.assertFalse(nf.trial_verdict(trial, [decoder('listener-1', 48000 * 60, 48000 * 60), decoder('listener-2', 48000 * 60, 48000 * 60)])['passed'])

    def test_stall_passes_on_continuous_decode_with_recovered_age(self):
        trial = self.stall_trial(1)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 21.1), metrics(21, age=1_200_000),
                  metrics(40, age=300_000), decoder('listener-1', expected, expected, ended_s=41.2), reset_after('p1', 41.2)]
        self.assertTrue(nf.trial_verdict(trial, events)['passed'])
        stuck = events[:-3] + [metrics(40, age=1_200_000), decoder('listener-1', expected, expected, ended_s=41.2), reset_after('p1', 41.2)]
        verdict = nf.trial_verdict(trial, stuck)
        self.assertFalse(verdict['passed'])
        self.assertIn('age', verdict['reason'])
        self.assertFalse(nf.trial_verdict(trial, events + [departure('p', 'media age exceeded', 5, 25)])['passed'])
        self.assertFalse(nf.trial_verdict(dict(trial, age_bound_us=None), events)['passed'])

    def test_stall_passes_on_media_age_departure_with_checked_close_and_fresh_admission(self):
        trial = self.stall_trial(4)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 24.1),
                  departure('p1', 'media age exceeded', 50_000_000, 23.9), close('p1', 50_010_000, 23.95),
                  admission('p2', 58_000_000, 31), decoder('listener-1', expected // 2, expected, ended_s=24.5), metrics(50),
                  fresh('listener-1-fresh', 41.3), reset_after('p2', 41.3)]
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['harness_closes'], 1)
        late = [event for event in events if event['kind'] != 'admission'] + [admission('p2', 60_010_001, 34)]
        self.assertFalse(nf.trial_verdict(trial, late)['passed'])
        self.assertFalse(nf.trial_verdict(trial, [event for event in events if event['kind'] != 'admission'])['passed'])
        self.assertFalse(nf.trial_verdict(trial, [event for event in events if event['kind'] != 'close'])['passed'])
        wrong_end = [dict(event, end='stopped reading') if event['kind'] == 'departure' else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, wrong_end)['passed'])
        # A listener that stopped decoding without any departure is neither outcome.
        self.assertFalse(nf.trial_verdict(trial, [metrics(0), decoder('listener-1', expected // 2, expected), metrics(50)])['passed'])

    def test_stall_with_two_listeners_judges_each(self):
        trial = self.stall_trial(2.5, 2)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 22.6),
                  departure('p1', 'media age exceeded', 50_000_000, 22.5), close('p1', 50_010_000, 22.6),
                  admission('p3', 55_000_000, 28), decoder('listener-1', expected // 2, expected, ended_s=23),
                  decoder('listener-2', expected, expected, ended_s=43.1), reset_after('p2', 43.1), metrics(50, age=400_000)]
        self.assertTrue(nf.trial_verdict(trial, events)['passed'])
        both_cut = [dict(event, frames=expected // 2) if event.get('label') == 'listener-2' else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, both_cut)['passed'])

    def test_deficit_requires_media_age_end_near_prediction_then_admission_and_heap_return(self):
        trial = self.deficit_trial(0.5)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), departure('p1', 'media age exceeded', 60_000_000, 36.5),
                  close('p1', 60_020_000, 36.6), injection('removed', 37), admission('p2', 62_000_000, 39),
                  decoder('listener-1', expected // 3, expected, ended_s=37.4), fresh('listener-1-fresh', 49.2), reset_after('p2', 49.2),
                  metrics(50, internal=99_000, psram=1_990_000)]
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertAlmostEqual(verdict['observed_end_s'], 16.5)
        early = [dict(event, at_s=30) if event['kind'] == 'departure' and event['end'] != RESET else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, early)['passed'])
        late_join = [dict(event, device_us=70_020_001) if event['kind'] == 'admission' else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, late_join)['passed'])
        self.assertFalse(nf.trial_verdict(dict(trial, end_tolerance_s=None), events)['passed'])
        heap = events[:-1] + [metrics(50, internal=90_000, psram=1_990_000)]
        self.assertFalse(nf.trial_verdict(trial, heap)['passed'])
        self.assertFalse(nf.trial_verdict(dict(trial, largest_tolerance_bytes=None), events)['passed'])
        self.assertFalse(nf.trial_verdict(trial, [metrics(0), injection('installed', 20), injection('removed', 80),
                                                  decoder('listener-1', expected, expected), metrics(90)])['passed'])

    def test_delayed_reader_requires_continuous_mp3_and_checked_wav_media_age_departure(self):
        trial = nf.trial_setup(nf.trial_spec('delayed-reader-1'), BEFORE, None, None)
        expected = trial['expected_frames']
        # The starved WAV reader ends on media age (stream.rs:729/757), not the frame write timeout.
        events = [metrics(0), departure('wav', 'media age exceeded', 30_000_000, 12), close('wav', 30_050_000, 12.1),
                  decoder('listener-1', expected, expected, ended_s=40.2), reset_after('p1', 40.2), wav(40.3), metrics(41)]
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['wav_end_category'], 'media_age')
        self.assertFalse(nf.trial_verdict(trial, [event for event in events if event['kind'] != 'close'])['passed'])
        self.assertFalse(nf.trial_verdict(trial, [event for event in events if event['kind'] != 'departure'])['passed'])
        timeout = [dict(event, end='stopped reading') if event.get('end') == 'media age exceeded' else event for event in events]
        verdict = nf.trial_verdict(trial, timeout)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['wav_end_category'], 'frame_timeout')
        cut = [dict(event, frames=expected // 2, ended='cut') if event['kind'] == 'decoder' else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, cut)['passed'])
        # A WAV reader the harness had to close itself is a reset after wav closed_s, not a product departure.
        never_departed = [metrics(0), decoder('listener-1', expected, expected, ended_s=40.2), reset_after('p1', 40.2), wav(40.3),
                          reset_after('wav', 40.3), metrics(41)]
        verdict = nf.trial_verdict(trial, never_departed)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['harness_closes'], 2)

    def test_uart_events_use_production_lines(self):
        lines = ['I (120) esp_shazam_firmware::stream: stream: 192.168.23.153:40000 joined /stream.mp3 (1 of 2) event_us=100 owner_id=0 sonos_intent=0 role=diagnostic',
                 'I (130) esp_shazam_firmware::stream: stream: 192.168.23.153:40000 left after 12.3 s: media age exceeded listener_stack_free_bytes=512 event_us=12400000',
                 'I (131) esp_shazam_firmware::stream: stream: 192.168.23.153:40000 socket closed in 3 ms; releasing slot (0 held) event_us=12403000 owner_id=0 sonos_intent=0',
                 'listener-telemetry: dropped=0 loss_occurred=false observed_event_range_valid=true observed_event_min_us=100 observed_event_max_us=12403000 record_bytes=248 queue_bytes=2016 metrics_stack_free_bytes=512 export_us=12500000']
        events = nf.uart_events(lines, at_s=[1.0, 13.0, 13.0, 13.1])
        self.assertEqual(events, [admission('192.168.23.153:40000', 100, 1.0),
                                  departure('192.168.23.153:40000', 'media age exceeded', 12_400_000, 13.0),
                                  close('192.168.23.153:40000', 12_403_000, 13.0)])
        self.assertEqual(nf.uart_events(lines[:3], at_s=[1.0, 13.0, 13.0])[-1]['kind'], 'departure')


class DriverHelperTests(unittest.TestCase):
    def snapshot(self, ages=(500_000, None), states=('writing', 'idle'), largest=(100_000, 2_000_000), overflows=0):
        listeners = [dict(generation=1 if state != 'idle' else 0, state=state, sample_us=10, media_age_us=age, write_us=0 if age is not None else None)
                     for age, state in zip(ages, states)]
        capture = dict.fromkeys(nf.LOSS_KEYS, 0)
        capture.update(queue_overflows=overflows, read_frames=480_000, internal_largest=largest[0], psram_largest=largest[1])
        return dict(schema_version=1, served_device_us=20, boot_token='a' * 32, build_elf_sha256='b' * 64,
                    settings=dict(active_format='mp3', next_play_format='mp3', next_play_buffer='med', buffer_available=True,
                                  autoplay=False, recognition_enabled=False, pga_gain=0),
                    network=dict(generation=3, connected=True, disconnects=0), listeners=listeners, capture=capture)

    def test_metrics_sample_keeps_writing_ages_largest_and_loss_counters(self):
        # Ready carries the age recorded after every successful write (listener_observation.rs:185).
        sample = nf.metrics_sample(self.snapshot(ages=(500_000, 900_000), states=('writing', 'ready')), 12.5)
        self.assertEqual(sample['kind'], 'metrics')
        self.assertEqual((sample['at_s'], sample['media_age_us'], sample['internal_largest'], sample['psram_largest']),
                         (12.5, 900_000, 100_000, 2_000_000))
        self.assertEqual(sample['capture'], dict.fromkeys(nf.LOSS_KEYS, 0))
        self.assertEqual(sample['read_frames'], 480_000)
        self.assertEqual(sample['settings']['next_play_buffer'], 'med')
        idle = nf.metrics_sample(self.snapshot(ages=(None, None), states=('idle', 'idle')), 1)
        self.assertIsNone(idle['media_age_us'])
        bare = self.snapshot()
        bare['capture'] = None
        self.assertEqual((nf.metrics_sample(bare, 1)['capture'], nf.metrics_sample(bare, 1)['internal_largest']), (None, None))

    def test_settings_gate_requires_autoplay_off_and_mp3(self):
        settings = self.snapshot()['settings']
        self.assertIsNone(nf.settings_problem(settings))
        self.assertIn('autoplay', nf.settings_problem(dict(settings, autoplay=True)))
        self.assertIn('flac', nf.settings_problem(dict(settings, active_format='flac')))
        self.assertIn('unavailable', nf.settings_problem(None))

    def test_null_tolerances_use_steady_ages_and_whole_trial_largest_swing(self):
        samples = [metrics(0, age=4_000_000, internal=100_000, psram=2_000_000), metrics(10, age=900_000, internal=96_000),
                   metrics(25, age=300_000, internal=99_000, psram=1_995_000), metrics(50, age=450_000, internal=101_000)]
        self.assertEqual(nf.null_tolerances(samples, steady_after_s=20),
                         dict(age_bound_us=450_000, largest_tolerance_bytes=dict(internal_largest=5_000, psram_largest=5_000)))
        self.assertIsNone(nf.null_tolerances(samples[:2], steady_after_s=20))
        merged = nf.merge_tolerances(dict(age_bound_us=400_000, largest_tolerance_bytes=dict(internal_largest=8_000, psram_largest=1_000)),
                                     nf.null_tolerances(samples, steady_after_s=20))
        self.assertEqual(merged, dict(age_bound_us=450_000, largest_tolerance_bytes=dict(internal_largest=8_000, psram_largest=5_000)))
        self.assertEqual(nf.merge_tolerances(None, None), None)

    def test_injection_commands_pair_install_with_removal(self):
        install, remove = nf.injection_commands(nf.stall_profile(1), 'enp5s0', 'ifb0')
        self.assertEqual(install, [nf.netem_change('ifb0', 'root', loss_percent=100), nf.netem_change('enp5s0', '1:4', loss_percent=100)])
        self.assertEqual(remove, [nf.netem_change('ifb0', 'root', loss_percent=0), nf.netem_change('enp5s0', '1:4', loss_percent=0)])
        install, remove = nf.injection_commands(nf.deficit_profile(0.8), 'enp5s0', 'ifb0')
        self.assertEqual(install, [nf.netem_change('ifb0', 'root', rate_bit_per_s=256_000)])
        self.assertEqual(remove, [nf.netem_change('ifb0', 'root', rate_bit_per_s=0)])
        self.assertEqual(nf.injection_commands(None, 'enp5s0', 'ifb0'), ([], []))
        # Commands follow the profile's own fields, so an egress-only profile touches only the egress netem.
        egress_only = dict(kind='stall', seconds=1, egress=dict(loss_percent=100), ingress=None)
        self.assertEqual(nf.injection_commands(egress_only, 'enp5s0', 'ifb0'),
                         ([nf.netem_change('enp5s0', '1:4', loss_percent=100)], [nf.netem_change('enp5s0', '1:4', loss_percent=0)]))

    def test_budget_covers_every_trial_plus_startup_and_cleanup(self):
        single = nf.budget_seconds(['null-1'])
        self.assertGreater(single, 60)
        self.assertGreater(nf.budget_seconds(['null-1', 'stall-4-1']), single + 44)

    def test_bind_to_device_pins_a_fresh_socket(self):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            nf.bind_to_device(sock, 'lo')
            self.assertEqual(sock.getsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, 256), b'lo\0')
            with self.assertRaises(OSError):
                nf.bind_to_device(sock, 'no-such-interface-0')


class CommandLineTests(unittest.TestCase):
    def test_root_refusal(self):
        stderr = io.StringIO()
        with self.assertRaises(SystemExit) as raised, contextlib.redirect_stderr(stderr):
            nf.require_unprivileged(0)
        self.assertEqual(raised.exception.code, 2)
        self.assertIn('sudo -v', stderr.getvalue())
        nf.require_unprivileged(1000)

    def test_route_interface_reads_dev_from_ip_route_get(self):
        text = '192.168.23.217 dev enp5s0 src 192.168.23.153 uid 1000 \n    cache \n'
        self.assertEqual(nf.route_interface(text), 'enp5s0')
        self.assertEqual(nf.route_interface('192.168.23.217 via 192.168.23.1 dev wlp6s0 src 192.168.23.72 uid 1000\n'), 'wlp6s0')
        with self.assertRaises(ValueError):
            nf.route_interface('RTNETLINK answers: Network is unreachable\n')

    def test_dry_run_prints_every_privileged_argv_and_creates_nothing(self):
        wrapper = SCRIPTS / 'network-fault'
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'result-dir'
            result = subprocess.run([str(wrapper), '--dry-run', BOARD, '--interface', 'lo', '--serial', '/dev/null',
                                     '--out', str(out), '--trials', 'null-1,stall-4-1,deficit-0.5-1'],
                                    cwd=directory, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])
            lines = result.stdout.splitlines()
            self.assertIn('sudo -n tc qdisc replace dev lo root handle 1: prio bands 4', lines)
            self.assertIn('sudo -n tc qdisc change dev ifb0 root netem loss 100%', lines)
            self.assertIn('sudo -n tc qdisc change dev lo parent 1:4 netem loss 100%', lines)
            self.assertIn('sudo -n tc qdisc change dev ifb0 root netem rate 160000bit', lines)
            self.assertIn('sudo -n tc qdisc change dev ifb0 root netem rate 0bit', lines)
            self.assertEqual(lines[-1], 'sudo -n ip link del ifb0')
            self.assertTrue(all(line.startswith('sudo -n ') or line.startswith('# ') for line in lines), lines)

    def test_help_and_bad_arguments_create_nothing(self):
        wrapper = SCRIPTS / 'network-fault'
        with tempfile.TemporaryDirectory() as directory:
            existing = Path(directory) / 'taken'
            existing.mkdir()
            cases = [(['--help'], 0),
                     (['--unrecognized-option'], 2),
                     (['esp-shazam.local', '--interface', 'lo', '--serial', '/dev/null', '--out', 'x'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', 'x', '--trials', 'nope'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', str(existing), '--dry-run'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null'], 2)]
            for args, expected in cases:
                result = subprocess.run([str(wrapper), *args], cwd=directory, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, expected, (args, result.stdout, result.stderr))
                self.assertEqual(list(Path(directory).iterdir()), [existing], args)

    def test_import_is_inert(self):
        code = """
import sys
sys.path.insert(0, sys.argv[1])
def audit(event, args):
    if event.startswith('socket.') or event.startswith('subprocess.') or event in ('os.mkdir', 'os.system'):
        raise RuntimeError('import side effect: ' + event)
sys.addaudithook(audit)
import network_fault
"""
        result = subprocess.run([sys.executable, '-B', '-c', code, str(SCRIPTS)], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
