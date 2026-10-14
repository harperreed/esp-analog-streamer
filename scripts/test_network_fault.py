# ABOUTME: Checks the network-fault harness's qdisc planning, qdisc-show parsing and trial verdicts on the host.
# ABOUTME: Runs tc unprivileged for real usage and show output; never installs a qdisc or contacts a board.
import argparse
import contextlib
import hashlib
import inspect
import io
import json
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


def fresh(label, ended_s, ended='completed', trigger='uart_close', close_device_us=None):
    return dict(kind='fresh', label=label, ended=ended, ended_s=ended_s, trigger=trigger, close_device_us=close_device_us)


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


def injection(phase, at_s, apply_seconds=0.0):
    return dict(kind='injection', phase=phase, at_s=at_s, apply_seconds=apply_seconds)


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
        events = [metrics(0), injection('installed', 20), injection('removed', 21.1), metrics(21.5, age=1_200_000),
                  metrics(40, age=300_000), decoder('listener-1', expected, expected, ended_s=41.2), reset_after('p1', 41.2)]
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual((verdict['age_peak_after_removal_us'], verdict['age_last_active_us']), (1_200_000, 300_000))
        stuck = events[:-3] + [metrics(40, age=1_200_000), decoder('listener-1', expected, expected, ended_s=41.2), reset_after('p1', 41.2)]
        verdict = nf.trial_verdict(trial, stuck)
        self.assertFalse(verdict['passed'])
        self.assertIn('age', verdict['reason'])
        self.assertEqual(verdict['age_last_active_us'], 1_200_000)
        # The post-trial idle sample has no age and must not be the one judged.
        idle_tail = events + [metrics(45, age=None)]
        self.assertTrue(nf.trial_verdict(trial, idle_tail)['passed'])
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
        late = [event for event in events if event['kind'] != 'admission'] + [admission('p2', 60_000_001, 34)]
        self.assertFalse(nf.trial_verdict(trial, late)['passed'])
        # With a free second slot the fresh join can land before the old close completes.
        before_close = [event for event in events if event['kind'] != 'admission'] + [admission('p2', 50_005_000, 24.0)]
        self.assertTrue(nf.trial_verdict(trial, before_close)['passed'])
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
                  decoder('listener-2', expected, expected, ended_s=43.1), reset_after('p2', 43.1), metrics(40, age=400_000), metrics(50, age=None)]
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
        # The prediction is an upper bound (TCP through netem rate gets at most the nominal rate): early is fine, late fails.
        early = [dict(event, at_s=30) if event['kind'] == 'departure' and event['end'] != RESET else event for event in events]
        self.assertTrue(nf.trial_verdict(trial, early)['passed'])
        self.assertAlmostEqual(nf.trial_verdict(trial, early)['observed_end_s'], 10.0)
        late = [dict(event, at_s=38.1) if event['kind'] == 'departure' and event['end'] != RESET else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, late)['passed'])
        late_join = [dict(event, device_us=70_000_001) if event['kind'] == 'admission' else event for event in events]
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
        # The reader's own frame write limit (ring capacity / 48 kHz) can fire before the 4 s age bound.
        timeout = [dict(event, end='stopped reading') if event.get('end') == 'media age exceeded' else event for event in events]
        verdict = nf.trial_verdict(trial, timeout)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['wav_end_category'], 'frame_timeout')
        broken = [dict(event, end=RESET) if event.get('end') == 'media age exceeded' else event for event in events]
        verdict = nf.trial_verdict(trial, broken)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['wav_end_category'], 'socket_error')
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

    def test_apply_records_the_injection_event_it_returns(self):
        # The first device run died here: the event dict already names its kind (oct14-interruption result.json).
        with tempfile.TemporaryDirectory() as out:
            args = argparse.Namespace(host='192.0.2.1', interface='lo', ifb='ifb0', trials=['null-1'], out=Path(out))
            driver = nf.Driver(args, Path(out))
            with (Path(out) / 'events.jsonl').open('w') as driver.events, contextlib.redirect_stdout(io.StringIO()):
                event = driver.apply('installed', [])
            recorded = [json.loads(line) for line in (Path(out) / 'events.jsonl').read_text().splitlines()]
        self.assertEqual(event['kind'], 'injection')
        self.assertEqual(event['phase'], 'installed')
        self.assertEqual(len(recorded), 1)
        self.assertEqual({key: recorded[0][key] for key in ('kind', 'phase', 'at_s', 'apply_seconds')}, event)

    def test_bind_to_device_pins_a_fresh_socket(self):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            nf.bind_to_device(sock, 'lo')
            self.assertEqual(sock.getsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, 256), b'lo\0')
            with self.assertRaises(OSError):
                nf.bind_to_device(sock, 'no-such-interface-0')



# Records lifted from target/kata-hardening/oct14-interruption-4 and -3 (result.json trials[*].events, events.jsonl
# metrics lines). Device microseconds are exact; metrics at_s is rounded to the millisecond; the absolute path in the
# dead fresh listener's `ended` text was stripped. Everything else is the retained value.
RUN4_BEFORE = dict(internal_largest=47104, psram_largest=2359296)
RUN4_TOLERANCES = dict(age_bound_us=142463, largest_tolerance_bytes=dict(internal_largest=6144, psram_largest=0))
RUN3_TOLERANCES = dict(age_bound_us=2126239, largest_tolerance_bytes=dict(internal_largest=0, psram_largest=0))
CAPTURE_FLAT = dict(queue_overflows=8, software_dropped_frames=0, read_errors=0, restarts=0, discontinuities=0)


def sample(at_s, age, internal=47104, psram=2359296):
    return dict(kind='metrics', at_s=at_s, media_age_us=age, internal_largest=internal, psram_largest=psram, capture=dict(CAPTURE_FLAT))


RUN4_STALL_4_1_EVENTS = [
    dict(kind='decoder', label='stall-4-1-listener-1', frames=2112000, expected_frames=2112000, discontinuities=0, exit_code=0, ended='completed', ended_s=480.91661526600365),
    dict(kind='injection', phase='installed', at_s=456.3355433960096, apply_seconds=0.024),
    dict(kind='injection', phase='removed', at_s=460.35991857398767, apply_seconds=0.025),
    dict(kind='admission', peer='192.168.23.153:57306', device_us=434991720, at_s=436.81212376098847),
    dict(kind='departure', peer='192.168.23.153:57306', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=479201063, at_s=481.11707020099857),
    dict(kind='close', peer='192.168.23.153:57306', device_us=479201621, at_s=481.11707428700174),
]
RUN4_STALL_4_1_SAMPLES = [sample(at_s, age) for at_s, age in [
    (461.311, 1550664), (462.334, 48341), (463.298, 46325), (464.325, 52468), (465.313, 52272), (466.309, 51374),
    (467.298, 52416), (468.322, 53058), (469.382, 51726), (470.326, 65673), (471.313, 49074), (472.307, 50727),
    (473.365, 49803), (474.318, 49377), (475.312, 45170), (476.303, 44693), (477.493, 46683), (478.318, 51691),
    (479.300, 52266), (480.304, 56819), (481.318, None), (482.342, None), (483.322, None), (484.315, None), (485.311, None)]]
RUN4_DEFICIT_0_5_1_EVENTS = [
    dict(kind='decoder', label='deficit-0.5-1-listener-1', frames=1172736, expected_frames=2880000, discontinuities=0, exit_code=251,
         ended='ffmpeg integrity failure: [http @ 0x5ee337ec4940] [error] Stream ends prematurely at 976800, should be 18446744073709551615', ended_s=620.1674808120006),
    dict(kind='fresh', label='deficit-0.5-1-fresh-1', ended='completed', ended_s=629.6810319210053, trigger='uart_departure'),
    dict(kind='injection', phase='installed', at_s=610.1240752309968, apply_seconds=0.015),
    dict(kind='injection', phase='removed', at_s=619.1498638300109, apply_seconds=0.015),
    dict(kind='admission', peer='192.168.23.153:51312', device_us=588857019, at_s=590.599611475016),
    dict(kind='departure', peer='192.168.23.153:51312', end='media age exceeded', device_us=617292409, at_s=619.1495294139895),
    dict(kind='admission', peer='192.168.23.153:46918', device_us=617845229, at_s=619.6657872840005),
    dict(kind='departure', peer='192.168.23.153:46918', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=628044204, at_s=629.8814693519962),
    dict(kind='close', peer='192.168.23.153:51312', device_us=618594011, at_s=620.6680814299907),
    dict(kind='close', peer='192.168.23.153:46918', device_us=628045092, at_s=630.0818047430075),
    sample(590.383, 48272), sample(634.368, None),
]
RUN3_STALL_2_5_1_EVENTS = [
    dict(kind='decoder', label='stall-2.5-1-listener-1', frames=1001088, expected_frames=2040000, discontinuities=0, exit_code=251,
         ended='ffmpeg integrity failure: [http @ 0x5a4b73912940] [error] Stream ends prematurely at 834240, should be 18446744073709551615', ended_s=374.2971532989759),
    dict(kind='fresh', label='stall-2.5-1-fresh-1', ended='completed', ended_s=384.3069482279825, trigger='uart_departure'),
    dict(kind='injection', phase='installed', at_s=367.7444286760001, apply_seconds=0.025),
    dict(kind='injection', phase='removed', at_s=370.26967800798593, apply_seconds=0.023),
    dict(kind='admission', peer='192.168.23.153:32884', device_us=346593932, at_s=348.71613394198357),
    dict(kind='departure', peer='192.168.23.153:32884', end='media age exceeded', device_us=371457022, at_s=373.79498827198404),
    dict(kind='admission', peer='192.168.23.153:46934', device_us=372701309, at_s=374.2962549259828),
    dict(kind='departure', peer='192.168.23.153:46934', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=382858298, at_s=384.70786008698633),
    dict(kind='close', peer='192.168.23.153:32884', device_us=372771338, at_s=374.7978945199866),
    dict(kind='close', peer='192.168.23.153:46934', device_us=382859103, at_s=384.70786355499877),
    sample(348.283, 52369),
]
RUN4_DELAYED_READER_EVENTS = [
    dict(kind='decoder', label='delayed-reader-1-listener-1', frames=1920000, expected_frames=1920000, discontinuities=0, exit_code=0, ended='completed', ended_s=675.3190738809935),
    dict(kind='wav', closed_s=680.3251240790123),
    dict(kind='admission', peer='192.168.23.153:58452', device_us=633444784, at_s=635.2762716129946),
    dict(kind='admission', peer='192.168.23.153:58458', device_us=633450656, at_s=635.7771773610148),
    dict(kind='departure', peer='192.168.23.153:58452', end='stopped reading', device_us=635842309, at_s=637.7789524270047),
    dict(kind='departure', peer='192.168.23.153:58458', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=673621474, at_s=675.7195508790028),
    dict(kind='close', peer='192.168.23.153:58452', device_us=638053895, at_s=639.7809107690118),
    dict(kind='close', peer='192.168.23.153:58458', device_us=673622028, at_s=675.7195528470038),
    sample(635.362, 285922), sample(680.366, None, internal=45056),
]
RUN4_DEFICIT_0_8_1_EVENTS = [
    dict(kind='decoder', label='deficit-0.8-1-listener-1', frames=1616256, expected_frames=2880000, discontinuities=0, exit_code=251,
         ended='ffmpeg integrity failure: [http @ 0x628bad6c3940] [error] Stream ends prematurely at 1346400, should be 18446744073709551615', ended_s=574.8788830449921),
    dict(kind='fresh', label='deficit-0.8-1-fresh-1', ended='completed', ended_s=584.8932915370096, trigger='uart_departure'),
    dict(kind='injection', phase='installed', at_s=555.8197047589929, apply_seconds=0.017),
    dict(kind='injection', phase='removed', at_s=574.3626188370108, apply_seconds=0.015),
    dict(kind='admission', peer='192.168.23.153:58624', device_us=534448276, at_s=536.2947610559931),
    dict(kind='departure', peer='192.168.23.153:58624', end='media age exceeded', device_us=572099472, at_s=574.3622049530095),
    dict(kind='admission', peer='192.168.23.153:38330', device_us=573085267, at_s=574.8787465060013),
    dict(kind='departure', peer='192.168.23.153:38330', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=583259321, at_s=585.0938199550146),
    dict(kind='close', peer='192.168.23.153:58624', device_us=573080131, at_s=574.8787456529972),
    dict(kind='close', peer='192.168.23.153:38330', device_us=583259872, at_s=585.0938234429923),
    sample(536.313, 45654), sample(589.372, None),
]
RUN3_DEFICIT_0_8_1_EVENTS = [
    dict(kind='decoder', label='deficit-0.8-1-listener-1', frames=1203840, expected_frames=2880000, discontinuities=0, exit_code=251,
         ended='ffmpeg integrity failure: [http @ 0x59b175925940] [error] Stream ends prematurely at 1002720, should be 18446744073709551615', ended_s=564.4247309589991),
    dict(kind='fresh', label='deficit-0.8-1-fresh-1', ended='completed', ended_s=574.9404425660032, trigger='uart_departure'),
    dict(kind='injection', phase='installed', at_s=553.884791421995, apply_seconds=0.012),
    dict(kind='injection', phase='removed', at_s=563.9086437939841, apply_seconds=0.015),
    dict(kind='admission', peer='192.168.23.153:44732', device_us=532669472, at_s=534.8600374959933),
    dict(kind='departure', peer='192.168.23.153:44732', end='media age exceeded', device_us=561705572, at_s=563.908253962989),
    dict(kind='admission', peer='192.168.23.153:48244', device_us=563193117, at_s=564.925657298998),
    dict(kind='departure', peer='192.168.23.153:48244', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=573748660, at_s=575.7419536459784),
    dict(kind='close', peer='192.168.23.153:44732', device_us=563041352, at_s=564.9256376799894),
    dict(kind='close', peer='192.168.23.153:48244', device_us=573749201, at_s=575.7419576669927),
    sample(534.506, 52185), sample(579.722, None),
]
RUN3_STALL_4_2_EVENTS = [
    dict(kind='decoder', label='stall-4-2-listener-1', frames=881280, expected_frames=2112000, discontinuities=0, exit_code=152,
         ended='ffmpeg integrity failure: [in#0/mp3 @ 0x5919e4438240] [error] Error during demuxing: Connection reset by peer', ended_s=509.93477874898235),
    dict(kind='decoder', label='stall-4-2-listener-2', frames=2112000, expected_frames=2112000, discontinuities=0, exit_code=0, ended='completed', ended_s=528.4625634789991),
    dict(kind='fresh', label='stall-4-2-fresh-1', ended='no decoded framehash output: stall-4-2-fresh-1.framehash', ended_s=509.43404839298455, trigger='uart_departure'),
    dict(kind='injection', phase='installed', at_s=502.8136451919854, apply_seconds=0.045),
    dict(kind='injection', phase='removed', at_s=506.8591269379831, apply_seconds=0.066),
    dict(kind='admission', peer='192.168.23.153:33740', device_us=482651424, at_s=484.28546230398933),
    dict(kind='admission', peer='192.168.23.153:33738', device_us=482656883, at_s=484.2854656589916),
    dict(kind='departure', peer='192.168.23.153:33738', end='media age exceeded', device_us=506161813, at_s=508.92976776100113),
    dict(kind='departure', peer='192.168.23.153:33740', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=527501585, at_s=529.4641988629883),
    dict(kind='close', peer='192.168.23.153:33738', device_us=508205727, at_s=510.4357955659798),
    dict(kind='close', peer='192.168.23.153:33740', device_us=527502147, at_s=529.4642027959926),
] + [sample(at_s, age) for at_s, age in [
    (507.427, 3724336), (508.408, 1898782), (509.412, 343188), (510.875, 50957), (511.551, 46279), (512.592, 52231),
    (513.651, 52283), (515.088, 146064), (515.595, 737389), (516.654, 1759588), (517.389, 2743485), (518.805, 2169322),
    (519.551, 55388), (520.575, 48416), (521.501, 51700), (522.929, 47205), (524.106, 47600), (525.003, 48999),
    (525.471, 48053), (526.334, 51300), (527.543, 48854), (528.424, 47551), (529.71, None), (530.781, None)]]


def installed_of(events):
    return next(event for event in events if event['kind'] == 'injection' and event['phase'] == 'installed')


class RetainedRunTests(unittest.TestCase):
    def test_run4_stall_4_1_survives_judged_on_last_active_age(self):
        trial = nf.trial_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_STALL_4_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN4_STALL_4_1_EVENTS + RUN4_STALL_4_1_SAMPLES)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['reason'], 'continuous')
        self.assertEqual((verdict['age_peak_after_removal_us'], verdict['age_last_active_us']), (1550664, 56819))
        stuck = [dict(item, media_age_us=1550664) if item['media_age_us'] is not None else item for item in RUN4_STALL_4_1_SAMPLES]
        verdict = nf.trial_verdict(trial, RUN4_STALL_4_1_EVENTS + stuck)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['age_last_active_us'], 1550664)

    def test_fresh_join_before_the_old_close_counts_from_the_departure(self):
        trial = nf.trial_setup(nf.trial_spec('deficit-0.5-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_DEFICIT_0_5_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN4_DEFICIT_0_5_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertAlmostEqual(verdict['observed_end_s'], 9.025, places=3)
        trial = nf.trial_setup(nf.trial_spec('stall-2.5-1'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_2_5_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN3_STALL_2_5_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['departed'], 1)
        # The checked close is still required.
        self.assertFalse(nf.trial_verdict(trial, [event for event in RUN3_STALL_2_5_1_EVENTS if event['kind'] != 'close'])['passed'])

    def test_run4_delayed_reader_wav_ended_on_its_frame_write_limit(self):
        trial = nf.trial_setup(nf.trial_spec('delayed-reader-1'), RUN4_BEFORE, RUN4_TOLERANCES, None)
        verdict = nf.trial_verdict(trial, RUN4_DELAYED_READER_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['wav_end_category'], 'frame_timeout')
        self.assertEqual(verdict['harness_closes'], 1)
        broken = [dict(event, end=RESET) if event.get('end') == 'stopped reading' else event for event in RUN4_DELAYED_READER_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, broken)['passed'])

    def test_deficit_prediction_is_an_upper_bound(self):
        trial = nf.trial_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_DEFICIT_0_8_1_EVENTS))
        self.assertAlmostEqual(trial['predicted_end_s'], 20.0)
        self.assertAlmostEqual(trial['end_tolerance_s'], 1.729315)
        verdict = nf.trial_verdict(trial, RUN4_DEFICIT_0_8_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertAlmostEqual(verdict['observed_end_s'], 18.5425, places=3)
        trial3 = nf.trial_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_DEFICIT_0_8_1_EVENTS))
        verdict = nf.trial_verdict(trial3, RUN3_DEFICIT_0_8_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertAlmostEqual(verdict['observed_end_s'], 10.0235, places=3)
        late = [dict(event, at_s=555.8197047589929 + 25) if event.get('end') == 'media age exceeded' else event for event in RUN4_DEFICIT_0_8_1_EVENTS]
        verdict = nf.trial_verdict(trial, late)
        self.assertFalse(verdict['passed'])
        self.assertAlmostEqual(verdict['observed_end_s'], 25.0)


    def test_run3_stall_4_2_fails_on_the_missing_join_not_timing(self):
        # The fresh ffmpeg was started on the departure line while the survivor held the other slot and the
        # departed slot's close was 2.04 s away; no join line exists, so the records cannot pass.
        trial = nf.trial_setup(nf.trial_spec('stall-4-2'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_4_2_EVENTS))
        verdict = nf.trial_verdict(trial, RUN3_STALL_4_2_EVENTS)
        self.assertFalse(verdict['passed'])
        self.assertIn('no fresh join', verdict['reason'])
        self.assertIn('192.168.23.153:33738', verdict['reason'])
        self.assertNotIn('timing', verdict['reason'])
        self.assertEqual(verdict['age_last_active_us'], 47551)
        joined = RUN3_STALL_4_2_EVENTS + [dict(kind='admission', peer='192.168.23.153:40000', device_us=508_900_000, at_s=511.2)]
        self.assertTrue(nf.trial_verdict(trial, joined)['passed'])


    def test_two_departures_consume_two_distinct_joins(self):
        trial = nf.trial_setup(nf.trial_spec('stall-2.5-2'), RUN4_BEFORE, RUN3_TOLERANCES, dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.02))
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 22.6),
                  departure('p1', 'media age exceeded', 50_000_000, 22.5), close('p1', 52_000_000, 24.6),
                  departure('p2', 'media age exceeded', 50_100_000, 22.5), close('p2', 52_100_000, 24.6),
                  admission('p3', 53_000_000, 25.5), admission('p4', 53_200_000, 25.7),
                  decoder('listener-1', expected // 2, expected, ended_s=23), decoder('listener-2', expected // 2, expected, ended_s=23),
                  fresh('f1', 36, close_device_us=52_000_000), fresh('f2', 36.2, close_device_us=52_100_000),
                  reset_after('p3', 36), reset_after('p4', 36.2)]
        self.assertTrue(nf.trial_verdict(trial, events)['passed'], nf.trial_verdict(trial, events))
        one_join = [event for event in events if event.get('peer') != 'p4']
        verdict = nf.trial_verdict(trial, one_join)
        self.assertFalse(verdict['passed'])
        self.assertIn('p2', verdict['reason'])

    def test_deficit_end_cannot_precede_the_age_floor(self):
        trial = nf.trial_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_DEFICIT_0_8_1_EVENTS))
        installed_at = installed_of(RUN4_DEFICIT_0_8_1_EVENTS)['at_s']
        early = [dict(event, at_s=installed_at + 0.3) if event.get('end') == 'media age exceeded' else event for event in RUN4_DEFICIT_0_8_1_EVENTS]
        verdict = nf.trial_verdict(trial, early)
        self.assertFalse(verdict['passed'])
        self.assertIn('earlier', verdict['reason'])
        self.assertAlmostEqual(verdict['earliest_end_s'], 0.017 + 4.0 - 0.142463)
        self.assertTrue(nf.trial_verdict(trial, RUN4_DEFICIT_0_8_1_EVENTS)['passed'])

    def test_harness_close_logged_just_before_the_decoder_end_is_still_harness_owned(self):
        trial = nf.trial_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_STALL_4_1_EVENTS))
        ended_s = 480.91661526600365
        shifted = [dict(event, at_s=ended_s - 0.005) if event.get('end') == RESET else event for event in RUN4_STALL_4_1_EVENTS]
        verdict = nf.trial_verdict(trial, shifted + RUN4_STALL_4_1_SAMPLES)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['harness_closes'], 1)
        too_early = [dict(event, at_s=ended_s - nf.POLL_INTERVAL_S - 0.5) if event.get('end') == RESET else event for event in RUN4_STALL_4_1_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, too_early + RUN4_STALL_4_1_SAMPLES)['passed'])


class ReplacementTriggerTests(unittest.TestCase):
    PEER = '192.168.23.153:33738'
    OTHER = '192.168.23.153:33740'
    LEFT = f'I (508900) esp_shazam_firmware::stream: stream: {PEER} left after 23.5 s: media age exceeded listener_stack_free_bytes=1652 event_us=506161813'
    CLOSE = f'I (510400) esp_shazam_firmware::stream: stream: {PEER} socket closed in 2043 ms; releasing slot (1 held) event_us=508205727 owner_id=0 sonos_intent=0'
    OTHER_CLOSE = f'I (529400) esp_shazam_firmware::stream: stream: {OTHER} socket closed in 1 ms; releasing slot (0 held) event_us=527502147 owner_id=0 sonos_intent=0'

    def test_waits_for_the_departed_peers_close_line_only(self):
        self.assertIsNone(nf.replacement_trigger(self.PEER, [self.LEFT], waited_s=0.5))
        self.assertIsNone(nf.replacement_trigger(self.PEER, [self.LEFT, self.OTHER_CLOSE], waited_s=1.0))
        self.assertEqual(nf.replacement_trigger(self.PEER, [self.LEFT, self.CLOSE], waited_s=1.5), ('uart_close', 508205727))
        # Decoder EOF is the board shutting the socket; the slot frees at the close line, so EOF never triggers.
        self.assertIsNone(nf.replacement_trigger(self.PEER, [self.LEFT], waited_s=2.9))
        self.assertEqual(nf.replacement_trigger(self.PEER, [self.LEFT], waited_s=nf.CLOSE_TIMEOUT_S), ('close_timeout', None))
        self.assertEqual(nf.replacement_trigger(self.PEER, [self.LEFT, self.CLOSE], waited_s=5.0), ('uart_close', 508205727))
        self.assertEqual(nf.CLOSE_TIMEOUT_S, 3.0)
        self.assertIsNone(nf.replacement_trigger('unknown peer', [self.CLOSE], waited_s=1.0))

    def test_run3_stall_4_2_timeline_starts_nothing_before_the_close_line(self):
        # Retained host times: departure line 508.930, cut decoder EOF 509.935, close line 510.436.
        departed = [dict(peer=self.PEER, index=0, at_s=508.930, replaced=False)]
        lines = [self.LEFT]
        starts = []
        for now_s in (508.95, 509.45, 509.95, 510.45, 510.95):
            if now_s >= 510.436 and self.CLOSE not in lines:
                lines.append(self.CLOSE)
            for pending, trigger, close_device_us in nf.replacement_decisions(departed, lines, now_s, fresh_count=len(starts), limit=2):
                pending['replaced'] = True
                starts.append((now_s, trigger, close_device_us))
        self.assertEqual(starts, [(510.45, 'uart_close', 508205727)])
        self.assertTrue(departed[0]['replaced'])
        # Without a close line the fallback fires at the timeout and says so.
        departed = [dict(peer=self.PEER, index=0, at_s=508.930, replaced=False)]
        self.assertEqual(nf.replacement_decisions(departed, [self.LEFT], 511.9, fresh_count=0, limit=2), [])
        decisions = nf.replacement_decisions(departed, [self.LEFT], 511.95, fresh_count=0, limit=2)
        self.assertEqual([(d[1], d[2]) for d in decisions], [('close_timeout', None)])
        # A fresh listener's own departure cannot spawn replacements beyond the original listener count.
        departed = [dict(peer=self.PEER, index=0, at_s=500.0, replaced=False), dict(peer=self.OTHER, index=1, at_s=500.0, replaced=False)]
        self.assertEqual(nf.replacement_decisions(departed, [self.LEFT, self.OTHER_CLOSE, self.CLOSE], 505.0, fresh_count=1, limit=1), [])
        self.assertEqual(len(nf.replacement_decisions(departed, [self.LEFT, self.OTHER_CLOSE, self.CLOSE], 505.0, fresh_count=0, limit=1)), 1)

    def test_driver_uses_the_trigger_function_and_never_the_departure_line(self):
        source = inspect.getsource(nf.Driver.run_trial)
        self.assertIn('replacement_decisions(', source)
        self.assertNotIn("'uart_departure'", source)
        self.assertNotIn("'decoder_eof'", source)


class RefusedStatusTests(unittest.TestCase):
    # Tail of target/kata-hardening/oct14-interruption-3/stall-4-2-fresh-1.ffmpeg.log.
    LOG = ('[info] ffmpeg stats and -progress period set to 1.\n'
           '[http @ 0x5b53af5b0940] [warning] HTTP error 503 Service Unavailable\n'
           '[in#0 @ 0x5b53af5b0240] [error] Error opening input: Server returned 5XX Server Error reply\n'
           '[error] Error opening input file http://192.168.23.217:8000/stream.mp3.\n'
           '[fatal] Error opening input files: Server returned 5XX Server Error reply\n')

    def test_refused_status_reads_the_http_code_or_says_unknown(self):
        self.assertEqual(nf.refused_status(self.LOG), 503)
        self.assertEqual(nf.refused_status(self.LOG.replace('HTTP error 503 Service Unavailable', 'HTTP error 404 Not Found')), 404)
        self.assertEqual(nf.refused_status('[fatal] Error opening input files: Server returned 5XX Server Error reply\n'), 'unknown')
        self.assertEqual(nf.refused_status(''), 'unknown')

    def test_missing_join_reason_names_the_refusal(self):
        trial = nf.trial_setup(nf.trial_spec('stall-4-2'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_4_2_EVENTS))
        refused = [dict(event, refused_status=503) if event['kind'] == 'fresh' else event for event in RUN3_STALL_4_2_EVENTS]
        verdict = nf.trial_verdict(trial, refused)
        self.assertFalse(verdict['passed'])
        self.assertIn('refused', verdict['reason'])
        self.assertIn('503', verdict['reason'])
        self.assertTrue(inspect.getsource(nf.Driver.run_trial).count('refused_status('))


class ReverdictTests(unittest.TestCase):
    def synthetic_run(self, directory):
        null_events = [dict(kind='decoder', label='null-1-listener-1', frames=2880000, expected_frames=2880000, discontinuities=0,
                            exit_code=0, ended='completed', ended_s=85.0),
                       dict(kind='admission', peer='p0', device_us=1_000_000, at_s=25.0),
                       departure('p0', RESET, 61_000_000, 85.3), close('p0', 61_000_500, 85.3)]
        before = dict(RUN4_BEFORE, kind='metrics', settings=dict(active_format='mp3', autoplay=False))
        trials = {'null-1': dict(name='null-1', started_s=24.0, ended_s=90.0, passed=True, reason='original', before=before, events=null_events),
                  'stall-4-1': dict(name='stall-4-1', started_s=436.0, ended_s=486.0, passed=False,
                                    reason='media age did not return under the null bound', before=before, events=RUN4_STALL_4_1_EVENTS),
                  'stall-1-1': dict(name='stall-1-1', started_s=500.0, ended_s=500.5, passed=False, reason='settings snapshot unavailable', before=before)}
        result = dict(passed=False, trial_order=['null-1', 'stall-4-1', 'stall-1-1'], trials=trials, tolerances=None)
        (directory / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        lines = [dict(kind='ready', elapsed_seconds=20.0)]
        lines += [dict(sample(at_s, 100_000), elapsed_seconds=at_s) for at_s in range(25, 90)]
        lines += [dict(item, elapsed_seconds=item['at_s']) for item in RUN4_STALL_4_1_SAMPLES]
        lines.append(dict(kind='metrics_unavailable', elapsed_seconds=457.0, error='TimeoutError'))
        (directory / 'events.jsonl').write_text(''.join(json.dumps(line) + '\n' for line in lines))

    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def test_reverdict_recomputes_from_retained_records_without_touching_them(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / 'run'
            run.mkdir()
            self.synthetic_run(run)
            before = {name: self.digest(run / name) for name in ('result.json', 'events.jsonl')}
            cwd = Path(directory) / 'cwd'
            cwd.mkdir()
            completed = subprocess.run([str(SCRIPTS / 'network-fault'), '--reverdict', str(run)], cwd=cwd, capture_output=True, text=True, timeout=60)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(list(cwd.iterdir()), [])
            self.assertEqual(sorted(path.name for path in run.iterdir()), ['events.jsonl', 'result.json', 'reverdict.json'])
            self.assertEqual({name: self.digest(run / name) for name in before}, before)
            written = json.loads((run / 'reverdict.json').read_text())
            self.assertEqual(written['source_result_sha256'], before['result.json'])
            self.assertTrue(written['harness_commit'] is None or re.fullmatch('[0-9a-f]{40}', written['harness_commit']))
            self.assertEqual(written['harness_sha256'], {name: self.digest(SCRIPTS / name) for name in ('network_fault.py', 'encoder_fault_check.py')})
            self.assertIn(written['harness_dirty'], (True, False, None))
            self.assertEqual(written['original'], {'null-1': True, 'stall-4-1': False, 'stall-1-1': False})
            self.assertTrue(written['trials']['null-1']['passed'], written['trials']['null-1'])
            self.assertEqual(written['tolerances'], dict(age_bound_us=100_000, largest_tolerance_bytes=dict(internal_largest=0, psram_largest=0), source=['null-1']))
            self.assertTrue(written['trials']['stall-4-1']['passed'], written['trials']['stall-4-1'])
            self.assertEqual(written['trials']['stall-4-1']['age_last_active_us'], 56819)
            self.assertFalse(written['trials']['stall-1-1']['passed'])
            self.assertEqual(written['trials']['stall-1-1']['reason'], 'settings snapshot unavailable')
            for name in ('null-1', 'stall-4-1', 'stall-1-1'):
                self.assertIn(name, completed.stdout)

    def test_reverdict_of_a_copy_of_the_retained_device_run(self):
        retained = SCRIPTS.parent / 'target/kata-hardening/oct14-interruption-4'
        if not (retained / 'result.json').exists() or not (retained / 'events.jsonl').exists():
            self.skipTest('retained run oct14-interruption-4 not present')
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / 'copy'
            run.mkdir()
            for name in ('result.json', 'events.jsonl'):
                shutil.copyfile(retained / name, run / name)
            written, lines = nf.reverdict(run, SCRIPTS.parent)
            self.assertEqual(json.loads((run / 'reverdict.json').read_text()), written)
            self.assertEqual(written['source_result_sha256'], self.digest(retained / 'result.json'))
            self.assertEqual(set(written['trials']), set(nf.TRIALS))
            self.assertEqual(set(written['original']), set(nf.TRIALS))
            self.assertEqual(written['tolerances'], json.loads((retained / 'result.json').read_text())['tolerances'])
            self.assertEqual(len(lines), len(nf.TRIALS) + 1)
        self.assertFalse((retained / 'reverdict.json').exists() and False)

    def test_reverdict_fails_closed_without_null_tolerances_and_names_unrecorded_trials(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / 'run'
            run.mkdir()
            self.synthetic_run(run)
            result = json.loads((run / 'result.json').read_text())
            # The null decoder loses a frame: no tolerances, so the stall trial cannot be judged.
            result['trials']['null-1']['events'][0]['frames'] -= 1
            result['trial_order'].append('deficit-0.5-1')
            (run / 'result.json').write_text(json.dumps(result))
            written, _ = nf.reverdict(run, SCRIPTS.parent)
            self.assertFalse(written['trials']['null-1']['passed'])
            self.assertIsNone(written['tolerances'])
            self.assertFalse(written['trials']['stall-4-1']['passed'])
            self.assertIn('null-*', written['trials']['stall-4-1']['reason'])
            self.assertEqual(written['trials']['deficit-0.5-1'], dict(passed=False, reason='trial not recorded'))
            self.assertIsNone(written['original']['deficit-0.5-1'])


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
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null'], 2),
                     ([], 2),
                     (['--reverdict', str(Path(directory) / 'missing-run')], 2),
                     (['--reverdict', str(existing), BOARD], 2),
                     (['--reverdict', str(existing), '--dry-run'], 2),
                     (['--reverdict', str(existing), '--trials', 'null-1'], 2),
                     (['--reverdict', str(existing), '--out', 'x'], 2)]
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
