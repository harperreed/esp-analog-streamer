# ABOUTME: Validates complete bounded current MP3 benchmark UART receipts.
# ABOUTME: Rejects incomplete, repeated, failed or inconsistent acquisitions.
import argparse
import json
from pathlib import Path

PREFIX = 'current-mp3-cost: '
KINDS = ('zero', 'dc', 'tones', 'noise')

def parse(raw):
    if len(raw.encode('utf-8')) > 2*1024*1024: raise ValueError('UART output cap exceeded')
    if not raw.endswith('\n'): raise ValueError('partial final UART line')
    records = []
    for line in raw.splitlines():
        markers = ('rst:', 'panic', 'watchdog', 'abort()') if records else ('panic', 'watchdog', 'abort()')
        if any(marker in line.lower() for marker in markers):
            raise ValueError('reset/panic/watchdog evidence')
        if PREFIX not in line: continue
        fields = line.split(PREFIX, 1)[1].split()
        row = {}
        for field in fields:
            key, value = field.split('=', 1)
            if key in row: raise ValueError('duplicate field')
            row[key] = value
        if row.get('v') != '1': raise ValueError('unknown schema')
        records.append(row)
    expected = [('boot', None, None)]
    for kind in KINDS:
        expected.extend([('phase', kind, 'warmup'), ('phase', kind, 'measured'), ('fixture_end', kind, None)])
    expected.append(('complete', None, None))
    if len(records) != len(expected): raise ValueError('missing or duplicate records')
    phases = []
    for row, (event, kind, phase) in zip(records, expected):
        if (row.get('event'), row.get('fixture'), row.get('phase')) != (event, kind, phase):
            raise ValueError('unexpected event order')
        if event == 'boot':
            for key, value in {'fixtures':'4', 'core':'1', 'priority':'8', 'stack_bytes':'8192'}.items():
                if row.get(key) != value: raise ValueError('wrong boot configuration: '+key)
            for key in ('pcm_psram', 'output_psram'):
                if row.get(key) not in ('true', 'false'): raise ValueError('invalid pointer classification: '+key)
        elif event == 'phase':
            count = 1 if phase == 'warmup' else 128
            for key, value in {'attempted':count,'succeeded':count,'failed':0,'frames':count*1152,'bytes':count*960,'failed_sum_us':0,'failed_max_us':0}.items():
                if int(row[key]) != value: raise ValueError('failed accounting: '+key)
            if row['runtime_valid'] != 'true' or row['saturated'] != 'false': raise ValueError('invalid counter interval')
            if row['runtime_wrapped'] not in ('true','false'): raise ValueError('invalid wrap flag')
            numbers = {k:int(row[k]) for k in ('sum_us','max_us','late','wall_us','runtime_us','acquisition_stack_bytes','checksum')}
            if any(n < 0 for n in numbers.values()): raise ValueError('negative value')
            if not (numbers['max_us'] <= numbers['sum_us'] <= count*numbers['max_us']): raise ValueError('invalid wall accounting')
            if numbers['sum_us'] > numbers['wall_us'] or numbers['runtime_us'] > numbers['wall_us'] or numbers['late'] > count: raise ValueError('invalid interval accounting')
            if numbers['wall_us'] >= 2**32-1: raise ValueError('runtime interval may span a full counter period')
            late, maximum, total = numbers['late'], numbers['max_us'], numbers['sum_us']
            if (late == 0) != (maximum <= 24000): raise ValueError('late count contradicts maximum')
            if late:
                minimum_sum = maximum + (late-1)*24001
                maximum_sum = late*maximum + (count-late)*24000
                if not minimum_sum <= total <= maximum_sum: raise ValueError('late count contradicts sum')
            if numbers['acquisition_stack_bytes'] == 0: raise ValueError('exhausted stack')
            row['mean_us'] = numbers['sum_us']/count
            phases.append(row)
        elif event == 'fixture_end':
            if row.get('attempted') != '129' or row.get('status') != 'ok': raise ValueError('failed fixture')
        elif row.get('expected') != '4' or row.get('finished') != '4' or row.get('status') != 'ok': raise ValueError('failed completion')
    return {'status':'ok','phases':phases}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('uart', type=Path)
    args = parser.parse_args()
    try: result = parse(args.uart.read_bytes().decode('utf-8', errors='strict'))
    except (ValueError, KeyError) as error:
        print(json.dumps({'status':'failed','error':str(error)})); raise SystemExit(1)
    print(json.dumps(result, indent=2))
if __name__ == '__main__': main()
