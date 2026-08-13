# Custom FFmpeg 8.0 — optimization inventory, CABAC deep dive, and applied changes

This covers what the custom fork (`ffmpeg_installer/custom_ffmpeg.diff`)
optimizes, a walk through the H.264 CABAC assembly.

## 1. What the custom fork already optimizes

Inventory of `custom_ffmpeg.diff` — **130 hunks across 30 files**.
Every change now carries a comment
in the source.
Roughly ~65 of those hunks are the same `if (!motion_vectors_only)` guard
applied to different pixel work.

| Area | Files | What it does | Note |
|---|---|---|---|
| **Skip pixel reconstruction** | `h264_mb.c`, `h264dec.c`, `h264_slice.c`, `hevcdec.c` ×20 | `ff_h264_hl_decode_mb()` returns immediately in `motion_vectors_only` mode; motion compensation, intra prediction and residual application never run | Mechanical and self-documenting. The interesting one is `hevcdec.c` `hls_prediction_unit` — the MV-only early return sits *immediately after* `tab_mvf` is written, which is precisely why reference pictures can be 64×64 stubs |
| **No-op DSP** | `h264dsp.c` (+63) | All IDCT/dequant/loop-filter/weight function pointers point at empty stubs, so any stray call is free | Same guard class. The `ff_h264dsp_init` signature change it needs is listed under *Signature propagation* below |
| **Skip the loop filter** | `h264_slice.c` | Every `loop_filter()` call gated off; deblocking never runs | H.265's equivalent is the subtle one — see *Threading correctness*: the filter stage is skipped but the progress report inside it must be kept |
| **Skip residual *storage*, keep residual *parsing*** | `h264_cabac.c` (+615), `h264_cavlc.c` (+273), `hevc/cabac.c` | `*_skip` clones of the residual decoders consume every bin/bit to keep the entropy decoder synchronized but write no coefficients — no `block[]` stores, no scantable index array, `non_zero_count_cache` reduced to the binary flag downstream contexts actually read | The one class that must stay bit-exact. CABAC needs it for adaptive state, CAVLC only for bit position — hence CAVLC's skip variants can use `skip_bits()` where CABAC must decode |
| **Significance scan in asm** | `x86/h264_cabac.c` (+215) | `decode_significance_skip_x86` / `decode_significance_8x8_skip_x86`: whole scan loop in inline asm, CABAC `low`/`range` and the coefficient counter held in registers across the loop instead of memory round-trips per bin | Same bit-exactness constraint as the row above |
| **Cheaper bypass consumption** | `x86/h264_cabac.c` | `consume_cabac_bypass_x86`: bypass decode that drops the return-value materialization for sign/suffix bins whose value is never used | Only the value-return path is dropped — the `low`/`range`/bytestream update is identical to the stock primitive |
| **Minimal allocations** | `h264_slice.c`, `decode.c`, `h264_ps.c`, `hevc/cabac.c` | 16×16 dummy picture buffers instead of full frames; 64-byte scratch buffers; no film grain, no ER, no error-flags pool; qscale table writes gated off (`h264_mvpred.h`); dequant tables and the `coeffs` memset elided | Where the −36% memory comes from |
| **Trimmed neighbor caches** | `h264_mvpred.h` (+99) | `fill_decode_neighbors_caches_bskip_direct()`: minimal cache fill for the B_SKIP/spatial-direct path, skipping fields only pixel reconstruction reads | Trimming `fill_decode_caches()` any further was measured and rejected — see §2.7 |
| **Compact export** | `mpegutils.c` (+166), `motion_vector.h`, `frame.h`, `hevc/hevcdec.c` | `AVMotionVectorCompact` (12 B vs ~40 B), single-pass worst-case-sized buffer (two-pass counting was a race under frame threading), `mv_l0_only` list-0 filter, zero-size vector filter | The worst-case buffer sizing is now stale — the export runs on a fully-decoded picture, see §3c |
| **Options plumbing** | `avcodec.h`, `options_table.h`, `h264_parser.c`, `h264_ps.c`, `hevcdec.h`/`.c` | `motion_vectors_only`, `mv_l0_only` AVOptions | HEVC keeps a decoder-private mirror of the flag because its hot loops test it per-CTU |
| **Validation drops** | `ff_h264_check_intra*_pred_mode`, `mismatches_ref` | Only reject inputs that would break *pixel* reconstruction | Side effect worth knowing: streams stock FFmpeg rejects with −1 still yield motion vectors here — see §3d |
| **Signature propagation** | `h264dsp.h`/`.c`, `svq3.c` | One `ff_h264dsp_init` signature carries the flag through | SVQ3 forwards the flag it never sets |
| **Threading correctness** (2026-08-03/04) | `h264dec.c`, `h264_picture.c`, `pthread_frame.c`, `hevc/refs.c`, `hevc/filter.c` | The export race fix + progress fast path + coarse row reporting | `hevc/filter.c` is the subtle one: the whole filter stage is skipped but the **progress report inside it is kept**, or frame threads awaiting the picture deadlock |
| **Build marker** | `ffbuild/version.sh` | ` custom` version suffix | So a benchmark run can prove which libavcodec it linked |
| **HEVC equivalents** | `hevc/*` (+400) | Same idea for H.265: parse-only residuals, skip filters/reconstruction, merged-PU MV export | Its changes fall under the rows above rather than forming a separate mechanism |

## 2. The CABAC assembly — is the biggest win there? (No. Evidence below.)

### 2.1 What the hot path actually is

Per context-coded bin, `BRANCHLESS_GET_CABAC` (x86 inline asm,
`libavcodec/x86/cabac.h`) does: state-indexed LPS lookup
(`ff_h264_cabac_tables`), cmov-based MPS/LPS select (`HAVE_FAST_CMOV=1` in
this build), table-driven renormalization (`norm_shift`), state transition
store, and a 16-bit refill with `bswap` when `low`'s low word empties. This is
the standard FFmpeg implementation and is already the product of years of
upstream tuning; there is no headroom inside a single bin decode.

The skip-mode structure per non-zero coefficient (`SKIP_BLOCK`,
[h264_cabac.c:2197](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_cabac.c#L2197)):

```
1 ctx bin  (abs > 1?)          ── get_cabac, adaptive context
abs == 1:  1 bypass bin (sign) ── consume_cabac_bypass
abs >= 2:  ≤13 ctx bins (unary) then, rarely, EG0 bypass escape; then sign
```

### 2.2 Why you cannot decode fewer bins

CABAC is an adaptive arithmetic coder: every bin updates either a context
model or the `low`/`range` state that all later bins are decoded against.
There is no marker to seek to — to reach macroblock *k+1*'s MVD you must
consume macroblock *k*'s residual bins, exactly. The fork's `_skip` variants
already removed everything that *is* removable (the stores). What remains is
the mandatory bin consumption.

### 2.3 Experiment: out-of-line `get_cabac` (negative result, kept as documentation)

`perf` on the post-change build (t16, no CSV) shows:

| symbol | self |
|---|---|
| `ff_h264_decode_mb_cabac` (with inlined skip loops) | 43.2% |
| `get_cabac` (out-of-line copies GCC made) | 31.0% |
| `fill_decode_caches` | 4.7% |
| `decode_cabac_mb_mvd_skip` | 3.1% |
| `ff_print_debug_info2_optimized` | 2.65% |
| `decode_cabac_mb_ref` | 2.4% |

A 31% standalone `get_cabac` looks like call overhead begging to be inlined.
Tested: forcing `av_always_inline` removes the out-of-line copies — and wall
time *does not move* (t1 no-CSV 2.004 s → 2.038 s; t16 0.465 → 0.470). The 31%
is the bin-decode work itself; the function boundary only made it visible.
Reverted, with a comment in `cabac_functions.h` so nobody retries it blind.

### 2.3b Instruction-level confirmation + `-march=native` (both measured)

`perf annotate` on the current build (t1): the hot instructions inside
`get_cabac` and inside the inlined copies in `ff_h264_decode_mb_cabac` are
the LPS table load (`movzbl 0x200(tables,range)`), the `sub`→`cmova`→`sbb`
select, the renorm `shl %cl`, and the state-transition store — the arithmetic
decoder's serial dependency chain, ~81% of t1 decode CPU in total. There is
no fat between those instructions.

Rebuilding with `--extra-cflags="-march=native"`: t1 no-CSV 2.016 → 1.999 s
(~1%, at the noise floor; t16 unchanged; output byte-identical). The engine
is inline asm, so the ISA baseline barely matters. Not adopted — not worth
losing binary portability. Build reverted to the standard configure.

### 2.4 Bypass batching (analyzed, not implemented)

Bypass bins are the binary digits of `low/range`: N consecutive bypass bins
can be decoded with one division — quotient = the N bits, remainder = new
`low`. This is a real technique (used in some AV1/VVC decoders) and would
replace N branchy iterations with one `div`. It does not pay here: in
`SKIP_BLOCK`, bypass bins arrive as *single* sign bits sandwiched between
context bins, and the only multi-bin bypass runs (EG0 escapes, `abs ≥ 15`)
are rare at surveillance bitrates. A division (~20-40 cycles) loses to 1-2
iterations of the existing asm. `consume_cabac_bypass` measured 6.0% self at
t1 — the realistic recovery from batching is a fraction of that.

### 2.5 The last asm idea — register-resident level loops (BUILT, MEASURED, REVERTED)

The hypothesis: `get_cabac_inline_x86` binds `c->low`/`c->range` as register
operands but they are *struct fields* under a `"memory"` clobber, so GCC
reloads and stores both around every bin — four stack round-trips inside the
serial chain. Binding the same asm to plain C locals (whose address never
escapes, so the `"memory"` clobber cannot spill them) should keep the state
in registers across a whole loop, the way `decode_significance_skip_x86`
does.

Implemented on 2026-08-05 as `GET_CABAC_REG` (BRANCHLESS_GET_CABAC on int
locals) + C bypass/refill on the same locals, rewriting `SKIP_BLOCK` and the
4:2:2 chroma-DC significance scan. Disassembly confirmed the intended
codegen: `low`/`range` fully register-resident through ctx bins, bypass bins
and refills — zero stack traffic in any loop. Output bit-identical on every
gate (MCTTR t1/t16, school, bus, determinism).

Result: **performance-neutral.** t1 no-CSV 1.995 s vs 2.00-2.03 baseline
(≤1%, noise); t16 0.480 vs 0.466 (slightly worse, likely code growth).
Store-to-load forwarding was already hiding the struct round-trips behind
the longer LUT→cmov→renorm dependency chain. Reverted — the experiment
closes the last open CABAC lever with a valid negative. Together with §2.3
(inlining: neutral), §2.3b (ISA flags: ≤1%), and §2.4 (bypass batching:
structurally unprofitable), the CABAC floor is now confirmed from four
independent directions. **There is no meaningful win left inside the
entropy decoder.** Ceiling ~3-6% of total, risk high (a bitstream desync is silent
corruption of everything downstream). Only worth attempting with
`mv_run_diff` regression gating, and only after everything in §3 is exhausted
— which it now is. **Conclusion: the biggest win was not in the CABAC asm; it
was in thread synchronization and serialization, both fixed below.**


### 2.6 PGO build — ADOPTED, +2.1% mean

Reproducible via **`make setup_ffmpeg_pgo`**: instrument the custom FFmpeg, run
a workload spanning CABAC/CAVLC/HEVC at 1 and N threads, rebuild with the
profile.

| extractor5, decode-only | t1 | t16 |
|---|---|---|
| h264 cabac | **+1.8%** | −0.0% |
| school (1080p cabac) | **+4.8%** | −0.0% |

Mean **+2.1%**, output byte-identical. The gain concentrates at t1 and vanishes
at t16 — once threading dominates, wall time is set by parallelism, not code
layout. Different mechanism from `-march=native` (§2.3b, ~1%): the engine is
inline asm so the ISA baseline barely matters, but PGO improves branch layout
and block ordering in the surrounding C.

### 2.7 Neighbour-cache trimming — measured, rejected

The idea was to trim `fill_decode_caches()` (3.1% at t1) for MV-only mode the
way `fill_decode_neighbors_caches_bskip_direct()` already trims the B_SKIP path.
Two findings, both negative:

- **P_SKIP needs no trimming — it already bypasses `fill_decode_caches()`
  entirely.** `decode_mb_skip()` calls only `fill_decode_neighbors()`, and
  `pred_pskip_motion()` reads `cur_pic.motion_val`/`ref_index` directly instead
  of the caches. The premise was simply wrong.
- **The only genuinely dead work is the `*_samples_available` derivation** —
  ~40 lines of mask arithmetic describing which neighbouring *pixels* an intra
  predictor may read, whose sole consumers (`ff_h264_check_intra*_pred_mode`)
  are already skipped in MV-only mode. Gating it measured **+0.2% / +0.1% /
  −0.1% / +0.6%** across cabac and cavlc at t1/t16 — noise, because it runs only
  inside `if (IS_INTRA(mb_type))` and intra MBs are a small minority of a
  P/B-heavy surveillance stream. Reverted.

Everything else `fill_decode_caches()` fills is live in MV-only mode:
`mv_cache`/`ref_cache` feed MV prediction, `nnz_cache` + `top_cbp`/`left_cbp`
feed CABAC context derivation (and `pred_non_zero_count()` for CAVLC),
`mvd_cache` feeds the MVD contexts, `direct_cache` feeds B direct mode.