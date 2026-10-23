# ABOUTME: Checks saved benchmark acquisition acceptance and rejection.
# ABOUTME: Uses text receipts only; never opens hardware.
import unittest
from functools import lru_cache
from parse_receipt import parse


@lru_cache(maxsize=None)
def reference(count, state=0x12345678, repeated=False, byteorder='little'):
    checksum = 2166136261
    for _ in range(count):
        if repeated: state = 0x12345678
        for _ in range(2304):
            state = (state ^ (state << 13)) & 0xffffffff
            state = (state ^ (state >> 17)) & 0xffffffff
            state = (state ^ (state << 5)) & 0xffffffff
            sample = state & 0xffff
            if sample >= 0x8000: sample -= 0x10000
            for byte in sample.to_bytes(2, byteorder, signed=True):
                checksum = ((checksum ^ byte) * 16777619) & 0xffffffff
    return checksum, state


def good():
    lines = ['mp3-input-repetition: v=1 event=boot conditions=3 pcm_psram=false output_psram=false core=1 priority=8 stack_bytes=8192']
    for kind in ('repeated_a', 'fresh', 'repeated_b'):
        state = 0x12345678
        for phase, count in (('warmup', 1), ('measured', 128)):
            checksum, state = reference(count, state, kind != 'fresh')
            lines.append(f'mp3-input-repetition: v=1 event=phase condition={kind} phase={phase} attempted={count} succeeded={count} failed=0 frames={count*1152} bytes={count*960} sum_us={count*20000} max_us=20000 late=0 failed_sum_us=0 failed_max_us=0 wall_us={count*22000} runtime_us={count*20000} runtime_valid=true runtime_wrapped=false saturated=false acquisition_stack_bytes=4000 checksum=123 input_checksum={checksum} final_prng_state={state}')
        lines.append(f'mp3-input-repetition: v=1 event=condition_end condition={kind} attempted=129 status=ok')
    lines.append('mp3-input-repetition: v=1 event=complete expected=3 finished=3 status=ok')
    return '\n'.join(lines)+'\n'

class ReceiptTests(unittest.TestCase):
    def test_success(self): self.assertEqual(len(parse(good())['phases']), 6)
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
        with self.assertRaises(ValueError): parse(good().replace('mp3-input-repetition: v=1 event=condition_end condition=repeated_a attempted=129 status=ok\n',''))
    def test_duplicate_end(self):
        with self.assertRaises(ValueError): parse(good().replace('event=complete','event=condition_end condition=repeated_b attempted=129 status=ok\nmp3-input-repetition: v=1 event=complete'))
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
        with self.assertRaises(ValueError): parse(good().replace('event=complete','rst:0x1\nmp3-input-repetition: v=1 event=complete'))
    def test_input_identity_corruption(self):
        for key in ('input_checksum', 'final_prng_state'):
            raw = good()
            import re
            raw = re.sub(key+r'=\d+', key+'=0', raw, count=1)
            with self.subTest(key=key), self.assertRaises(ValueError): parse(raw)
    def test_fresh_must_continue_after_warmup(self):
        lines = good().splitlines()
        checksum, state = reference(128)
        import re
        lines[5] = re.sub(r'input_checksum=\d+ final_prng_state=\d+', f'input_checksum={checksum} final_prng_state={state}', lines[5])
        with self.assertRaises(ValueError): parse('\n'.join(lines)+'\n')
    def test_wrong_sample_byte_order(self):
        checksum, state = reference(1, byteorder='big')
        raw = good()
        import re
        raw = re.sub(r'input_checksum=\d+', f'input_checksum={checksum}', raw, count=1)
        with self.assertRaises(ValueError): parse(raw)
    def test_old_prefix(self):
        with self.assertRaises(ValueError): parse(good().replace('mp3-input-repetition:', 'current-mp3-cost:'))
    def test_condition_order(self):
        with self.assertRaises(ValueError): parse(good().replace('condition=repeated_a', 'condition=fresh'))
    def test_missing_or_duplicate_phase(self):
        lines = good().splitlines()
        for replacement in (lines[:2]+lines[3:], lines[:2]+[lines[1]]+lines[2:]):
            with self.subTest(records=len(replacement)), self.assertRaises(ValueError):
                parse('\n'.join(replacement)+'\n')
    def test_subphase_order(self):
        lines = good().splitlines()
        lines[1], lines[2] = lines[2], lines[1]
        with self.assertRaises(ValueError): parse('\n'.join(lines)+'\n')
    def test_repeated_inputs_match_and_fresh_continues(self):
        rows = parse(good())['phases']
        for index in (0, 1):
            for key in ('input_checksum', 'final_prng_state'):
                self.assertEqual(rows[index][key], rows[index+4][key])
        self.assertEqual(reference(1), (3834239343, 3670521071))
        self.assertEqual(reference(128, 3670521071), (111076412, 638221377))
        self.assertNotEqual(rows[1]['input_checksum'], rows[3]['input_checksum'])
    def test_saturation_and_failure_markers(self):
        for old, replacement in (('saturated=false', 'saturated=true'), ('status=ok', 'status=failed'), ('failed_sum_us=0', 'failed_sum_us=1')):
            with self.subTest(field=old), self.assertRaises(ValueError):
                parse(good().replace(old, replacement, 1))
if __name__ == '__main__': unittest.main()
