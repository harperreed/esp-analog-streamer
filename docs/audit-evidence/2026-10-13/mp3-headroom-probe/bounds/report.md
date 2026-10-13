# Fixed-point transform headroom

One additional MDCT table bit is sufficient and minimal for any signed 16-bit
PCM stream, with valid preceding history. Divide the existing 8/9 MDCT integer
table by two, truncating toward zero. Add four to the emitted MP3 global gain.
This changes the internal representation; the decoder exponent restores scale.

| Stage | Absolute bound after table /2 |
| --- | ---: |
| Every window accumulation prefix | 47,456,174 |
| Every analysis accumulation prefix | 909,534,933 |
| Every MDCT accumulation prefix | 1,750,915,446 |
| Final MDCT coefficient | 1,749,234,033 |
| Alias output | 1,764,964,939 |
| Alias signed 64-bit numerator | 3,790,233,342,744,681,691 |

All coefficient bounds fit int32, and the alias numerator fits int64. Products
of the bounded int32 factors fit int64 individually. The full integer report
also checks additional table shifts two and three, which are unnecessary.

With no added bit, the MDCT prefix bound is 3,501,830,858 and the alias output
bound is 3,529,929,835. A concrete signed-rail PCM witness makes the actual
35-term MDCT prefix equal 3,501,775,918, proving that zero added bits is unsafe.
The same witness with table /2 gives 1,750,887,950. Python and C agree exactly.

## Derivation

Let W[q], F[b,i], and M[k,t] be the actual compiled int32 tables, q=0..511,
i=q mod64. Slot t consumes PCM sample n=32t+31-q. The analysis filter reversal,
512-sample history and 32-sample stride give this index directly. Odd time slots
in odd bands change sign before MDCT, as in Shine.

Ignoring floor operations temporarily, a PCM sample contributes
W[q] F[b,i] M[k,t] / 2^80 to a MDCT coefficient: PCM shifts left by16,
then three products each shift right by32. Contributions for repeated PCM
indices are summed before taking absolute values. The proof computes every
descending MDCT prefix row in exact Python integers, over its 1,632-sample
support. Its L1 norm times32,768 bounds all possible signed16 PCM inputs.

Floor errors are bounded separately. Each y[i] has less than8 units of error;
each subband output therefore has error bounded by
ceil(8 sum(abs(F[b,:])) / 2^32)+64. A MDCT prefix of d terms adds at most
ceil(E_subband sum(abs(M_prefix)) / 2^32)+d. Alias rows combine the two fused
MDCT rows with the actual compiled CS/CA integers, preserving cancellation,
and propagate their error bounds. One final floor adds less than one unit.

The proof uses arithmetic right shifts, as the generic Shine macros do on the
tested host/target. It proves numerical bounds, not unrelated C semantics.
The separate negative-PCM left-shift UB was reproduced and fixed by replacing
the PCM shift with multiplication by65,536. Both represent the same Q31 domain.

## Gain and limits

Quantization uses the inverse scale2^(-stepsize/4), while the decoder uses
2^((global_gain-210)/4). Halving the internal coefficients and adding4 to
global_gain restores the intended scale. Integer table truncation and quantizer
thresholds can still change individual quantized values.

Every valid steptab index0..127 means stepsize in[-127,0]. With compensation,
global_gain lies in[87,214], within the 8-bit field. Bin search returns[-120,-1].
The existing inner loop increments stepsize without an explicit upper guard;
this proof does not establish its termination for every bit budget, or audit
all quantizer/psychoacoustic integer accumulations.

## Reproduction and evidence

This is throwaway host diagnosis. Run from the repository root:

```sh
uv run --locked --no-sync python target/kata-hardening/oct13-mp3-headroom/bounds/check-bounds.py
```

The checker requires the retained baseline table SHA256
0950ca08d6a8b5dab09ed482aad97acb48243b4bb282c6a0757e2dd86d8ab200.
Its first3,208 int32 entries have SHA256
1b8e667f1a9242eb8157f3ead53552617700badbe02c5ba34db982df8d68cabc;
the longer file adds16 actual C alias coefficients to the original W/F/M dump.

The separately compiled production candidate dump is exactly the baseline
MDCT table truncated /2, with every other table unchanged. Its SHA256 is
8ea74ddb63023da8bbfdc2cc093225f83e50f80722c191ddf13bd28b34478829.
See candidate-table-check.log. Do not rebuild baseline tables from modified
production sources; dump-tables.c now compiles the candidate when rebuilt.

check-witness.c calls actual production analysis code and computes the selected
MDCT prefix in int64. The first full UBSan run failed on the existing negative
PCM shift; witness-full-ubsan-red.stderr.log preserves that error. Rebuilding
against the multiplication fix passed the identical witness with full UBSan,
exit0 and empty stderr, in witness-full-ubsan-green.*.log.
