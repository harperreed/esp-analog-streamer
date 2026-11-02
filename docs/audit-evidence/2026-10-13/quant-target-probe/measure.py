# ABOUTME: Validates every device benchmark row against independently decoded host receipts.
# ABOUTME: Summarizes paired timing and bounded storage without granting production acceptance.
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import re
import statistics
import sys


def parser_fixture(host_rows):
    """Synthetic UART for parser tests only; never device measurement evidence."""
    expected = {(row['kind'], row['policy'], row['phase']): row for row in host_rows}
    setup = ('input_bytes=32768 chunk_bytes=1920 rng_state_bytes=8 input_psram=true '
             'chunk_psram=false output_psram=true output_capacity=16399 measured_blocks=16 '
             'orders=6 header_number=0 timer_calls_per_block=78 empty_timer_pairs=256 '
             'empty_timer_pair_sum_us=128 empty_timer_pair_max_us=1 '
             'redundant_requantization=true approximate_prng=true')
    lines = ['quant-cost: setup ' + setup]
    orders = [('trunc', 'round', 'tpdf'), ('trunc', 'tpdf', 'round'),
              ('round', 'trunc', 'tpdf'), ('round', 'tpdf', 'trunc'),
              ('tpdf', 'trunc', 'round'), ('tpdf', 'round', 'trunc')]
    for kind in ['silence', 'quarter_dc', 'triangle_fade', 'noise24', 'extrema']:
        for order, policies in enumerate(orders):
            for policy in policies:
                for phase in ['warmup', 'measured']:
                    want = expected[kind, policy, phase]
                    blocks = want['blocks']
                    row = dict(kind=kind, order=order, policy=policy, phase=phase,
                               **{key: want[key] for key in ['blocks', 'source_hash', 'bytes',
                                  'frame_hash', 'pcm_hash', 'rng_left', 'rng_right']},
                               max_frame_bytes=want['largest'], quant_us=10 * blocks,
                               encoder_us=100 * blocks, block_wall_us=130 * blocks,
                               max_quant_us=10, max_encoder_us=100, max_block_wall_us=130,
                               wall_us=10130 * blocks, task_runtime_us=130 * blocks,
                               runtime_valid='true', stack_free_bytes=2048,
                               heap_internal_before=50000, heap_internal_after=50000,
                               heap_external_before=1000000, heap_external_after=1000000,
                               output_ptr='0x3ffb1000', output_capacity=16399,
                               output_storage_reused='true', host_receipt_match='true')
                    lines.append('quant-cost: ' + ' '.join(f'{key}={value}' for key, value in row.items()))
    lines.append('quant-cost: complete phases=180 assertions_passed=true capture=false wifi=false audio=false')
    return '\n'.join(lines) + '\n'


def parser_tests(host_rows):
    """Use real host receipts with synthetic rows to test rejection behavior."""
    log = parser_fixture(host_rows)
    assert validate(log, host_rows)['status'] == 'pass'
    controls = {}

    def rejected(name, bad_log=log, bad_host=None):
        try:
            validate(bad_log, host_rows if bad_host is None else bad_host)
        except (AssertionError, KeyError, ValueError):
            controls[name] = 'rejected'
        else:
            controls[name] = 'ACCEPTED'

    for name in ['max_quant_us', 'max_encoder_us', 'max_block_wall_us']:
        for bad_value in [0, 1, 999999]:
            rejected(f'{name}_{bad_value}', re.sub(fr'{name}=\d+', f'{name}={bad_value}', log, count=1))
            lines = log.splitlines()
            lines[2] = re.sub(fr'{name}=\d+', f'{name}={bad_value}', lines[2])
            rejected(f'measured_{name}_{bad_value}', '\n'.join(lines))
    # A measured maximum may exceed its average while remaining below the sum.
    lines = log.splitlines()
    lines[2] = lines[2].replace('max_quant_us=10', 'max_quant_us=20')
    assert validate('\n'.join(lines), host_rows)['status'] == 'pass'
    for key, bad_value in [('input_bytes', 1), ('chunk_bytes', 1), ('orders', 1),
                           ('header_number', 1), ('timer_calls_per_block', 76),
                           ('rng_state_bytes', 1), ('measured_blocks', 1),
                           ('redundant_requantization', 'false'), ('approximate_prng', 'false')]:
        rejected(f'setup_{key}', log.replace(f'{key}={fields(log.splitlines()[0].split("setup ")[1])[key]}', f'{key}={bad_value}', 1))
    rejected('capacity_below_bound', log.replace('output_capacity=16399', 'output_capacity=16398'))
    rejected('capacity_setup_mismatch', log.replace('output_capacity=16399', 'output_capacity=16400', 1))
    rejected('duplicate_field', log.replace('max_block_wall_us=130', 'max_block_wall_us=130 max_block_wall_us=130', 1))
    rejected('malformed_field', log.replace('max_block_wall_us=130', 'max_block_wall_us=130=extra', 1))
    rejected('unpaired_field', log.replace('max_block_wall_us=130', 'max_block_wall_us=130 junk', 1))
    rejected('empty_key', log.replace('max_block_wall_us=130', 'max_block_wall_us=130 =oops', 1))
    rejected('empty_value', log.replace('max_block_wall_us=130', 'max_block_wall_us=', 1))
    rejected('missing_completion', log.replace('quant-cost: complete', 'removed: complete'))
    rejected('wrong_checksum', re.sub(r'pcm_hash=\d+', 'pcm_hash=0', log, count=1))
    for flag in ['capture', 'wifi', 'audio']:
        rejected(f'completion_{flag}', log.replace(f'{flag}=false', f'{flag}=true'))
    rejected('duplicate_host', bad_host=[*host_rows, host_rows[0]])
    rejected('missing_host', bad_host=host_rows[1:])
    bad_host = [dict(row) for row in host_rows]
    bad_host[0]['kind'] = 'unknown_fixture'
    rejected('wrong_host_identity', bad_host=bad_host)
    rejected('duplicate_host_identity_same_count', bad_host=[host_rows[0], host_rows[0], *host_rows[2:]])
    for flag in ['independent_quantizer_exact', 'sample_exact']:
        bad_host = [dict(row) for row in host_rows]
        bad_host[0][flag] = False
        rejected(f'host_{flag}', bad_host=bad_host)
    accepted = [name for name, status in controls.items() if status != 'rejected']
    assert not accepted, f'parser negative controls accepted: {accepted}'
    return {'status': 'pass', 'scope': 'parser tests only; synthetic UART is not device evidence',
            'valid_rows': 180, 'negative_controls': controls}


def fields(line):
    result = {}
    for word in line.split():
        assert re.fullmatch(r'[a-z][a-z0-9_]*=[^=\s]+', word), ('malformed field', word)
        key, value = word.split('=')
        assert key not in result, ('duplicate field', key)
        result[key] = value
    return result


def validate(log, host_rows):
    clean = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', log)
    assert clean.count('quant-cost: complete phases=180 assertions_passed=true') == 1
    completed = [fields(line.split('quant-cost: complete ', 1)[1]) for line in clean.splitlines() if 'quant-cost: complete ' in line]
    assert len(completed) == 1
    assert completed[0] == dict(phases='180', assertions_passed='true', capture='false', wifi='false', audio='false')
    fatal = [line for line in clean.splitlines() if re.search(r'Guru Meditation|assertion .*failed|panicked at|abort\(\)|Task watchdog', line)]
    assert not fatal, fatal
    setups = [fields(line.split('quant-cost: setup ', 1)[1]) for line in clean.splitlines() if 'quant-cost: setup ' in line]
    assert len(setups) == 1
    setup = setups[0]
    assert setup['input_psram'] == 'true' and setup['chunk_psram'] == 'false'
    assert setup['output_psram'] in ['true', 'false']
    for key, expected in [('input_bytes', 32768), ('chunk_bytes', 1920),
                          ('rng_state_bytes', 8), ('measured_blocks', 16), ('orders', 6),
                          ('header_number', 0), ('timer_calls_per_block', 78),
                          ('empty_timer_pairs', 256)]:
        assert int(setup[key]) == expected, ('setup', key)
    for key in ['redundant_requantization', 'approximate_prng']:
        assert setup[key] == 'true', ('setup', key)
    assert int(setup['output_capacity']) >= 16399
    rows = [fields(line.split('quant-cost: ', 1)[1]) for line in clean.splitlines() if 'quant-cost: kind=' in line]
    kinds = ['silence', 'quarter_dc', 'triangle_fade', 'noise24', 'extrema']
    host_identities = set(itertools.product(kinds, ['trunc', 'round', 'tpdf'], ['warmup', 'measured']))
    assert len(host_rows) == 30
    expected = {(row['kind'], row['policy'], row['phase']): row for row in host_rows}
    assert set(expected) == host_identities
    for row in host_rows:
        assert row['independent_quantizer_exact'] is True
        assert row['sample_exact'] is True
        assert row['blocks'] == (1 if row['phase'] == 'warmup' else 16)
        assert row['samples'] == row['blocks'] * 4096 * 2
    identities = [(kind, str(order), policy, phase) for kind in kinds
                  for order, policies in enumerate(itertools.permutations(['trunc', 'round', 'tpdf']))
                  for policy in policies for phase in ['warmup', 'measured']]
    assert [(row['kind'], row['order'], row['policy'], row['phase']) for row in rows] == identities
    for row in rows:
        want = expected[row['kind'], row['policy'], row['phase']]
        for key in ['blocks', 'source_hash', 'bytes', 'frame_hash', 'pcm_hash', 'rng_left', 'rng_right']:
            assert int(row[key]) == want[key], (row['kind'], row['policy'], key)
        assert int(row['max_frame_bytes']) == want['largest']
        for key in ['runtime_valid', 'output_storage_reused', 'host_receipt_match']:
            assert row[key] == 'true', key
        assert int(row['heap_internal_before']) == int(row['heap_internal_after'])
        assert int(row['heap_external_before']) == int(row['heap_external_after'])
        for key in ['quant_us', 'encoder_us', 'block_wall_us', 'task_runtime_us']:
            assert int(row[key]) > 0
        blocks = int(row['blocks'])
        for total_key, max_key in [('quant_us', 'max_quant_us'), ('encoder_us', 'max_encoder_us'),
                                   ('block_wall_us', 'max_block_wall_us')]:
            total, maximum = int(row[total_key]), int(row[max_key])
            assert 0 < total <= maximum * blocks and maximum <= total, (total_key, max_key)
        assert int(row['quant_us']) + int(row['encoder_us']) <= int(row['block_wall_us'])
        assert int(row['stack_free_bytes']) >= 512
        assert int(row['output_capacity']) == int(setup['output_capacity'])
    assert len({row['output_ptr'] for row in rows}) == 1
    assert len({row['output_capacity'] for row in rows}) == 1
    results = []
    for kind in kinds:
        for policy in ['trunc', 'round', 'tpdf']:
            selected = [row for row in rows if row['kind'] == kind and row['policy'] == policy and row['phase'] == 'measured']
            assert len(selected) == 6
            item = {'kind': kind, 'policy': policy, 'measured_phases': 6,
                    'blocks_per_phase': 16, 'bytes_per_block': int(selected[0]['bytes']) / 16}
            for key in ['quant_us', 'encoder_us', 'block_wall_us', 'task_runtime_us']:
                values = [int(row[key]) / 16 for row in selected]
                item[key + '_per_block'] = {'median': statistics.median(values), 'min': min(values), 'max': max(values)}
            item['max_block_wall_us'] = max(int(row['max_block_wall_us']) for row in selected)
            item['isolated_max_below_audio_block_duration'] = item['max_block_wall_us'] < 4096 * 1e6 / 48000
            results.append(item)
    return {'status': 'pass', 'rows': rows, 'setup': setup, 'summary': results,
            'warmup_rows': 90, 'measured_rows': 90, 'minimum_stack_free_bytes': min(int(row['stack_free_bytes']) for row in rows),
            'host_receipts_match': True, 'phase_heap_endpoints_stable': True,
            'warning_or_error_lines': [line for line in clean.splitlines() if re.search(r'\b[WE] \(\d+\)', line)],
            'scope': 'isolated preprocessing plus unchanged encoder; no capture/network/audio-load or policy acceptance'}


def main():
    if sys.argv[1:2] == ['--parser-tests']:
        parser = argparse.ArgumentParser(description='Parser tests only; synthetic UART is not device evidence.')
        parser.add_argument('--parser-tests', type=Path, required=True, metavar='HOST_RESULTS_JSON')
        args = parser.parse_args()
        print(json.dumps(parser_tests(json.loads(args.parser_tests.read_text())), indent=2))
        return
    parser = argparse.ArgumentParser()
    parser.add_argument('uart', type=Path)
    parser.add_argument('host', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    log = args.uart.read_text()
    host = json.loads(args.host.read_text())
    result = validate(log, host)
    controls = {}
    for name, bad in [('missing_completion', log.replace('quant-cost: complete', 'removed: complete')),
                      ('wrong_checksum', re.sub(r'pcm_hash=\d+', 'pcm_hash=0', log, count=1))]:
        try:
            validate(bad, host)
        except AssertionError:
            controls[name] = 'rejected'
        else:
            raise AssertionError(f'negative control accepted: {name}')
    result['negative_controls'] = controls
    result['uart_sha256'] = hashlib.sha256(args.uart.read_bytes()).hexdigest()
    result['host_receipt_sha256'] = hashlib.sha256(args.host.read_bytes()).hexdigest()
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: result[key] for key in ['status', 'warmup_rows', 'measured_rows', 'minimum_stack_free_bytes', 'negative_controls']}))


if __name__ == '__main__':
    main()
