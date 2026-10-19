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


def departure(peer, end, device_us, at_s, skips=None, frames_skipped=None, last_skip_us=None):
    return dict(kind='departure', peer=peer, end=end, device_us=device_us, at_s=at_s,
                skips=skips, frames_skipped=frames_skipped, last_skip_us=last_skip_us)


def exact_setup(spec, before, tolerances, installed):
    return nf.trial_setup(spec, before, tolerances, installed, nf.EXACT)


def live_setup(spec, before, tolerances, installed):
    return nf.trial_setup(spec, before, tolerances, installed, nf.LIVE)


def close(peer, device_us, at_s, released_us=None, reaper_stack_free_bytes=None):
    return dict(kind='close', peer=peer, device_us=device_us, at_s=at_s, released_us=released_us, reaper_stack_free_bytes=reaper_stack_free_bytes)


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


class CloseLineTests(unittest.TestCase):
    def test_close_line_matches_with_and_without_the_release_stamp(self):
        line = ('I (37827) esp_shazam_firmware::stream: stream: 192.168.23.153:60800 socket closed in 242 ms; '
                'releasing slot (1 held) event_us=32605374 owner_id=3 sonos_intent=0')
        self.assertEqual(nf.CLOSED.search(line).group(1), '192.168.23.153:60800')
        self.assertEqual(nf.CLOSED.search(line + ' released_us=32363100').group(1), '192.168.23.153:60800')
        self.assertEqual(nf.listener_event_us(line + ' released_us=32363100'), 32605374)

    def test_close_events_carry_the_reaper_release_stamp_when_present(self):
        # Real lines from oct19-reaper-capture null-1 (image 2ed701de…, the close reaper), peer masked.
        lines = ['I (22959) esp_shazam_firmware::stream: stream: 192.0.2.1:49452 joined /stream.mp3 (1 of 2) event_us=21704612 owner_id=1 sonos_intent=0 role=diagnostic',
                 'I (84519) esp_shazam_firmware::stream: stream: 192.0.2.1:49452 left after 61.2 s: socket error kind=ConnectionReset raw_os_error=Some(104) '
                 'skips=0 frames_skipped=0 last_skip_us=0 listener_stack_free_bytes=1016 event_us=82951350',
                 'I (84559) esp_shazam_firmware::stream: stream: 192.0.2.1:49452 socket closed in 0 ms; releasing slot (1 held) event_us=82952492 owner_id=1 '
                 'sonos_intent=0 released_us=82951564 reaper_stack_free_bytes=2272',
                 'I (84609) esp_shazam_firmware::stream: listener-telemetry: dropped=0 loss_occurred=false observed_event_range_valid=true '
                 'observed_event_min_us=82951350 observed_event_max_us=82952492 record_bytes=368 queue_bytes=2984 metrics_stack_free_bytes=360 export_us=83359689']
        events = nf.uart_events(lines, [22.9, 84.5, 84.6, 84.6])
        close = [event for event in events if event['kind'] == 'close']
        self.assertEqual(close, [dict(kind='close', peer='192.0.2.1:49452', device_us=82952492, at_s=84.6, released_us=82951564, reaper_stack_free_bytes=2272)])
        # Departure, release and close are ordered by construction: the slot is handed off before the checked close.
        departure = [event for event in events if event['kind'] == 'departure'][0]
        self.assertLess(departure['device_us'], close[0]['released_us'])
        self.assertLess(close[0]['released_us'], close[0]['device_us'])
        # Lines from the image before the reaper carry neither stamp.
        old = lines[2].split(' released_us=')[0]
        close = [event for event in nf.uart_events([lines[1], old, lines[3]], [84.5, 84.6, 84.6]) if event['kind'] == 'close']
        self.assertEqual((close[0]['released_us'], close[0]['reaper_stack_free_bytes']), (None, None))


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
                                     'stall-2.5-1', 'stall-2.5-2', 'stall-4-1', 'stall-4-2', 'stall-8-1', 'stall-8-2',
                                     'stall-15-1', 'stall-15-2', 'stall-30-1', 'stall-30-2', 'deficit-0.8-1', 'deficit-0.5-1',
                                     'delayed-reader-1', 'freeze-1', 'freeze-2'])
        self.assertEqual(nf.trial_spec('stall-15-2'), dict(name='stall-15-2', kind='stall', listeners=2, seconds=20 + 15 + 20,
                                                           profile=nf.stall_profile(15.0)))
        # A frozen reader departs at the live deadline; the survivor outlives the departure, the close and the fresh listener.
        self.assertEqual(nf.trial_spec('freeze-2'), dict(name='freeze-2', kind='freeze', listeners=2, seconds=nf.FREEZE_SECONDS, profile=None))
        self.assertGreater(nf.FREEZE_SECONDS, nf.STEADY_SECONDS + nf.LIVE_WRITE_DEADLINE_S + nf.CLOSE_TIMEOUT_S + nf.FRESH_LISTENER_SECONDS)

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
                    for policy in nf.POLICIES:
                        trial = nf.trial_setup(nf.trial_spec(name), BEFORE, tolerances, installed, policy)
                        self.assertLessEqual(set(nf.VERDICT_TRIAL_KEYS), set(trial), name)
                        self.assertEqual(trial['policy'], policy)
        with self.assertRaises(ValueError):
            nf.trial_setup(nf.trial_spec('null-1'), BEFORE, None, None, 'strict')

    def test_setup_predicts_from_the_unbuffered_bound_and_measured_tolerances(self):
        installed = dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.25)
        trial = exact_setup(nf.trial_spec('deficit-0.5-1'), BEFORE, TOLERANCES, installed)
        # Plain /stream.mp3 is Route::Stream with no preset: MediaAge::for_stream(None, Mp3) bounds at 4 s.
        self.assertEqual(trial['media_age_bound_s'], 4.0)
        self.assertNotIn('preset', trial)
        self.assertAlmostEqual(trial['predicted_end_s'], 8.0)
        self.assertAlmostEqual(trial['end_tolerance_s'], nf.end_tolerance_s(600_000, 0.5, 0.25))
        self.assertEqual(trial['expected_frames'], 48000 * 60)
        self.assertEqual(trial['largest_before'], BEFORE)
        self.assertEqual(trial['largest_tolerance_bytes'], TOLERANCES['largest_tolerance_bytes'])
        self.assertIsNone(exact_setup(nf.trial_spec('deficit-0.5-1'), BEFORE, TOLERANCES, None)['end_tolerance_s'])
        self.assertIsNone(exact_setup(nf.trial_spec('deficit-0.5-1'), BEFORE, None, installed)['end_tolerance_s'])
        self.assertIsNone(exact_setup(nf.trial_spec('stall-1-1'), BEFORE, None, None)['age_bound_us'])


class VerdictTests(unittest.TestCase):
    def null_trial(self, listeners=1):
        return exact_setup(nf.trial_spec(f'null-{listeners}'), BEFORE, None, None)

    def stall_trial(self, seconds=4, listeners=1):
        return exact_setup(nf.trial_spec(f'stall-{seconds:g}-{listeners}'), BEFORE, TOLERANCES, None)

    def deficit_trial(self, fraction=0.5):
        installed = dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.25)
        return dict(exact_setup(nf.trial_spec(f'deficit-{fraction:g}-1'), BEFORE, TOLERANCES, installed),
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
        trial = exact_setup(nf.trial_spec('delayed-reader-1'), BEFORE, None, None)
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
        self.assertEqual(nf.settings_problem(dict(settings, autoplay=True)), 'autoplay is enabled; the harness needs it off')
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
        # A live listener keeps its socket through the stall, so the decoder's wall time grows by the stall.
        self.assertGreaterEqual(nf.budget_seconds(['stall-15-1']), 180 + 55 + 15 + nf.FRESH_LISTENER_SECONDS + 30)
        self.assertGreaterEqual(nf.budget_seconds(['stall-8-2']), 180 + 48 + 8 + nf.FRESH_LISTENER_SECONDS + 30)

    def test_apply_records_the_injection_event_it_returns(self):
        # The first device run died here: the event dict already names its kind (oct14-interruption result.json).
        with tempfile.TemporaryDirectory() as out:
            args = argparse.Namespace(host='192.0.2.1', interface='lo', ifb='ifb0', trials=['null-1'], out=Path(out), policy=nf.LIVE)
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
# Attempt 5 (target/kata-hardening/oct14-interruption-5): the WAV reader's connect() timed out; wav_reader record
# merged into the wav event as the driver now records it (phase/connected_s/error/error_s).
RUN5_WAV_RECORD = dict(label='delayed-reader-1-wav', error="TimeoutError('timed out')", error_s=545.1624819659919)
RUN5_DELAYED_READER_EVENTS = [
    dict(kind='decoder', label='delayed-reader-1-listener-1', frames=1920000, expected_frames=1920000, discontinuities=0, exit_code=0, ended='completed', ended_s=575.7049240759807),
    dict(kind='wav', closed_s=None, connected_s=None, phase='connect', error="TimeoutError('timed out')", error_s=545.1624819659919),
    dict(kind='admission', peer='192.168.23.153:48022', device_us=533982213, at_s=535.6526196140039),
    dict(kind='departure', peer='192.168.23.153:48022', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=574402319, at_s=576.7066269039933),
    dict(kind='close', peer='192.168.23.153:48022', device_us=574402984, at_s=576.7066337589931),
    sample(535.336, None), sample(581.861, None),
]
# Attempt 5 stall-4-2: both decoders died on MediaAge at T; the close lines came 0.2 s and 0.4 s later.
RUN5_STALL_4_2_T = 431.33311865499127
RUN5_STALL_4_2_LINES = [
    'I (429831) esp_shazam_firmware::stream: stream: 192.168.23.153:45016 left after 25.0 s: media age exceeded listener_stack_free_bytes=1652 event_us=427420327',
    'I (429831) esp_shazam_firmware::stream: stream: 192.168.23.153:45014 left after 25.0 s: media age exceeded listener_stack_free_bytes=1652 event_us=427421438',
    'I (431533) esp_shazam_firmware::stream: stream: 192.168.23.153:45014 socket closed in 2401 ms; releasing slot (1 held) event_us=429823193 owner_id=0 sonos_intent=0',
    'I (431733) esp_shazam_firmware::stream: stream: 192.168.23.153:45016 socket closed in 2402 ms; releasing slot (0 held) event_us=429823492 owner_id=0 sonos_intent=0',
]
RUN5_STALL_4_2_CLOSE_AT = {2: 431.53370176799945, 3: 431.7339414629969}


def installed_of(events):
    return next(event for event in events if event['kind'] == 'injection' and event['phase'] == 'installed')


# Retained freeze-2 from oct19-reaper-freeze (image 2ed701de…, the close reaper; peers masked): the frozen reader departed
# 53.1 s after SIGSTOP (receive buffers absorb ~29 s before the 24 s live deadline), the reaper released its slot 179 us later,
# the fresh listener joined 1.28 s after the release and 1.0 s before the lingering close completed.
FREEZE_2_EVENTS = [
    dict(kind='decoder', label='freeze-2-listener-1', frames=903168, expected_frames=3600000, discontinuities=0, exit_code=255, ended='frozen',
         ended_s=133.47590858698823, skips=0, frames_skipped=0, skip_peer='192.0.2.1:33812'),
    dict(kind='decoder', label='freeze-2-listener-2', frames=3600000, expected_frames=3600000, discontinuities=0, exit_code=0, ended='completed',
         ended_s=193.59056590299588, skips=1, frames_skipped=180, skip_peer='192.0.2.1:33806'),
    dict(kind='fresh', label='freeze-2-fresh-1', ended='completed', ended_s=197.59651977196336, trigger='uart_departure', close_device_us=None, refused_status=None),
    dict(kind='freeze', label='freeze-2-listener-1', at_s=133.47590858698823),
    dict(kind='admission', peer='192.0.2.1:33812', device_us=112365708, at_s=114.42719076399226),
    dict(kind='admission', peer='192.0.2.1:33806', device_us=112371296, at_s=114.42719389603008),
    dict(kind='departure', peer='192.0.2.1:33812', end='stopped reading', device_us=184218350, at_s=186.57376874098554, skips=0, frames_skipped=0, last_skip_us=0),
    dict(kind='admission', peer='192.0.2.1:38914', device_us=185502930, at_s=187.57582773594186),
    dict(kind='departure', peer='192.0.2.1:33806', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=192267042, at_s=194.59208966896404,
         skips=1, frames_skipped=180, last_skip_us=184365491),
    dict(kind='departure', peer='192.0.2.1:38914', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=196270020, at_s=198.19714081799611,
         skips=0, frames_skipped=0, last_skip_us=0),
    dict(kind='close', peer='192.0.2.1:33812', device_us=186500778, at_s=188.57722347893286, released_us=184218529, reaper_stack_free_bytes=2280),
    dict(kind='close', peer='192.0.2.1:33806', device_us=192268452, at_s=194.59213046298828, released_us=192267241, reaper_stack_free_bytes=2280),
    dict(kind='close', peer='192.0.2.1:38914', device_us=196271488, at_s=198.19714471395127, released_us=196270325, reaper_stack_free_bytes=2280),
]


# oct19-reaper-freeze-3 (peers masked): both readers joined 40 ms apart and the frozen decoder's peer was the second join;
# by join order the harness had paired the survivor's skip with the frozen decoder and the survivor's own reset with
# a stranger, turning a passing trial into 'unexpected departure'.
FREEZE_3_EVENTS = [
    dict(kind='decoder', label='freeze-2-listener-1', frames=911232, expected_frames=3600000, discontinuities=0, exit_code=255, ended='frozen', ended_s=42.756872572004795),
    dict(kind='decoder', label='freeze-2-listener-2', frames=3600000, expected_frames=3600000, discontinuities=0, exit_code=0, ended='completed', ended_s=103.33415767305996),
    dict(kind='fresh', label='freeze-2-fresh-1', ended='completed', ended_s=107.84054669504985, trigger='uart_departure', close_device_us=None, refused_status=None),
    dict(kind='freeze', label='freeze-2-listener-1', at_s=42.756872572004795),
    dict(kind='admission', peer='192.0.2.1:44916', device_us=22112626, at_s=23.72488906106446),
    dict(kind='admission', peer='192.0.2.1:44914', device_us=22152685, at_s=24.226102603017353),
    dict(kind='departure', peer='192.0.2.1:44914', end='stopped reading', device_us=94658507, at_s=96.81931339995936, skips=0, frames_skipped=0, last_skip_us=0),
    dict(kind='admission', peer='192.0.2.1:49092', device_us=95717549, at_s=97.82339795306325),
    dict(kind='departure', peer='192.0.2.1:44916', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=102640893, at_s=105.33685614005663,
         skips=1, frames_skipped=175, last_skip_us=41144489),
    dict(kind='departure', peer='192.0.2.1:49092', end='socket error kind=ConnectionReset raw_os_error=Some(104)', device_us=106273952, at_s=108.24150650203228,
         skips=0, frames_skipped=0, last_skip_us=0),
    dict(kind='close', peer='192.0.2.1:44914', device_us=97055241, at_s=98.8245550020365, released_us=94658681, reaper_stack_free_bytes=2272),
    dict(kind='close', peer='192.0.2.1:44916', device_us=102641955, at_s=105.33686378004495, released_us=102641096, reaper_stack_free_bytes=2272),
    dict(kind='close', peer='192.0.2.1:49092', device_us=106275829, at_s=108.24151081498712, released_us=106274152, reaper_stack_free_bytes=2272),
]


class FreezeVerdictTests(unittest.TestCase):
    def test_the_frozen_decoder_pairs_with_the_peer_that_stopped_reading(self):
        attributed = {event['label']: event.get('skip_peer') for event in nf.attribute_skips(FREEZE_3_EVENTS) if event['kind'] == 'decoder'}
        self.assertEqual(attributed, {'freeze-2-listener-1': '192.0.2.1:44914', 'freeze-2-listener-2': '192.0.2.1:44916'})
        verdict = nf.trial_verdict(live_setup(nf.trial_spec('freeze-2'), RUN4_BEFORE, RUN4_TOLERANCES, None), FREEZE_3_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        # The survivor's reset 2.004 s after its decoder completed is the harness close, matched through its own peer.
        self.assertEqual(verdict['harness_closes'], 2)
        self.assertEqual((verdict['survivor_skips'], verdict['survivor_frames_skipped']), (1, 175))
        # A retained run paired by join order (skips already on the decoders) is re-paired the same way.
        retained = [dict(event, skips=1, frames_skipped=175, skip_peer='192.0.2.1:44916') if event.get('label') == 'freeze-2-listener-1'
                    else dict(event, skips=0, frames_skipped=0, skip_peer='192.0.2.1:44914') if event.get('label') == 'freeze-2-listener-2' else event
                    for event in FREEZE_3_EVENTS]
        self.assertEqual({event['label']: (event['skip_peer'], event['skips']) for event in nf.attribute_skips(retained) if event['kind'] == 'decoder'},
                         {'freeze-2-listener-1': ('192.0.2.1:44914', 0), 'freeze-2-listener-2': ('192.0.2.1:44916', 1)})
        # Join order still pairs trials without a freeze.
        plain = [event for event in FREEZE_3_EVENTS if event['kind'] != 'freeze']
        plain = [dict(event, ended='stopped') if event.get('ended') == 'frozen' else event for event in plain]
        self.assertEqual({event['label']: event.get('skip_peer') for event in nf.attribute_skips(plain) if event['kind'] == 'decoder'},
                         {'freeze-2-listener-1': '192.0.2.1:44916', 'freeze-2-listener-2': '192.0.2.1:44914'})

    def trial(self):
        return live_setup(nf.trial_spec('freeze-2'), RUN4_BEFORE, RUN4_TOLERANCES, None)

    def test_retained_freeze_2_passes_with_the_join_inside_the_release_to_close_window(self):
        verdict = nf.trial_verdict(self.trial(), FREEZE_2_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual({key: verdict[key] for key in ('departure_to_release_us', 'release_to_join_us', 'join_to_close_us', 'release_to_close_us',
                                                       'reaper_stack_free_bytes', 'survivor_skips', 'survivor_frames_skipped', 'harness_closes')},
                         dict(departure_to_release_us=179, release_to_join_us=1284401, join_to_close_us=997848, release_to_close_us=2282249,
                              reaper_stack_free_bytes=2280, survivor_skips=1, survivor_frames_skipped=180, harness_closes=2))
        self.assertAlmostEqual(verdict['freeze_to_departure_s'], 53.098, places=3)
        self.assertIn('1 skip(s)', verdict['reason'])

    def test_a_close_without_the_release_stamp_is_the_old_image(self):
        events = [dict(event, released_us=None, reaper_stack_free_bytes=None) if event['kind'] == 'close' else event for event in FREEZE_2_EVENTS]
        verdict = nf.trial_verdict(self.trial(), events)
        self.assertFalse(verdict['passed'])
        self.assertIn('released_us', verdict['reason'])

    def test_a_join_after_the_close_is_outside_the_window(self):
        events = [dict(event, device_us=186500779) if event['kind'] == 'admission' and event['peer'] == '192.0.2.1:38914' else event
                  for event in FREEZE_2_EVENTS]
        verdict = nf.trial_verdict(self.trial(), events)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['reason'], 'fresh join landed outside the release-to-close window')
        self.assertEqual(verdict['join_to_close_us'], -1)

    def test_a_refused_fresh_listener_names_its_status(self):
        events = [dict(event, ended='stopped', refused_status=503) if event['kind'] == 'fresh' else event for event in FREEZE_2_EVENTS
                  if not (event['kind'] in ('admission', 'departure', 'close') and event['peer'] == '192.0.2.1:38914')]
        verdict = nf.trial_verdict(self.trial(), events)
        self.assertFalse(verdict['passed'])
        self.assertIn('503', verdict['reason'])

    def test_a_short_survivor_fails(self):
        events = [dict(event, frames=1800000, ended='cut', exit_code=251) if event.get('label') == 'freeze-2-listener-2' else event for event in FREEZE_2_EVENTS]
        verdict = nf.trial_verdict(self.trial(), events)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['reason'], 'survivor not continuous')

    def test_a_second_product_departure_fails(self):
        extra = dict(kind='departure', peer='192.0.2.1:33806', end='media age exceeded', device_us=190000000, at_s=192.3, skips=0, frames_skipped=0, last_skip_us=0)
        verdict = nf.trial_verdict(self.trial(), FREEZE_2_EVENTS + [extra])
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['reason'], 'unexpected departure')


class RetainedRunTests(unittest.TestCase):
    def test_run4_stall_4_1_survives_judged_on_last_active_age(self):
        trial = exact_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_STALL_4_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN4_STALL_4_1_EVENTS + RUN4_STALL_4_1_SAMPLES)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['reason'], 'continuous')
        self.assertEqual((verdict['age_peak_after_removal_us'], verdict['age_last_active_us']), (1550664, 56819))
        stuck = [dict(item, media_age_us=1550664) if item['media_age_us'] is not None else item for item in RUN4_STALL_4_1_SAMPLES]
        verdict = nf.trial_verdict(trial, RUN4_STALL_4_1_EVENTS + stuck)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['age_last_active_us'], 1550664)

    def test_fresh_join_before_the_old_close_counts_from_the_departure(self):
        trial = exact_setup(nf.trial_spec('deficit-0.5-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_DEFICIT_0_5_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN4_DEFICIT_0_5_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertAlmostEqual(verdict['observed_end_s'], 9.025, places=3)
        trial = exact_setup(nf.trial_spec('stall-2.5-1'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_2_5_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN3_STALL_2_5_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['departed'], 1)
        # The checked close is still required.
        self.assertFalse(nf.trial_verdict(trial, [event for event in RUN3_STALL_2_5_1_EVENTS if event['kind'] != 'close'])['passed'])

    def test_run4_delayed_reader_wav_ended_on_its_frame_write_limit(self):
        trial = exact_setup(nf.trial_spec('delayed-reader-1'), RUN4_BEFORE, RUN4_TOLERANCES, None)
        verdict = nf.trial_verdict(trial, RUN4_DELAYED_READER_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['wav_end_category'], 'frame_timeout')
        self.assertEqual(verdict['harness_closes'], 1)
        broken = [dict(event, end=RESET) if event.get('end') == 'stopped reading' else event for event in RUN4_DELAYED_READER_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, broken)['passed'])

    def test_deficit_prediction_is_an_upper_bound(self):
        trial = exact_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_DEFICIT_0_8_1_EVENTS))
        self.assertAlmostEqual(trial['predicted_end_s'], 20.0)
        self.assertAlmostEqual(trial['end_tolerance_s'], 1.729315)
        verdict = nf.trial_verdict(trial, RUN4_DEFICIT_0_8_1_EVENTS)
        self.assertTrue(verdict['passed'], verdict)
        self.assertAlmostEqual(verdict['observed_end_s'], 18.5425, places=3)
        trial3 = exact_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_DEFICIT_0_8_1_EVENTS))
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
        trial = exact_setup(nf.trial_spec('stall-4-2'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_4_2_EVENTS))
        verdict = nf.trial_verdict(trial, RUN3_STALL_4_2_EVENTS)
        self.assertFalse(verdict['passed'])
        self.assertIn('no fresh join', verdict['reason'])
        self.assertIn('192.168.23.153:33738', verdict['reason'])
        self.assertNotIn('timing', verdict['reason'])
        self.assertEqual(verdict['age_last_active_us'], 47551)
        joined = RUN3_STALL_4_2_EVENTS + [dict(kind='admission', peer='192.168.23.153:40000', device_us=508_900_000, at_s=511.2)]
        self.assertTrue(nf.trial_verdict(trial, joined)['passed'])


    def test_two_departures_consume_two_distinct_joins(self):
        trial = exact_setup(nf.trial_spec('stall-2.5-2'), RUN4_BEFORE, RUN3_TOLERANCES, dict(kind='injection', phase='installed', at_s=20, apply_seconds=0.02))
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
        trial = exact_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_DEFICIT_0_8_1_EVENTS))
        installed_at = installed_of(RUN4_DEFICIT_0_8_1_EVENTS)['at_s']
        early = [dict(event, at_s=installed_at + 0.3) if event.get('end') == 'media age exceeded' else event for event in RUN4_DEFICIT_0_8_1_EVENTS]
        verdict = nf.trial_verdict(trial, early)
        self.assertFalse(verdict['passed'])
        self.assertIn('earlier', verdict['reason'])
        self.assertAlmostEqual(verdict['earliest_end_s'], 0.017 + 4.0 - 0.142463)
        self.assertTrue(nf.trial_verdict(trial, RUN4_DEFICIT_0_8_1_EVENTS)['passed'])

    def test_harness_close_logged_just_before_the_decoder_end_is_still_harness_owned(self):
        trial = exact_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_STALL_4_1_EVENTS))
        ended_s = 480.91661526600365
        shifted = [dict(event, at_s=ended_s - 0.005) if event.get('end') == RESET else event for event in RUN4_STALL_4_1_EVENTS]
        verdict = nf.trial_verdict(trial, shifted + RUN4_STALL_4_1_SAMPLES)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['harness_closes'], 1)
        too_early = [dict(event, at_s=ended_s - nf.POLL_INTERVAL_S - 0.5) if event.get('end') == RESET else event for event in RUN4_STALL_4_1_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, too_early + RUN4_STALL_4_1_SAMPLES)['passed'])


    def test_run5_delayed_reader_fails_on_never_connected_not_missing_departure(self):
        trial = exact_setup(nf.trial_spec('delayed-reader-1'), RUN4_BEFORE, None, None)
        verdict = nf.trial_verdict(trial, RUN5_DELAYED_READER_EVENTS)
        self.assertFalse(verdict['passed'])
        self.assertTrue(verdict['reason'].startswith('WAV reader never connected'), verdict['reason'])
        self.assertIn("TimeoutError('timed out')", verdict['reason'])
        self.assertIn('connect', verdict['reason'])
        self.assertNotIn('departure', verdict['reason'])
        # Attempt 4's record, with and without the richer wav fields, still passes.
        trial4 = exact_setup(nf.trial_spec('delayed-reader-1'), RUN4_BEFORE, RUN4_TOLERANCES, None)
        self.assertTrue(nf.trial_verdict(trial4, RUN4_DELAYED_READER_EVENTS)['passed'])
        rich = [dict(event, connected_s=635.4, phase='reading', error=None, error_s=None) if event['kind'] == 'wav' else event
                for event in RUN4_DELAYED_READER_EVENTS]
        self.assertTrue(nf.trial_verdict(trial4, rich)['passed'])

    def test_null_failure_detail_lists_departures_and_decoder_end_text(self):
        trial = exact_setup(nf.trial_spec('null-1'), RUN4_BEFORE, None, None)
        events = [metrics(0), decoder('listener-1', 1_000_000, 2_880_000, exit_code=251, ended='ffmpeg integrity failure: Stream ends prematurely', ended_s=30),
                  departure('p1', 'media age exceeded', 50_000_000, 29.5), close('p1', 50_010_000, 31.5), metrics(60)]
        verdict = nf.trial_verdict(trial, events)
        self.assertFalse(verdict['passed'])
        self.assertEqual(verdict['decoders'][0]['ended'], 'ffmpeg integrity failure: Stream ends prematurely')
        self.assertEqual([item['end'] for item in verdict['departures']], ['media age exceeded'])


class LoopExitTests(unittest.TestCase):
    def test_run5_stall_4_2_loop_waits_for_the_close_lines_then_the_fresh_listeners(self):
        T = RUN5_STALL_4_2_T
        departed = [dict(peer='192.168.23.153:45016', index=0, at_s=429.83144429599633, replaced=False),
                    dict(peer='192.168.23.153:45014', index=1, at_s=429.8314734089945, replaced=False)]
        lines = RUN5_STALL_4_2_LINES[:2]
        fresh_ends = []
        for now_s in (T + 0.07, T + 0.57, T + 1.07):
            lines = [line for index, line in enumerate(RUN5_STALL_4_2_LINES) if index < 2 or RUN5_STALL_4_2_CLOSE_AT[index] <= now_s]
            for pending, trigger, _ in nf.replacement_decisions(departed, lines, now_s, len(fresh_ends), 2):
                pending['replaced'] = True
                fresh_ends.append(None)
            self.assertFalse(nf.may_exit(['cut', 'cut'], fresh_ends, departed, lines, now_s, injection_done=True), now_s)
        self.assertEqual(len(fresh_ends), 2)
        self.assertTrue(nf.may_exit(['cut', 'cut'], ['completed', 'completed'], departed, lines, T + 12, injection_done=True))
        # Without close lines the timeout releases the loop; a running decoder or pending injection never does.
        waiting = [dict(item, replaced=False) for item in departed]
        self.assertFalse(nf.may_exit(['cut', 'cut'], [], waiting, RUN5_STALL_4_2_LINES[:2], 429.8314734089945 + nf.CLOSE_TIMEOUT_S - 0.01, injection_done=True))
        self.assertTrue(nf.may_exit(['cut', 'cut'], [], waiting, RUN5_STALL_4_2_LINES[:2], 429.8314734089945 + nf.CLOSE_TIMEOUT_S, injection_done=True))
        self.assertFalse(nf.may_exit(['cut', None], [], [], [], T + 99, injection_done=True))
        self.assertFalse(nf.may_exit(['completed'], [], [], [], T + 99, injection_done=False))
        self.assertTrue(nf.may_exit(['completed'], [], [], [], T, injection_done=True))

    def test_driver_exits_through_may_exit_and_records_wav_phases(self):
        source = inspect.getsource(nf.Driver.run_trial)
        self.assertIn('may_exit(', source)
        self.assertNotIn("if all(job['ended'] is not None for job in jobs + fresh)", source)
        self.assertIn("connected_s=wav.get('connected_s')", source)
        reader = inspect.getsource(nf.Driver.trickle_reader)
        for phase in ('connect', 'request', 'reading'):
            self.assertIn(f"report['phase'] = '{phase}'", reader)
        self.assertLess(reader.index("report['phase'] = 'connect'"), reader.index('sock.connect('))


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

    def test_driver_uses_the_trigger_function_except_for_a_frozen_reader(self):
        # Stall and deficit replacements wait for the close line or its timeout (the path was black-holed anyway);
        # only the freeze trial starts its replacement at the departure line, to land the join inside the
        # release-to-close window the reaper opens.
        source = inspect.getsource(nf.Driver.run_trial)
        self.assertIn('replacement_decisions(', source)
        self.assertNotIn("'decoder_eof'", source)
        self.assertIn("spec['kind'] == 'freeze'", source)
        self.assertEqual(source.count("'uart_departure'"), 1)


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
        trial = exact_setup(nf.trial_spec('stall-4-2'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_4_2_EVENTS))
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
                  'stall-1-1': dict(name='stall-1-1', started_s=500.0, ended_s=500.5, passed=False, reason='settings snapshot unavailable', before=before),
                  'delayed-reader-1': dict(name='delayed-reader-1', started_s=534.8233159739757, ended_s=582.0, passed=False,
                                           reason='expected one WAV reader departure, found 0', before=before, wav_reader=dict(RUN5_WAV_RECORD),
                                           events=[event for event in RUN5_DELAYED_READER_EVENTS if event['kind'] != 'metrics' and event['kind'] != 'wav']
                                           + [dict(kind='wav', closed_s=None)])}
        result = dict(passed=False, trial_order=['null-1', 'stall-4-1', 'stall-1-1', 'delayed-reader-1'], trials=trials, tolerances=None)
        (directory / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        lines = [dict(kind='ready', elapsed_seconds=20.0)]
        lines += [dict(sample(at_s, 100_000), elapsed_seconds=at_s) for at_s in range(25, 90)]
        lines += [dict(item, elapsed_seconds=item['at_s']) for item in RUN4_STALL_4_1_SAMPLES]
        lines.append(dict(kind='metrics_unavailable', elapsed_seconds=457.0, error='TimeoutError'))
        lines += [dict(sample(at_s, None), elapsed_seconds=at_s) for at_s in (535.336, 581.861)]
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
            self.assertEqual(written['policy'], nf.LIVE)
            self.assertEqual(written['source_result_sha256'], before['result.json'])
            self.assertTrue(written['harness_commit'] is None or re.fullmatch('[0-9a-f]{40}', written['harness_commit']))
            self.assertEqual(written['harness_sha256'], {name: self.digest(SCRIPTS / name) for name in ('network_fault.py', 'encoder_fault_check.py')})
            self.assertIn(written['harness_dirty'], (True, False, None))
            self.assertEqual(written['original'], {'null-1': True, 'stall-4-1': False, 'stall-1-1': False, 'delayed-reader-1': False})
            # The retained wav event lacks the connect record; reverdict merges it from the trial's wav_reader.
            self.assertTrue(written['trials']['delayed-reader-1']['reason'].startswith('WAV reader never connected'))
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
            written, lines = nf.reverdict(run, SCRIPTS.parent, nf.EXACT)
            self.assertEqual(json.loads((run / 'reverdict.json').read_text()), written)
            self.assertEqual(written['policy'], nf.EXACT)
            # Attempt 4's stall-4-1 completed with no product departure: a pass under both policies.
            self.assertTrue(written['trials']['stall-4-1']['passed'], written['trials']['stall-4-1'])
            live, _ = nf.reverdict(run, SCRIPTS.parent, nf.LIVE)
            self.assertTrue(live['trials']['stall-4-1']['passed'], live['trials']['stall-4-1'])
            self.assertEqual(live['trials']['stall-4-1']['skips'], 0)
            # Its deficit trials ended in MediaAge departures: exact accepts them, live never does.
            self.assertTrue(written['trials']['deficit-0.8-1']['passed'])
            self.assertFalse(live['trials']['deficit-0.8-1']['passed'])
            self.assertIn('departed', live['trials']['deficit-0.8-1']['reason'])
            self.assertEqual(written['source_result_sha256'], self.digest(retained / 'result.json'))
            retained_result = json.loads((retained / 'result.json').read_text())
            self.assertEqual(list(written['trials']), retained_result['trial_order'])
            self.assertEqual(set(written['original']), set(retained_result['trial_order']))
            self.assertEqual(written['tolerances'], retained_result['tolerances'])
            self.assertEqual(len(lines), len(retained_result['trial_order']) + 1)
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
            written, _ = nf.reverdict(run, SCRIPTS.parent, nf.LIVE)
            self.assertFalse(written['trials']['null-1']['passed'])
            self.assertIsNone(written['tolerances'])
            self.assertFalse(written['trials']['stall-4-1']['passed'])
            self.assertIn('null-*', written['trials']['stall-4-1']['reason'])
            self.assertEqual(written['trials']['deficit-0.5-1'], dict(passed=False, reason='trial not recorded'))
            self.assertIsNone(written['original']['deficit-0.5-1'])
            self.assertFalse(written['trials']['delayed-reader-1']['passed'])


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
            full = subprocess.run([str(wrapper), '--dry-run', BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', str(out)],
                                  cwd=directory, capture_output=True, text=True, timeout=30)
            self.assertEqual(full.returncode, 0, full.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])
            for name in ('stall-8-1', 'stall-8-2', 'stall-15-1', 'stall-15-2'):
                self.assertIn(f'# trial {name}', full.stdout.splitlines())
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
                     (['--reverdict', str(existing), '--out', 'x'], 2),
                     (['--reverdict', str(existing), '--policy', 'strict'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', 'x', '--policy', 'strict', '--dry-run'], 2)]
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


# UART departure lines as the firmware logs them once the skip counters land; the harness reads the
# tokens wherever they sit and tolerates their absence (retained attempts 3-5 have none).
SKIP_LINE_AFTER = ('I (82627) esp_shazam_firmware::stream: stream: 192.168.23.153:52582 left after 60.1 s: socket error '
                   'kind=ConnectionReset raw_os_error=Some(104) listener_stack_free_bytes=1148 event_us=81182401 '
                   'skips=2 frames_skipped=333 last_skip_us=81000000')
SKIP_LINE_BEFORE = ('I (429831) esp_shazam_firmware::stream: stream: 192.168.23.153:45016 left after 25.0 s: media age exceeded '
                    'skips=1 frames_skipped=166 listener_stack_free_bytes=1652 event_us=427420327')
NO_SKIP_LINE = ('I (82627) esp_shazam_firmware::stream: stream: 192.168.23.153:52582 left after 60.1 s: socket error '
                'kind=ConnectionReset raw_os_error=Some(104) listener_stack_free_bytes=1148 event_us=81182401')


class SkipFieldTests(unittest.TestCase):
    def test_skip_fields_read_tokens_anywhere_and_tolerate_absence(self):
        self.assertEqual(nf.skip_fields(SKIP_LINE_AFTER), dict(skips=2, frames_skipped=333, last_skip_us=81000000))
        self.assertEqual(nf.skip_fields(SKIP_LINE_BEFORE), dict(skips=1, frames_skipped=166, last_skip_us=None))
        self.assertEqual(nf.skip_fields(NO_SKIP_LINE), dict(skips=None, frames_skipped=None, last_skip_us=None))

    def test_uart_departures_carry_skips_and_keep_the_end_text_clean(self):
        events = nf.uart_events([SKIP_LINE_AFTER, SKIP_LINE_BEFORE, NO_SKIP_LINE], at_s=[1.0, 2.0, 3.0])
        self.assertEqual(events, [
            departure('192.168.23.153:52582', RESET, 81182401, 1.0, skips=2, frames_skipped=333, last_skip_us=81000000),
            departure('192.168.23.153:45016', 'media age exceeded', 427420327, 2.0, skips=1, frames_skipped=166),
            departure('192.168.23.153:52582', RESET, 81182401, 3.0)])
        self.assertEqual([nf.listener_end_category(event['end']) for event in events], ['socket_error', 'media_age', 'socket_error'])

    def test_metrics_sample_sums_active_slot_skips_or_says_none(self):
        helper = DriverHelperTests()
        snapshot = helper.snapshot(ages=(500_000, 900_000), states=('writing', 'ready'))
        self.assertIsNone(nf.metrics_sample(snapshot, 1)['skips'])
        self.assertIsNone(nf.metrics_sample(snapshot, 1)['frames_skipped'])
        snapshot['listeners'][0].update(skips=2, frames_skipped=300, last_skip_us=5)
        snapshot['listeners'][1].update(skips=1, frames_skipped=100, last_skip_us=6)
        sample = nf.metrics_sample(snapshot, 1)
        self.assertEqual((sample['skips'], sample['frames_skipped']), (3, 400))
        idle = helper.snapshot(ages=(None, None), states=('idle', 'idle'))
        idle['listeners'][0].update(skips=7, frames_skipped=900)
        self.assertEqual(nf.metrics_sample(idle, 1)['skips'], 0)

    def test_attribute_skips_pairs_decoders_with_admissions_in_device_order(self):
        events = [decoder('t-listener-1', 10, 10), decoder('t-listener-2', 10, 10),
                  admission('p2', 2_000, 1.1), admission('p1', 1_000, 1.0),
                  departure('p1', RESET, 50_000, 9, skips=1, frames_skipped=166), departure('p2', RESET, 50_100, 9, skips=3, frames_skipped=500),
                  admission('p1', 60_000, 12), departure('p1', RESET, 70_000, 13, skips=9, frames_skipped=999)]
        decoders = [event for event in nf.attribute_skips(events) if event['kind'] == 'decoder']
        self.assertEqual([(event['label'], event['skips'], event['frames_skipped'], event['skip_peer']) for event in decoders],
                         [('t-listener-1', 1, 166, 'p1'), ('t-listener-2', 3, 500, 'p2')])
        # Without counters on the lines the decoders say so; a departure before the admission never pairs.
        bare = [decoder('t-listener-1', 10, 10), admission('p1', 1_000, 1.0), departure('p1', RESET, 500, 0.5, skips=4), departure('p1', RESET, 5_000, 9)]
        self.assertEqual([event.get('skips') for event in nf.attribute_skips(bare) if event['kind'] == 'decoder'], [None])
        self.assertEqual(nf.attribute_skips(bare)[1:], bare[1:])
        # Already-attributed decoders (a retained result.json) are kept.
        kept = [dict(decoder('t-listener-1', 10, 10), skips=5, frames_skipped=1, skip_peer='x'), admission('p1', 1_000, 1.0)]
        self.assertEqual(nf.attribute_skips(kept)[0]['skips'], 5)

    def test_trial_skips_prefers_departure_counters_then_metrics_then_zero(self):
        lines = [departure('p1', RESET, 50_000, 9, skips=1, frames_skipped=166), departure('p2', RESET, 50_100, 9, skips=3, frames_skipped=500)]
        self.assertEqual(nf.trial_skips(lines + [dict(metrics(5), skips=9, frames_skipped=9)]), dict(skips=4, frames_skipped=666, source='departures'))
        self.assertEqual(nf.trial_skips([dict(metrics(5), skips=2, frames_skipped=300), dict(metrics(6), skips=3, frames_skipped=400), metrics(7)]),
                         dict(skips=3, frames_skipped=400, source='metrics'))
        self.assertEqual(nf.trial_skips([metrics(5), departure('p1', RESET, 1, 1)]), dict(skips=0, frames_skipped=0, source='none'))

    def test_decoder_timeouts_grow_with_the_stall(self):
        self.assertEqual(nf.decoder_timeouts(nf.trial_spec('null-1')), dict(rw_timeout_s=10, progress_timeout_s=20))
        self.assertEqual(nf.decoder_timeouts(nf.trial_spec('deficit-0.5-1')), dict(rw_timeout_s=10, progress_timeout_s=20))
        self.assertEqual(nf.decoder_timeouts(nf.trial_spec('stall-15-2')), dict(rw_timeout_s=25, progress_timeout_s=35))
        self.assertEqual(nf.decoder_timeouts(nf.trial_spec('stall-0.5-1')), dict(rw_timeout_s=10.5, progress_timeout_s=20.5))


# Attempt 4's stall-4-1 as the firmware will log it once the listener skips instead of departing: the decoder completed,
# the only departure is the harness's own close, and that line carries the skip counters.
LIVE_STALL_4_1_EVENTS = [dict(event, skips=1, frames_skipped=166, last_skip_us=458_400_000) if event['kind'] == 'departure' else event
                         for event in RUN4_STALL_4_1_EVENTS]


def deficit_samples(installed_s, removed_s, period_s, skips_at_start=0):
    """One-second metrics whose skip counter rises every period_s after install; idle (no age) after removal."""
    out = []
    for at in range(int(installed_s) - 2, int(removed_s) + 20):
        held = min(at, removed_s) - installed_s
        skips = skips_at_start + (int(held // period_s) if held >= 0 else 0)
        out.append(dict(sample(at, 100_000 if at < removed_s + 10 else None), skips=skips, frames_skipped=skips * 166))
    return out


class LivePolicyTests(unittest.TestCase):
    def test_live_stall_passes_with_no_departure_and_discontinuities_within_the_skips(self):
        trial = live_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(LIVE_STALL_4_1_EVENTS))
        verdict = nf.trial_verdict(trial, LIVE_STALL_4_1_EVENTS + RUN4_STALL_4_1_SAMPLES)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual((verdict['skips'], verdict['frames_skipped'], verdict['departed']), (1, 166, 0))
        self.assertEqual(verdict['decoders'][0]['skips'], 1)
        self.assertEqual(verdict['age_last_active_us'], 56819)
        self.assertIn('1 skip', verdict['reason'])
        # The decoder may stop one skip's worth of audio early (ffmpeg -t ends at the jumped timeline), within one MP3 frame.
        short = [dict(event, frames=2112000 - 166 * 1152 - 1152, ended='short') if event['kind'] == 'decoder' else event for event in LIVE_STALL_4_1_EVENTS]
        self.assertTrue(nf.trial_verdict(trial, short + RUN4_STALL_4_1_SAMPLES)['passed'])
        shorter = [dict(event, frames=2112000 - 167 * 1152 - 1153) if event['kind'] == 'decoder' else event for event in LIVE_STALL_4_1_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, shorter + RUN4_STALL_4_1_SAMPLES)['passed'])
        gapped = [dict(event, discontinuities=1) if event['kind'] == 'decoder' else event for event in LIVE_STALL_4_1_EVENTS]
        self.assertTrue(nf.trial_verdict(trial, gapped + RUN4_STALL_4_1_SAMPLES)['passed'])
        over = [dict(event, discontinuities=2) if event['kind'] == 'decoder' else event for event in LIVE_STALL_4_1_EVENTS]
        verdict = nf.trial_verdict(trial, over + RUN4_STALL_4_1_SAMPLES)
        self.assertFalse(verdict['passed'])
        self.assertIn('discontinuit', verdict['reason'])
        failed = [dict(event, exit_code=1) if event['kind'] == 'decoder' else event for event in LIVE_STALL_4_1_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, failed + RUN4_STALL_4_1_SAMPLES)['passed'])
        # Any product departure fails, including the MediaAge end exact policy accepts with a fresh admission.
        departed = LIVE_STALL_4_1_EVENTS + [departure('p9', 'media age exceeded', 458_000_000, 459.0), close('p9', 458_100_000, 459.1),
                                            admission('p10', 459_000_000, 460.0)]
        verdict = nf.trial_verdict(trial, departed + RUN4_STALL_4_1_SAMPLES)
        self.assertFalse(verdict['passed'])
        self.assertIn('departed', verdict['reason'])
        stuck = [dict(item, media_age_us=1550664) if item['media_age_us'] is not None else item for item in RUN4_STALL_4_1_SAMPLES]
        self.assertFalse(nf.trial_verdict(trial, LIVE_STALL_4_1_EVENTS + stuck)['passed'])
        self.assertFalse(nf.trial_verdict(dict(trial, age_bound_us=None), LIVE_STALL_4_1_EVENTS + RUN4_STALL_4_1_SAMPLES)['passed'])

    def test_ring_depth_rule_splits_the_stall_matrix(self):
        # Twice the MP3 ring: 2 x 500 frames x 1152 samples / 48 kHz. 15 s stalls must survive; 30 s is beyond.
        self.assertAlmostEqual(nf.LIVE_WRITE_DEADLINE_S, 24.0)
        self.assertEqual([nf.beyond_ring_depth(nf.trial_spec(name)['profile']['seconds']) for name in nf.TRIALS if name.startswith('stall')],
                         [False] * 12 + [True, True])
        self.assertFalse(nf.beyond_ring_depth(24.0))
        self.assertTrue(nf.beyond_ring_depth(24.01))

    def beyond_events(self, listeners=1):
        trial = live_setup(nf.trial_spec(f'stall-30-{listeners}'), RUN4_BEFORE, RUN4_TOLERANCES, None)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 50.1), metrics(65, age=None)]
        for index in range(1, listeners + 1):
            # The write deadline fires mid-stall; the harness replaces the listener once its close line arrives.
            events += [admission(f'p{index}', 1_000_000 + index, 0.5),
                       departure(f'p{index}', 'stopped reading', 50_000_000 + index, 44.0 + index / 10, skips=0, frames_skipped=0),
                       close(f'p{index}', 50_010_000 + index, 44.2 + index / 10),
                       admission(f'f{index}', 56_000_000 + index, 49.0 + index / 10),
                       decoder(f'l-listener-{index}', expected // 3, expected, exit_code=251, ended='cut', ended_s=44.5 + index / 10),
                       fresh(f'l-fresh-{index}', 59.3 + index / 10), reset_after(f'f{index}', 59.3 + index / 10, device_us=66_000_000 + index)]
        return trial, events

    def test_board_reset_after_a_complete_decoder_is_the_harness_close_even_when_late(self):
        # oct18-interruption-2 deficit-0.8-1: the decoder completed every frame at 73.63 s and the board's
        # ConnectionReset line landed 2.000 s later, because under the rate limit it noticed our close only on
        # its next write. That reset can only be ours.
        trial = live_setup(nf.trial_spec('deficit-0.8-1'), RUN4_BEFORE, None, injection('installed', 20.07, 0.02))
        expected = trial['expected_frames']
        events = [injection('installed', 20.07, 0.02), injection('removed', 80.13, 0.02), admission('p1', 955_998_561, 0.55),
                  decoder('d-listener-1', expected, expected, ended='completed', ended_s=73.63),
                  departure('p1', RESET, 1_030_532_702, 75.63, skips=3, frames_skipped=502), close('p1', 1_030_533_290, 75.63)]
        # attribute_skips pairs the decoder with admission p1 and that peer's counters-bearing departure.
        verdict = nf.trial_verdict(trial, nf.attribute_skips(events) + deficit_samples(20.07, 80.13, 20.0))
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['harness_closes'], 1, verdict)
        self.assertEqual(verdict['departed'], 0)
        # A late reset after a decoder the board cut is still the product's departure.
        cut = [dict(event, frames=expected // 2, exit_code=251, ended='cut') if event['kind'] == 'decoder' else event for event in events]
        cut = [dict(event, at_s=78.63) if event['kind'] == 'departure' else event for event in cut]
        verdict = nf.trial_verdict(trial, cut + deficit_samples(20.07, 80.13, 20.0))
        self.assertEqual(verdict['harness_closes'], 0)
        self.assertFalse(verdict['passed'])

    def test_live_deficit_falls_back_to_attributed_skips_when_metrics_are_starved(self):
        # oct16-interruption-1 deficit-0.5-1: the ifb0 rate limit starved every metrics poll (0 of 96 samples), the
        # listener kept its socket, and its departure line carried skips=7 frames_skipped=1261, attributed to the decoder.
        trial = live_setup(nf.trial_spec('deficit-0.5-1'), RUN4_BEFORE, None, installed_of(self.deficit_events()))
        starved = [event for event in self.deficit_events() if event['kind'] != 'metrics']
        starved = [dict(event, skips=7, frames_skipped=1261) if event['kind'] == 'decoder' else event for event in starved]
        verdict = nf.trial_verdict(trial, starved)
        self.assertTrue(verdict['passed'], verdict)
        self.assertTrue(verdict['metrics_starved'])
        self.assertIn('7 attributed skips', verdict['reason'])
        self.assertIn('unjudged', verdict['reason'])
        none = [dict(event, skips=0, frames_skipped=0) if event['kind'] == 'decoder' else event for event in starved]
        verdict = nf.trial_verdict(trial, none)
        self.assertFalse(verdict['passed'])
        self.assertIn('no skip', verdict['reason'])
        # oct18-interruption-2 deficit-0.8-1: the few samples that beat the rate limit all predate the first skip,
        # so the sampled increase is 0 while the departure line and the attributed decoder say 3.
        sparse = [dict(item, skips=0, frames_skipped=0) for item in deficit_samples(20, 80.05, 8.0)[:3]]
        verdict = nf.trial_verdict(trial, [event for event in starved if event['kind'] != 'metrics'] + sparse)
        self.assertTrue(verdict['passed'], verdict)
        self.assertTrue(verdict['metrics_starved'])
        self.assertIn('7 attributed skips', verdict['reason'])
        unattributed = [dict(event, skips=None, frames_skipped=None) if event['kind'] == 'decoder' else event for event in starved]
        verdict = nf.trial_verdict(trial, unattributed)
        self.assertFalse(verdict['passed'])
        self.assertIn('attributed departure', verdict['reason'])

    def test_live_stall_beyond_ring_depth_expects_checked_frame_timeout_departures_then_fresh_admissions(self):
        trial, events = self.beyond_events()
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual((verdict['beyond_ring_depth'], verdict['departed'], verdict['harness_closes']), (True, 1, 1))
        self.assertIn('frame_timeout', verdict['reason'])
        two, two_events = self.beyond_events(2)
        verdict = nf.trial_verdict(two, two_events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['departed'], 2)
        # One survivor of two is not the expected outcome beyond the ring depth.
        one_short = [event for event in two_events if event.get('peer') != 'p2' or event['kind'] == 'admission']
        self.assertFalse(nf.trial_verdict(two, one_short)['passed'])
        wrong_end = [dict(event, end='media age exceeded') if event['kind'] == 'departure' and event['end'] == 'stopped reading' else event
                     for event in events]
        self.assertFalse(nf.trial_verdict(trial, wrong_end)['passed'])
        self.assertFalse(nf.trial_verdict(trial, [event for event in events if event['kind'] != 'close'])['passed'])
        late_join = [dict(event, device_us=60_000_002) if event.get('peer') == 'f1' and event['kind'] == 'admission' else event for event in events]
        verdict = nf.trial_verdict(trial, late_join)
        self.assertFalse(verdict['passed'])
        self.assertIn('no fresh join line', verdict['reason'])
        no_removal = [event for event in events if not (event['kind'] == 'injection' and event['phase'] == 'removed')]
        self.assertFalse(nf.trial_verdict(trial, no_removal)['passed'])
        # A listener that stayed through a 15 s stall is not what the ring depth predicts either.
        stayed = [metrics(0), injection('installed', 20), injection('removed', 35.1), admission('p1', 1_000_000, 0.5),
                  decoder('l-listener-1', trial['expected_frames'], trial['expected_frames'], ended_s=56.0), reset_after('p1', 56.0), metrics(50, age=100_000)]
        verdict = nf.trial_verdict(trial, stayed)
        self.assertFalse(verdict['passed'])
        self.assertIn('expected 1 frame_timeout', verdict['reason'])
        self.assertTrue(verdict['beyond_ring_depth'])

    def test_live_stall_8_stays_on_the_zero_departure_side(self):
        trial = live_setup(nf.trial_spec('stall-8-1'), RUN4_BEFORE, RUN4_TOLERANCES, None)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 28.1), metrics(29, age=3_000_000), metrics(45, age=100_000),
                  admission('p1', 1_000_000, 0.5), decoder('l-listener-1', expected, expected, ended_s=48.2),
                  departure('p1', RESET, 49_000_000, 48.5, skips=1, frames_skipped=333), metrics(50, age=None)]
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual((verdict['beyond_ring_depth'], verdict['skips']), (False, 1))
        timed_out = events + [departure('p1', 'stopped reading', 48_000_000, 27.0), close('p1', 48_010_000, 27.1), admission('p2', 50_000_000, 29.0)]
        verdict = nf.trial_verdict(trial, timed_out)
        self.assertFalse(verdict['passed'])
        self.assertIn('departed under live policy', verdict['reason'])

    def test_live_stall_without_counters_bounds_discontinuities_by_zero(self):
        trial = live_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(RUN4_STALL_4_1_EVENTS))
        verdict = nf.trial_verdict(trial, RUN4_STALL_4_1_EVENTS + RUN4_STALL_4_1_SAMPLES)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual((verdict['skips'], verdict['skips_source']), (0, 'none'))
        gapped = [dict(event, discontinuities=1) if event['kind'] == 'decoder' else event for event in RUN4_STALL_4_1_EVENTS]
        self.assertFalse(nf.trial_verdict(trial, gapped + RUN4_STALL_4_1_SAMPLES)['passed'])
        # Counters only in the metrics samples bound the sum for every decoder.
        counted = [dict(item, skips=1, frames_skipped=166) for item in RUN4_STALL_4_1_SAMPLES]
        verdict = nf.trial_verdict(trial, gapped + counted)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['skips_source'], 'metrics')

    def test_live_two_listener_stall_judges_each_decoder_against_its_own_counters(self):
        trial = live_setup(nf.trial_spec('stall-2.5-2'), RUN4_BEFORE, RUN4_TOLERANCES, None)
        expected = trial['expected_frames']
        events = [metrics(0), injection('installed', 20), injection('removed', 22.6), metrics(23, age=1_000_000), metrics(40, age=100_000),
                  admission('p1', 1_000_000, 0.5), admission('p2', 1_100_000, 0.5),
                  decoder('l-listener-1', expected, expected, discontinuities=1, ended_s=43.1), decoder('l-listener-2', expected, expected, ended_s=43.1),
                  departure('p1', RESET, 44_000_000, 43.4, skips=1, frames_skipped=100), departure('p2', RESET, 44_100_000, 43.5, skips=0, frames_skipped=0),
                  metrics(45, age=None)]
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual([item['skips'] for item in verdict['decoders']], [1, 0])
        swapped = [dict(event, skips=0, frames_skipped=0) if event.get('peer') == 'p1' and event['kind'] == 'departure' else
                   dict(event, skips=1, frames_skipped=100) if event.get('peer') == 'p2' and event['kind'] == 'departure' else event for event in events]
        self.assertFalse(nf.trial_verdict(trial, swapped)['passed'])

    def test_exact_policy_keeps_the_retained_verdicts(self):
        trial = exact_setup(nf.trial_spec('stall-2.5-1'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_2_5_1_EVENTS))
        self.assertTrue(nf.trial_verdict(trial, RUN3_STALL_2_5_1_EVENTS)['passed'])
        live = live_setup(nf.trial_spec('stall-2.5-1'), RUN4_BEFORE, RUN3_TOLERANCES, installed_of(RUN3_STALL_2_5_1_EVENTS))
        self.assertFalse(nf.trial_verdict(live, RUN3_STALL_2_5_1_EVENTS)['passed'])
        exact = exact_setup(nf.trial_spec('stall-4-1'), RUN4_BEFORE, RUN4_TOLERANCES, installed_of(LIVE_STALL_4_1_EVENTS))
        verdict = nf.trial_verdict(exact, LIVE_STALL_4_1_EVENTS + RUN4_STALL_4_1_SAMPLES)
        self.assertEqual(verdict['reason'], 'continuous')
        self.assertNotIn('skips', verdict)

    def deficit_events(self, period_s=8.0, listener_end_s=92.0):
        installed, removed = injection('installed', 20, 0.02), injection('removed', 80.05, 0.02)
        expected = round(nf.trial_spec('deficit-0.5-1')['seconds'] * 48000)
        events = [installed, removed, admission('p1', 1_000_000, 0.5),
                  decoder('d-listener-1', expected, expected, ended_s=listener_end_s), departure('p1', RESET, 95_000_000, listener_end_s + 0.3)]
        return events + deficit_samples(20, 80.05, period_s)

    def test_live_deficit_passes_when_the_listener_stays_and_skips_steadily(self):
        events = self.deficit_events()
        trial = live_setup(nf.trial_spec('deficit-0.5-1'), RUN4_BEFORE, None, installed_of(events))
        verdict = nf.trial_verdict(trial, events)
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual(verdict['skips'], 7)
        self.assertAlmostEqual(verdict['predicted_skip_period_s'], 8.0)
        self.assertLessEqual(verdict['max_skip_gap_s'], 9.0)
        self.assertEqual(verdict['departed'], 0)
        self.assertIn('7 skips', verdict['reason'])

    def test_live_deficit_fails_on_departure_cut_or_unsteady_skips(self):
        trial = live_setup(nf.trial_spec('deficit-0.5-1'), RUN4_BEFORE, None, installed_of(self.deficit_events()))
        departed = self.deficit_events() + [departure('p1', 'media age exceeded', 60_000_000, 50.0), close('p1', 60_010_000, 50.1)]
        verdict = nf.trial_verdict(trial, departed)
        self.assertFalse(verdict['passed'])
        self.assertIn('departed', verdict['reason'])
        cut = [dict(event, frames=1_000_000, exit_code=251, ended_s=60.0, ended='cut') if event['kind'] == 'decoder' else event for event in self.deficit_events()]
        self.assertFalse(nf.trial_verdict(trial, cut)['passed'])
        # A complete decoder may finish before the deficit is lifted (ffmpeg -t counts audio seconds and skips
        # shorten the wall time); only a cut decoder ending early is a failure.
        early_complete = self.deficit_events(listener_end_s=70.0)
        self.assertTrue(nf.trial_verdict(trial, early_complete)['passed'])
        early_cut = [dict(event, frames=1_000_000, exit_code=251, ended='cut') if event['kind'] == 'decoder' else event
                     for event in self.deficit_events(listener_end_s=70.0)]
        verdict = nf.trial_verdict(trial, early_cut)
        self.assertFalse(verdict['passed'])
        # The cut decoder fails on its own exit code first; the early-end rule is the backstop.
        self.assertTrue('exit code 251' in verdict['reason'] or 'before the injection was removed' in verdict['reason'], verdict)
        none = [event for event in self.deficit_events() if event['kind'] != 'metrics'] + [dict(item, skips=0, frames_skipped=0) for item in deficit_samples(20, 80.05, 8.0)]
        verdict = nf.trial_verdict(trial, none)
        self.assertFalse(verdict['passed'])
        self.assertIn('no skip', verdict['reason'])
        unreported = [event for event in self.deficit_events() if event['kind'] != 'metrics'] + [sample(at, 100_000) for at in range(18, 100)]
        self.assertFalse(nf.trial_verdict(trial, unreported)['passed'])
        # A skip gap beyond twice the predicted period is not a steady rate.
        sparse = self.deficit_events(period_s=17.0)
        verdict = nf.trial_verdict(trial, sparse)
        self.assertFalse(verdict['passed'])
        self.assertIn('steady', verdict['reason'])
        # The counter must never fall during the trial.
        falling = [dict(item, skips=0) if item['kind'] == 'metrics' and item['at_s'] == 60 else item for item in self.deficit_events()]
        self.assertFalse(nf.trial_verdict(trial, falling)['passed'])
        self.assertFalse(nf.trial_verdict(trial, [event for event in self.deficit_events() if event['kind'] != 'injection'])['passed'])

    def test_null_and_delayed_reader_are_policy_blind(self):
        for policy in nf.POLICIES:
            trial = nf.trial_setup(nf.trial_spec('delayed-reader-1'), RUN4_BEFORE, RUN4_TOLERANCES, None, policy)
            self.assertTrue(nf.trial_verdict(trial, RUN4_DELAYED_READER_EVENTS)['passed'], policy)
            null = nf.trial_setup(nf.trial_spec('null-1'), BEFORE, None, None, policy)
            good = [metrics(0), decoder('listener-1', 48000 * 60, 48000 * 60, ended_s=60.2), reset_after('p1', 60.2), metrics(61)]
            self.assertTrue(nf.trial_verdict(null, good)['passed'], policy)
            self.assertFalse(nf.trial_verdict(null, good + [departure('p3', RESET, 50_000_000, 30)])['passed'], policy)


if __name__ == '__main__':
    unittest.main()
