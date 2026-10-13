# ABOUTME: Proves Shine transform headroom for every signed 16-bit PCM stream.
# ABOUTME: Uses exact integer fused row norms and explicit fixed-point rounding bounds.
from pathlib import Path
import hashlib
import json
import numpy as np

HERE = Path(__file__).resolve().parent
I32 = 2**31 - 1
I64 = 2**63 - 1


def ceildiv(n, d):
    return (int(n) + d - 1) // d


def norm(row):
    return sum(abs(int(v)) for v in row)


def record(best, value, **where):
    if value > best.get('bound', -1):
        best.clear()
        best.update(bound=int(value), **where)


def main():
    table_path = HERE / 'tables.i32le'
    assert hashlib.sha256(table_path.read_bytes()).hexdigest() == (
        '0950ca08d6a8b5dab09ed482aad97acb48243b4bb282c6a0757e2dd86d8ab200'
    ), 'This proof must use the retained 8/9 baseline tables, including alias constants.'
    tables = np.fromfile(table_path, dtype='<i4').astype(np.int64)
    assert tables.size == 512 + 32 * 64 + 18 * 36 + 16
    window = tables[:512]
    filt = tables[512:2560].reshape(32, 64)
    original_mdct = tables[2560:3208].reshape(18, 36)
    alias_cs, alias_ca = tables[3208:3216], tables[3216:3224]
    q = np.arange(512)
    # For slot t, table index q consumes PCM at n=32*t+31-q.
    # Place n in [-480,1151] at array index n+480.
    indices = [32 * t + 511 - q for t in range(36)]
    wf = window.astype(object)[None, :] * filt[:, q % 64].astype(object)
    sb_error = [ceildiv(8 * norm(row), 2**32) + 64 for row in filt]
    stages = {'window_prefix': {}, 'analysis_prefix': {}}
    for i in range(64):
        subtotal = 0
        for depth in range(8):
            subtotal += abs(int(window[i + 64 * depth]))
            bound = ceildiv(32768 * subtotal, 2**16) + depth + 1
            record(stages['window_prefix'], bound, index=i, depth=depth + 1)
    for band in range(32):
        subtotal = 0
        for depth, i in enumerate(range(63, -1, -1), 1):
            subtotal += sum(abs(int(window[i + 64 * j]) * int(filt[band, i]))
                            for j in range(8))
            # Each y_i has at most 8 units of truncation error.
            err = ceildiv(8 * norm(filt[band, i:]), 2**32) + depth
            bound = ceildiv(32768 * subtotal, 2**48) + err
            record(stages['analysis_prefix'], bound, band=band, depth=depth)
    assert all(stage['bound'] <= I32 for stage in stages.values())
    results = []
    for shift in range(4):
        matrix = (np.abs(original_mdct) // (2**shift)) * np.sign(original_mdct)
        full_rows = {}
        full_errors = {}
        maxima = {'mdct_prefix': {}, 'mdct_final': {}, 'alias_output': {},
                  'alias_numerator': {}}
        worst_row = None
        for band in range(32):
            for k in range(18):
                row = np.zeros(1632, dtype=object)
                coefficient_norm = 0
                for depth, t in enumerate(range(35, -1, -1), 1):
                    coefficient = int(matrix[k, t])
                    coefficient_norm += abs(coefficient)
                    sign = -1 if band % 2 and t % 2 else 1
                    row[indices[t]] += wf[band] * (coefficient * sign)
                    error = ceildiv(sb_error[band] * coefficient_norm, 2**32) + depth
                    bound = ceildiv(32768 * norm(row), 2**80) + error
                    if bound > maxima['mdct_prefix'].get('bound', -1):
                        worst_row = row.copy()
                    record(maxima['mdct_prefix'], bound, band=band, coefficient=k,
                           depth=depth, rounding_error_bound=error)
                full_rows[band, k] = row
                full_errors[band, k] = error
                record(maxima['mdct_final'], bound, band=band, coefficient=k,
                       rounding_error_bound=error)
        for band in range(1, 32):
            for i in range(8):
                cs, ca = int(alias_cs[i]), int(alias_ca[i])
                a, b = full_rows[band, i], full_rows[band - 1, 17 - i]
                ea, eb = full_errors[band, i], full_errors[band - 1, 17 - i]
                for output, row, err in [('real', a * cs - b * ca, ea * abs(cs) + eb * abs(ca)),
                                         ('imag', a * ca + b * cs, ea * abs(ca) + eb * abs(cs))]:
                    numerator_bound = ceildiv(32768 * norm(row), 2**80) + err
                    bound = ceildiv(numerator_bound, 2**31) + 1
                    record(maxima['alias_output'], bound, band=band, index=i, output=output)
                    record(maxima['alias_numerator'], numerator_bound, band=band,
                           index=i, output=output)
        # Every multiply has int32 factors when all prior bounds pass.
        safe = (maxima['mdct_prefix']['bound'] <= I32 and
                maxima['alias_output']['bound'] <= I32 and
                maxima['alias_numerator']['bound'] <= I64)
        result = {'additional_table_shift': shift, 'global_gain_compensation': 4 * shift,
                  'safe_by_bound': safe, **maxima}
        if shift == 0:
            pcm = np.array([32767 if int(v) >= 0 else -32768 for v in worst_row], dtype=np.int64)
            (HERE / 'worst-prefix.s16le').write_bytes(pcm.astype('<i2').tobytes())
            sb = []
            for t in range(36):
                samples = pcm[indices[t]]
                y = ((samples * window) >> 16).reshape(8, 64).sum(axis=0)
                samples_sb = ((filt * y) >> 32).sum(axis=1)
                if t % 2:
                    samples_sb[1::2] *= -1
                sb.append(samples_sb)
            where = maxima['mdct_prefix']
            terms = [(int(sb[t][where['band']]) * int(matrix[where['coefficient'], t])) >> 32
                     for t in range(35, 35 - where['depth'], -1)]
            witness = sum(terms)
            result['constructive_witness'] = {'prefix_sum': witness,
                                             'outside_int32': not -2**31 <= witness <= I32,
                                             'pcm_file': 'worst-prefix.s16le',
                                             'pcm_index_zero_absolute_sample': -480}
            assert result['constructive_witness']['outside_int32']
        results.append(result)
        print(json.dumps(result), flush=True)
    minimum = next(row['additional_table_shift'] for row in results if row['safe_by_bound'])
    report = {
        'tables_sha256': hashlib.sha256(table_path.read_bytes()).hexdigest(),
        'input': 'Any PCM sample in [-32768,32767]; zeroed or valid preceding history.',
        'arithmetic': 'Generic Shine mul=floor(a*b/2^32), cmuls=floor((a*b +/- c*d)/2^31).',
        'proof': 'Exact integer fused PCM row L1 norms; absolute PCM bound 32768; each floor adds <1 error.',
        'table_attenuation': 'Each actual MDCT table entry truncates toward zero after division by 2^shift.',
        'scope': 'Every window, analysis, MDCT accumulation prefix; pre/post-alias coefficient; alias int64 numerator.',
        'not_proven': 'Quantizer internals and 8-bit global_gain availability/range are outside this transform proof.',
        'stage_bounds': stages, 'candidates': results, 'smallest_shift_proven_safe': minimum,
    }
    (HERE / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print('smallest_shift_proven_safe=' + str(minimum), flush=True)


if __name__ == '__main__':
    main()
