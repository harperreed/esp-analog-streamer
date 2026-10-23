# ABOUTME: Checks saved benchmark acquisition acceptance and rejection.
# ABOUTME: Uses text receipts only; never opens hardware.
import unittest
from parse_receipt import parse


def good():
    lines = ['current-mp3-cost: v=1 event=boot fixtures=4 pcm_psram=false output_psram=false core=1 priority=8 stack_bytes=8192']
    for kind in ('zero', 'dc', 'tones', 'noise'):
        for phase, count in (('warmup', 1), ('measured', 128)):
            lines.append(f'current-mp3-cost: v=1 event=phase fixture={kind} phase={phase} attempted={count} succeeded={count} failed=0 frames={count*1152} bytes={count*960} sum_us={count*20000} max_us=20000 late=0 failed_sum_us=0 failed_max_us=0 wall_us={count*22000} runtime_us={count*20000} runtime_valid=true runtime_wrapped=false saturated=false acquisition_stack_bytes=4000 checksum=123')
        lines.append(f'current-mp3-cost: v=1 event=fixture_end fixture={kind} attempted=129 status=ok')
    lines.append('current-mp3-cost: v=1 event=complete expected=4 finished=4 status=ok')
    return '\n'.join(lines)+'\n'

class ReceiptTests(unittest.TestCase):
    def test_success(self): self.assertEqual(len(parse(good())['phases']), 8)
    def test_initial_rom_boot(self): self.assertEqual(parse("rst:0x1\n"+good())["status"], "ok")
    def test_output_cap(self):
        with self.assertRaises(ValueError): parse(" "*(2*1024*1024)+good())
    def test_late_below_budget(self):
        with self.assertRaises(ValueError): parse(good().replace('max_us=20000 late=0', 'max_us=20000 late=1', 1))
    def test_late_missing_above_budget(self):
        with self.assertRaises(ValueError): parse(good().replace('sum_us=20000 max_us=20000 late=0', 'sum_us=25000 max_us=25000 late=0', 1).replace('wall_us=22000', 'wall_us=27000', 1))
    def test_valid_late_call(self):
        raw = good().replace('sum_us=20000 max_us=20000 late=0', 'sum_us=25000 max_us=25000 late=1', 1).replace('wall_us=22000', 'wall_us=27000', 1)
        self.assertEqual(parse(raw)['phases'][0]['mean_us'], 25000)
    def test_exact_budget_not_late(self):
        raw = good().replace('sum_us=20000 max_us=20000 late=0', 'sum_us=24000 max_us=24000 late=0', 1).replace('wall_us=22000', 'wall_us=26000', 1)
        self.assertEqual(parse(raw)['status'], 'ok')
    def test_late_sum_impossible(self):
        with self.assertRaises(ValueError): parse(good().replace('sum_us=2560000 max_us=20000 late=0', 'sum_us=2560000 max_us=30000 late=128', 1))
    def test_runtime_interval_wrap_limit(self):
        with self.assertRaises(ValueError): parse(good().replace('wall_us=22000', 'wall_us=4294967295', 1))
    def test_worker_configuration(self):
        for old, replacement in [('core=1', 'core=0'), ('priority=8', 'priority=7'), ('stack_bytes=8192', 'stack_bytes=16384')]:
            with self.subTest(field=old), self.assertRaises(ValueError): parse(good().replace(old, replacement, 1))
    def test_pointer_classification_schema(self):
        for key in ('pcm_psram', 'output_psram'):
            with self.subTest(field=key), self.assertRaises(ValueError): parse(good().replace(key+'=false', key+'=unknown', 1))
    def test_boot_missing_configuration(self):
        with self.assertRaises(ValueError): parse(good().replace(' core=1', '', 1))
    def test_missing_end(self):
        with self.assertRaises(ValueError): parse(good().replace('current-mp3-cost: v=1 event=fixture_end fixture=zero attempted=129 status=ok\n',''))
    def test_duplicate_end(self):
        with self.assertRaises(ValueError): parse(good().replace('event=complete','event=fixture_end fixture=noise attempted=129 status=ok\ncurrent-mp3-cost: v=1 event=complete'))
    def test_failed_encode(self):
        with self.assertRaises(ValueError): parse(good().replace('succeeded=128 failed=0','succeeded=127 failed=1',1))
    def test_partial_line(self):
        with self.assertRaises(ValueError): parse(good().rstrip())
    def test_second_boot(self):
        with self.assertRaises(ValueError): parse(good()+good())
    def test_wrong_bytes(self):
        with self.assertRaises(ValueError): parse(good().replace('bytes=960 ', 'bytes=959 ',1))
    def test_invalid_runtime(self):
        with self.assertRaises(ValueError): parse(good().replace('runtime_valid=true','runtime_valid=false',1))
    def test_reset(self):
        with self.assertRaises(ValueError): parse(good().replace('event=complete','rst:0x1\ncurrent-mp3-cost: v=1 event=complete'))
if __name__ == '__main__': unittest.main()
