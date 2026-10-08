# ABOUTME: Tests bounded post-failure log collection with real processes and temporary files.
# ABOUTME: Checks that decoder exit leaves UART alive while budgets and collector exit stop observation.

import argparse
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

MODULE = Path(__file__).with_name('failure_observation.py')


class ObservationTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(MODULE.is_file(), 'failure observation helper is missing')
        spec = importlib.util.spec_from_file_location('failure_observation', MODULE)
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.uart = Path(self.directory.name) / 'uart.log'
        self.events = []

    def child(self, code):
        process = subprocess.Popen([sys.executable, '-u', '-c', code, str(self.uart)],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.addCleanup(self.stop, process)
        return process

    @staticmethod
    def stop(process):
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=3)

    def collector(self):
        process = self.child("import pathlib,sys,time\np=pathlib.Path(sys.argv[1])\n"
                             "with p.open('ab', buffering=0) as f:\n"
                             " while True:\n  f.write(b'capture evidence\\n'); time.sleep(.02)\n")
        deadline = time.monotonic() + 2
        while not self.uart.exists() and time.monotonic() < deadline:
            time.sleep(.01)
        self.assertTrue(self.uart.exists(), 'real collector did not start')
        return process

    def record(self, kind, **fields):
        self.events.append(dict(kind=kind, **fields))

    def budget(self):
        if self.uart.stat().st_size > 1024 * 1024:
            raise RuntimeError('output budget exceeded')

    def test_primary_decoder_exit_does_not_end_uart_observation(self):
        # Stopping when any child exits would destroy the post-failure UART evidence.
        collector = self.collector()
        decoder = self.child('raise SystemExit(7)')
        self.assertEqual(decoder.wait(timeout=2), 7)
        before = self.uart.stat().st_size
        result = self.module.observe_failure(.25, collector, self.budget, self.record)
        self.assertEqual(result['stop_reason'], 'deadline')
        self.assertIsNone(result['secondary_error'])
        self.assertGreaterEqual(result['observed_seconds'], .24)
        self.assertLess(result['observed_seconds'], 1)
        self.assertGreater(self.uart.stat().st_size, before)
        self.assertIsNone(collector.poll())
        self.assertEqual([event['kind'] for event in self.events], ['failure_observation_start', 'failure_observation_end'])

    def test_collector_exit_ends_observation_with_secondary_failure(self):
        # UART loss must be reported separately rather than waiting on a dead collector.
        collector = self.child('import time; time.sleep(.05); raise SystemExit(4)')
        started = time.monotonic()
        result = self.module.observe_failure(2, collector, lambda: None, self.record)
        self.assertEqual(result['stop_reason'], 'collector_exit')
        self.assertIn('4', result['secondary_error'])
        self.assertLess(time.monotonic() - started, 1)

    def test_budget_growth_ends_observation_with_secondary_failure(self):
        # Grace must keep enforcing the existing checked output budget.
        collector = self.collector()
        limit = self.uart.stat().st_size + 30
        def budget():
            if self.uart.stat().st_size > limit:
                raise RuntimeError('output budget exceeded')
        result = self.module.observe_failure(2, collector, budget, self.record)
        self.assertEqual(result['stop_reason'], 'output_budget')
        self.assertEqual(result['secondary_error'], 'output budget exceeded')
        self.assertLess(result['observed_seconds'], 1)

    def test_zero_duration_does_not_observe_or_touch_collector(self):
        # Default zero grace must leave the prior immediate-cleanup behavior intact.
        collector = self.collector()
        result = self.module.observe_failure(0, collector, self.budget, self.record)
        self.assertIsNone(result)
        self.assertEqual(self.events, [])

    def test_operator_interrupt_during_observation_ends_without_hiding_failure(self):
        # Interrupting grace must reach normal cleanup promptly, with separate metadata.
        collector = self.collector()
        def interrupted_budget():
            self.budget()
            raise KeyboardInterrupt
        result = self.module.observe_failure(2, collector, interrupted_budget, self.record)
        self.assertEqual(result['stop_reason'], 'interrupted')
        self.assertEqual(result['secondary_error'], 'interrupted during failure observation')
        self.assertLess(result['observed_seconds'], 1)

    def test_missing_uart_collector_reports_why_observation_cannot_run(self):
        # A failure before UART setup must not cause an unbound-variable error.
        result = self.module.observe_failure(2, None, lambda: None, self.record)
        self.assertEqual(result['stop_reason'], 'collector_exit')
        self.assertEqual(result['secondary_error'], 'UART collector unavailable')

    def test_stream_soak_rejects_invalid_grace_before_any_collector(self):
        # Omitting the helper's parser type would permit an unbounded diagnostic grace.
        script = Path(__file__).with_name('stream-soak')
        for value in ('-1', '120.1', 'nan', 'inf', 'abc'):
            with self.subTest(value=value):
                result = subprocess.run([str(script), '--failure-observe-seconds', value],
                                        capture_output=True, text=True, timeout=3)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, '')

    def test_argument_rejects_nonfinite_and_out_of_range_duration(self):
        for value in ('-1', '120.1', 'nan', 'inf', 'abc'):
            with self.subTest(value=value):
                with self.assertRaises(argparse.ArgumentTypeError):
                    self.module.observation_seconds(value)
        self.assertEqual(self.module.observation_seconds('0'), 0)
        self.assertEqual(self.module.observation_seconds('120'), 120)


if __name__ == '__main__':
    unittest.main()
