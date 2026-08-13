# Assembly in custom FFmpeg 8.0 — what VTune shows, and what every asm site actually does

Companion to [OPTIMIZATION_ANALYSIS.md](OPTIMIZATION_ANALYSIS.md). That document
argues *whether* the assembly is worth optimizing (answer: no, §2 there). This
one answers a different question: **which assembly does the MV-only decoder
actually execute, and what is each piece of it doing**, in plain language.

Everything below is grounded in the VTune hotspot/call-tree data of the recent
runs, then traced back to the source in
[ffmpeg/FFmpeg-8.0-custom/](ffmpeg/FFmpeg-8.0-custom/).

---

## 1. Scope and method

**Profiles used.** All 153 `vtune_results/hotspots.csv` + `topdown.csv` pairs
written since 2026-08-01, under [results/](results/):

| Sweep | Runs | Content |
|---|---|---|
| `results/bulk/20260805_21xx–20260806_02xx` | 24 | MCTTR / school / bus, t1…t128 |
| `results/bulk_3/20260803_11xx–16xx` | 24 | same matrix, pre-PGO |
| `results/h264_cabac/2026080x_*`, `20260809_*` | 105 | the 2026-08-09 t1…t128 sweep is the newest |

Every one of these profiled `executables/extractor4` against **H.264 CABAC**
inputs (`videos/h264_cabac/*.mp4`), collected with `--type=cpu:stack
--interval=10 --stack-stitching`. Confirmed from
`vtune_results/config/runss.options`.

**Binary under test.** `ffmpeg/FFmpeg-8.0-custom/lib/libavcodec.so.62`, x86-64,
built with `HAVE_INLINE_ASM=1`, `HAVE_FAST_CMOV=1`, `HAVE_7REGS=1` (implied by
`ARCH_X86_64`), `HAVE_I686=1`, `HAVE_SSE2/AVX2=1`, PIC (so
`BROKEN_RELOCATIONS` is on), PGO-instrumented profile applied. These switches
decide which asm variant you get — see §6.

---

## 2. What the profiles actually say

### 2.1 Aggregate over all 153 recent runs

```
grand total CPU:  658.1 s
libavcodec:       457.8 s   (69.6% of total)
x86 inline asm:   229.4 s   (34.9% of total, 50.1% of libavcodec)
```

**Half of all time spent inside libavcodec is spent inside hand-written inline
assembly.** Per-run the share ranges 21%–66% (median 41%, n=97 after dropping
runs whose total sample time was under 1 s and therefore too sparse to be
meaningful). The spread is not asm variance — it is how much of the run went to
CSV writing on the Rust side, which grows with clip size and thread count.

### 2.2 The six symbols, ranked (aggregate seconds)

| Symbol | Σ CPU | Source | Custom or upstream |
|---|---|---|---|
| `get_cabac_inline_x86` | 142.7 s | [x86/cabac.h:186](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L186) | upstream |
| `decode_significance_skip_x86` | 46.7 s | [x86/h264_cabac.c:208](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L208) | **custom** |
| `decode_significance_8x8_skip_x86` | 29.3 s | [x86/h264_cabac.c:286](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L286) | **custom** |
| `consume_cabac_bypass_x86` | 9.4 s | [x86/h264_cabac.c:385](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L385) | **custom** |
| `get_cabac_bypass_sign_x86` | 0.66 s | [x86/cabac.h:221](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L221) | upstream |
| `get_cabac_bypass_x86` | 0.53 s | [x86/cabac.h:267](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L267) | upstream |

Three more asm sites are real but do not get their own VTune symbol because
they are always inlined into their caller (§4.8–§4.10): `mid_pred`,
`AV_COPY128`/`AV_ZERO128` (visible indirectly as `_mm_store_si128`, 5.9 s /
`_mm_load_si128`, 1.2 s) and `av_bswap32/64`.

### 2.3 Per-run shares — the newest sweep (MCTTR0102b, 2026-08-09, extractor4)

| threads | total CPU | asm total | asm % | `get_cabac_inline` | `dec_sig_skip` | `dec_sig_8x8_skip` | `consume_bypass` |
|---|---|---|---|---|---|---|---|
| t1 | 2.000 s | 1.276 s | **63.8%** | 0.677 | 0.223 | 0.288 | 0.088 |
| t2 | 2.190 | 1.276 | 58.3% | 0.682 | 0.266 | 0.262 | 0.066 |
| t4 | 2.500 | 1.280 | 51.2% | 0.708 | 0.196 | 0.280 | 0.084 |
| t8 | 2.420 | 1.418 | 58.6% | 0.720 | 0.324 | 0.306 | 0.068 |
| t16 | 2.560 | 1.252 | 48.9% | 0.708 | 0.166 | 0.320 | 0.048 |
| t32 | 2.750 | 1.266 | 46.0% | 0.606 | 0.252 | 0.346 | 0.062 |
| t64 | 2.490 | 1.164 | 46.7% | 0.708 | 0.232 | 0.204 | 0.020 |
| t128 | 2.270 | 1.020 | 44.9% | 0.540 | 0.190 | 0.220 | 0.040 |

The **absolute** asm cost is flat across thread counts (1.0–1.4 s) — it is the
same bitstream being decoded either way. The share falls only because
synchronization and CSV writing add CPU time around it.

Larger clip, same picture (school, 1080p CABAC): t1 `get_cabac_inline_x86`
32.4%, `decode_significance_skip_x86` 10.0%, `decode_significance_8x8_skip_x86`
6.3%, `consume_cabac_bypass_x86` 1.9%.

### 2.4 Where the assembly sits in the call tree

From `results/bulk/20260805_2219_MCTTR0102b_t1/vtune_results/topdown.csv`
(percentages are *total* time under that node; `self` is seconds in the node
itself):

```
ff_h264_decode_mb_cabac                            98.2%
├── decode_cabac_luma_residual_skip                41.9%
│   ├── decode_cabac_residual_nondc_skip           20.6%   (8x8 transform blocks)
│   │   └── decode_cabac_residual_internal_skip    20.0%
│   │       ├── decode_significance_8x8_skip_x86   15.8%  ← ASM
│   │       ├── get_cabac → get_cabac_inline_x86    2.0%  ← ASM
│   │       └── consume_cabac_bypass_x86            1.8%  ← ASM
│   └── decode_cabac_residual_nondc_skip           15.7%   (4x4 transform blocks)
│       └── decode_cabac_residual_internal_skip    12.3%
│           ├── decode_significance_skip_x86        5.8%  ← ASM
│           ├── get_cabac → get_cabac_inline_x86    4.6%  ← ASM
│           └── consume_cabac_bypass_x86            0.8%  ← ASM
├── decode_cabac_residual_dc_422_skip              14.5%
│   └── decode_cabac_residual_internal_skip        13.5%
│       ├── get_cabac → get_cabac_inline_x86        9.9%  ← ASM
│       └── consume_cabac_bypass_x86                1.2%  ← ASM
├── decode_mb_skip                                  5.6%
│   ├── ff_h264_pred_direct_motion                  2.2%
│   └── write_back_motion → write_back_motion_list  1.4%
│       └── AV_ZERO128 → _mm_store_si128            0.4%  ← SSE2
├── get_cabac → get_cabac_inline_x86                4.6%  ← ASM  (mb_type etc.)
├── decode_cabac_mb_ref → get_cabac_inline_x86      1.8%  ← ASM
├── decode_cabac_mb_cbp_luma_skip → …               2.4%  ← ASM
├── decode_cabac_mb_mvd_skip                        3.0%
│   ├── get_cabac → get_cabac_inline_x86            1.2%  ← ASM
│   └── get_cabac_bypass_sign_x86                   0.8%  ← ASM
└── fill_decode_caches                              1.6%
```

---

## 3. CABAC

CABAC (Context-Adaptive Binary Arithmetic Coding) is the entropy coder H.264
uses in `high` profile. It has one primitive: **decode one bit ("bin") given a
context**. Everything in an H.264 slice — macroblock type, reference index,
motion vector difference, every residual coefficient — is a sequence of bins.

The decoder state is three numbers plus a table of context models:

| State | Meaning |
|---|---|
| `range` | width of the current arithmetic interval (9 bits) |
| `low` | position inside it, with 16 bits of look-ahead (`CABAC_BITS = 16`) |
| `bytestream` | read pointer into the slice payload |
| `state[ctx]` | one byte per context: 6-bit probability index + 1 MPS bit |

Decoding a bin means: split `range` by the context's probability, decide which
side `low` falls in, subtract if it fell in the upper side, update the context's
probability, renormalize, and refill `low` from the bytestream when it runs dry.

That is a **strictly serial dependency chain**. Bin *n+1*'s interval split
depends on bin *n*'s result. You cannot vectorize it, you cannot speculate past
it, and — because the coder is adaptive — you cannot skip a bin you don't care
about. This is why the assembly exists, why it is scalar rather than SIMD, and
why the custom fork's savings come from *not storing* the results rather than
*not decoding* the bins.

---

## 4. Every assembly site, explained

### 4.1 `BRANCHLESS_GET_CABAC` — the engine

**Source:** [x86/cabac.h:80-113](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L80-L113)
(PIC variant, the one this build uses) plus
[`BRANCHLESS_GET_CABAC_UPDATE`:58-64](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L58-L64)
(the `HAVE_FAST_CMOV` variant).

It is a **macro that expands one bin decode inline**.
Every symbol in §2.2 except `consume_cabac_bypass_x86` is built out of one or
two copies of it. All 142.7 s of `get_cabac_inline_x86` and most of the
76 s in the two significance scanners is this macro running.

Here is the actual compiled code from the shipped binary, annotated. This is
`objdump -d lib/libavcodec.so.62.11.103` at `<get_cabac>` — the out-of-line copy
GCC emits (see §4.2 on why it exists). `%rdi` = `CABACContext *c`, `%rsi` =
`uint8_t *state`:

```asm
  lea    0x9baa1(%rip),%r9            ; r9 = ff_h264_cabac_tables (PIC base)
  mov    (%rdi),%r8d                  ; low   = c->low
  mov    0x4(%rdi),%edx               ; range = c->range
  movzbl (%rsi),%eax                  ; s     = *state  (6-bit prob idx | MPS bit)

  ;; ---- 1. Split the interval -------------------------------------------
  mov    %edx,%r10d                   ; keep a copy of range
  and    $0xc0,%edx                   ; quantize range to its top 2 bits
  lea    (%eax,%edx,2),%ecx           ; index = s + 2*(range & 0xC0)
  movzbl 0x200(%r9,%rcx,1),%edx       ; RangeLPS = lps_range[index]   (offset 512)
  sub    %edx,%r10d                   ; rMPS = range - RangeLPS

  ;; ---- 2. Decide MPS vs LPS, branchlessly ------------------------------
  mov    %r10d,%ecx                   ; save rMPS
  shl    $0x11,%r10d                  ; rMPS << 17  (align to low's 16-bit look-ahead)
  cmp    %r8d,%r10d                   ; compare (rMPS<<17) against low  -> sets CF
  cmova  %ecx,%edx                    ; MPS taken?  range = rMPS   else range = RangeLPS
  sbb    %rcx,%rcx                    ; rcx = 0 (MPS) or -1 (LPS), reusing cmp's flags
  and    %ecx,%r10d                   ; (rMPS<<17) &= mask
  xor    %rcx,%rax                    ; s ^= mask  -> selects the LPS half of the LUT
  sub    %r10d,%r8d                   ; low -= (rMPS<<17) & mask

  ;; ---- 3. Renormalize + update the context model -----------------------
  movzbl (%r9,%rdx,1),%ecx            ; shift = norm_shift[new range]   (offset 0)
  shl    %cl,%edx                     ; range <<= shift
  movzbl 0x480(%r9,%rax,1),%r10d      ; next = mlps_state[128 + s]      (offset 1024+128)
  shl    %cl,%r8d                     ; low   <<= shift
  mov    %r10b,(%rsi)                 ; *state = next        <- the "adaptive" part

  ;; ---- 4. Refill 16 bits when low's look-ahead empties ------------------
  test   %r8w,%r8w
  jne    done                         ; still have bits -> skip the refill
  mov    0x10(%rdi),%rcx              ; p = c->bytestream
  addq   $0x2,0x10(%rdi)              ; c->bytestream += 2
  movzwl (%rcx),%r10d                 ; grab 2 bytes
  lea    -0x1(%r8d),%ecx              ; \
  xor    %r8d,%ecx                    ;  } isolate how many low bits are free
  shr    $0xf,%ecx                    ; /
  bswap  %r10d                        ; big-endian bitstream -> host order
  shr    $0xf,%r10d
  movzbl (%r9,%rcx,1),%ecx            ; norm_shift lookup again
  sub    $0xffff,%r10d
  neg    %ecx
  add    $0x7,%ecx
  shl    %cl,%r10d                    ; position the new bits
  add    %r10d,%r8d                   ; low |= new bits
done:
  mov    %r8d,(%rdi)                  ; c->low   = low
  and    $0x1,%eax                    ; bit = s & 1
  mov    %edx,0x4(%rdi)               ; c->range = range
  ret
```

**Why it is written in assembly, in three points:**

1. **`cmova` + `sbb` reuse one comparison for two purposes.** The `cmp` sets the
   carry flag. `cmova` (which does not touch flags) picks the new `range`, then
   `sbb %rcx,%rcx` turns that *same* carry into an all-ones/all-zeros mask
   without a second compare. That mask then drives *both* the `low` subtraction
   and the context-state flip. A compiler will generally emit a branch here, and
   the branch is ~50/50 unpredictable by construction — an arithmetic coder that
   is doing its job has no predictable bins. Branchless is worth roughly a
   mispredict (15–20 cycles) on half the bins.
2. **Three table lookups against one PIC base register.** `lps_range` (+512),
   `norm_shift` (+0) and `mlps_state` (+1024+128) are all slices of one 1536-byte
   array `ff_h264_cabac_tables`. Holding its address in `%r9` for the whole
   decode makes each lookup a single `movzbl base(%r9,%idx,1)`. This is the
   `BROKEN_RELOCATIONS` path — in a position-independent shared library the
   alternative (`MANGLE()`-style absolute addressing) is not available.
3. **Renormalization is a table lookup, not a loop.** The textbook algorithm
   shifts `range` left one bit at a time until it exceeds 0x100. Here
   `norm_shift[range]` gives the whole count and one variable `shl %cl` does it.

**The cost is the dependency chain, not the instruction count.** Every arrow in
that listing is a true data dependency: LUT load → `sub` → `cmp`/`cmova` → mask →
`shl` → store. ~30 instructions that cannot overlap with each other, ~9–12
cycles of latency, executed once per bin. `perf annotate` (OPTIMIZATION_ANALYSIS.md
§2.3b) puts ~81% of single-threaded decode time on exactly these instructions.

### 4.2 `get_cabac_inline_x86` — one bin, the general case

**Source:** [x86/cabac.h:186-216](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L186-L216)
· **142.7 s aggregate, 23–33% of any single run — the single largest consumer of
CPU in the whole extractor.**

A one-line wrapper: expand `BRANCHLESS_GET_CABAC` once, return `bit & 1`. It is
`av_always_inline` on x86-64, so most call sites get a private copy welded into
the caller; VTune attributes those copies to this symbol via debug info, which is
why the same name appears a dozen times in a single `hotspots.csv`.

Everything that is not a residual significance scan comes through here:

| Caller | What bin it is decoding |
|---|---|
| `decode_cabac_mb_skip` | is this macroblock skipped? |
| `decode_cabac_mb_type` / `_intra_mb_type_skip` | macroblock partitioning |
| `decode_cabac_mb_ref` | which reference picture |
| `decode_cabac_mb_mvd_skip` | motion vector difference magnitude (unary prefix) |
| `decode_cabac_mb_cbp_luma_skip` / `_chroma_skip` | which blocks carry residual |
| `get_cabac_cbf_ctx` | coded-block-flag, per transform block |
| `decode_cabac_residual_internal_skip` | coefficient level magnitudes |

Note the 9.9% under `decode_cabac_residual_dc_422_skip` in the call tree: the
4:2:2 chroma-DC path has only 7 coefficient positions, so it uses the plain C
`DECODE_SIGNIFICANCE_COUNT_ONLY` loop calling `get_cabac` per position rather
than the asm scanner — which is why `get_cabac_inline_x86` shows up so heavily
there instead of `decode_significance_skip_x86`.

One measured non-obvious point, already recorded in
[cabac_functions.h:144-151](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/cabac_functions.h#L144-L151):
GCC emits out-of-line copies of `get_cabac` (the `objdump` above is one) and
`perf` shows them at ~31% self. Forcing them inline **does not improve wall
time** — the 31% is the bin-decode work itself, the function boundary only made
it visible. Do not "fix" this.

### 4.3 `decode_significance_skip_x86` — the 4×4 significance scan *(custom)*

**Source:** [x86/h264_cabac.c:208-283](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L208-L283)
· **46.7 s aggregate, 8–12% per run** · called from
[h264_cabac.c:2153](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_cabac.c#L2153)

**The problem it solves.** For each 4×4 transform block, H.264 codes a
"significance map": for each of up to 15 scan positions, one bin saying *is
there a coefficient here?*, and if yes, a second bin saying *is this the last
one?*. That is a tight loop over up to 30 bins.

**What upstream does** ([the `decode_significance_x86` twin at line 46](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L46)):
the same loop, but each significant position writes its scan index into an
`index[]` array so the caller can later place the decoded coefficient into
`block[]`.

**What the custom version does differently.** In MV-only mode nothing ever reads
`block[]` — reconstruction is skipped entirely. So the scanner does not need the
positions, only *how many* there were (the caller still needs the count to know
how many level values to consume). The rewrite therefore:

- **Deletes the `index[]` stores.** Upstream's loop does
  `mov %2,%0 / movl %7,%%ecx / add %1,%%rcx / movl %%ecx,(%0)` — an address
  reload, an add and a store on *every significant coefficient*, plus an
  `add $4, %2` to bump the write pointer. All of it is gone.
- **Replaces them with `add $1, %2` on a register.** The comment in the source
  says it outright: `/* counter++ (REGISTER!) */`. `counter` is bound `"+r"`,
  not `"+m"`. Across a whole block the coefficient count never touches memory.
- **Lays the loop out hot-path-first.** `test $1,%4 / jnz 6f` falls through on
  *not significant* — the common case at surveillance bitrates — and jumps away
  to a cold block for significant positions. The cold block decodes the
  last-flag, bumps the counter, and either exits (`jnz 5f`, leaving the context
  pointer dead so no restore is needed) or `sub`s the offset back and rejoins.

Only three registers change across an entire block: the context base pointer, the
counter, and `low`/`range`. That register residency is the whole point — and it
is why the equivalent trick applied to the *level* loop (`GET_CABAC_REG`,
OPTIMIZATION_ANALYSIS.md §2.5) measured neutral: there the chain was already
long enough to hide the memory traffic.

### 4.4 `decode_significance_8x8_skip_x86` — the 8×8 significance scan *(custom)*

**Source:** [x86/h264_cabac.c:286-372](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L286-L372)
· **29.3 s aggregate, 4.5–16% per run** · called from
[h264_cabac.c:2147](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_cabac.c#L2147)

Same job for 8×8 transform blocks — 64 positions instead of 16 — with one extra
complication that makes the assembly look busier.

For 4×4 blocks the context index *is* the scan position. For 8×8 blocks it is
not: the standard maps 64 positions onto a much smaller set of contexts through
two lookup tables. The loop therefore does two indirections per position that
the 4×4 version does not:

```asm
  mov    %9, %0                       ; %9 = sig_off table
  movzb  (%0, %6), %6                 ; ctx_offset = sig_off[position]
  add    %8, %6                       ; + significant_coeff_ctx_base
  <BRANCHLESS_GET_CABAC>              ; significance bin

  ;; if significant, the last-flag has its own mapping:
  movzb  %c13(%14, %q6), %6           ; ff_h264_cabac_tables[LAST_8x8_OFFSET + position]
  add    %10, %6                      ; + last_coeff_ctx_base
  <BRANCHLESS_GET_CABAC>              ; last bin
  add    $1, %2                       ; counter++  (register)
```

Note `%c13(%14, %q6)` — the 8×8 last-flag mapping lives at offset 1280 inside the
*same* `ff_h264_cabac_tables` blob already held in a register, so it costs one
`movzb` rather than a separate table pointer.

`%6` (`state`) is deliberately reused as both loop counter and scratch context
pointer, with `%1` (`last`) as its backing store — that juggling is what the
repeated `mov %1,%6` / `mov %6,%1` pairs are doing. It exists because x86-64
inline asm with this many live operands is at the limit of what GCC can allocate
(`HAVE_7REGS`), and it is also why the `jnz 5f` early exits are written to leave
dead registers unrestored.

Same custom modifications as §4.3: no `index[]` stores, counter in a register.
Its share *grows* with thread count in the recent sweep (0.288 s at t1 →
0.346 s at t32) while the 4×4 scanner's does not, which is a sampling artifact
of which slices land on which worker, not a real effect.

### 4.5 `consume_cabac_bypass_x86` — decode a bin and throw it away *(custom)*

**Source:** [x86/h264_cabac.c:385-420](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/h264_cabac.c#L385-L420)
· **9.4 s aggregate, 1.3–3.8% per run** · called from `SKIP_BLOCK()` at
[h264_cabac.c:2196-2226](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_cabac.c#L2196-L2226)

**Bypass bins** are the cheap kind: no context model, no probability lookup,
just "shift `low` left one and see which half it lands in" — the bin is
literally the next binary digit of `low/range`. H.264 uses them for coefficient
sign bits and for the suffix of large-magnitude escape codes.

**The observation this function is built on:** in MV-only mode, the *values* of
those bins are never used. A sign bit only matters if you are going to store a
coefficient, and nothing stores coefficients here. But the bin still has to be
decoded, because consuming it is what advances `low` and the bytestream to where
the next bin the decoder *does* care about begins.

So this is a byte-for-byte copy of the stock `get_cabac_bypass_x86` (§4.7) with
exactly one instruction removed:

```asm
  movl   %c5(%1), %k0        ; tmp  = c->range
  movl   %c2(%1), %%eax      ; low  = c->low
  shl    $17, %k0            ; range << 17
  add    %%eax, %%eax        ; low  <<= 1        <- consume one bit
  sub    %k0, %%eax          ; low - (range<<17)
  cdq                        ; edx = sign(eax) -> 0 or -1: which half we landed in
  and    %%edx, %k0          ; mask the subtraction
  add    %k0, %%eax          ; undo it if we were in the lower half
                             ; >>> stock has "inc %edx" here to turn -1/0 into 0/1 <<<
  test   %%ax, %%ax          ; look-ahead exhausted?
  jnz    1f
  mov    %c3(%1), %0         ;   refill: 2 bytes, bswap, splice into low
  subl   $0xFFFF, %%eax
  movzwl (%0), %%ecx
  bswap  %%ecx
  shrl   $15, %%ecx
  addl   %%ecx, %%eax
  cmp    %c4(%1), %0
  jge    1f
  add    $2, %c3(%1)
1:
  movl   %%eax, %c2(%1)      ; c->low = low
```

The `cdq` trick is the whole algorithm: after `sub`, `eax`'s sign bit *is* the
answer, and `cdq` broadcasts it into `edx` as an all-ones/all-zeros mask that
both undoes the subtraction on a 0-bin and (in the stock version) becomes the
return value. Dropping `inc %edx` saves one µop — roughly one cycle on Intel —
per sign bit. At ~1 sign bit per non-zero coefficient that is a small but real
and free win, and it costs nothing in correctness because the CABAC state
update is bit-identical.

`SKIP_BLOCK()` is careful about where it uses this: the escape *prefix* loop
(`while (get_cabac_bypass(CC) && j_loop < 23)`) genuinely reads its return value
as the loop condition, so that one still calls the stock primitive. Only the
sign bit and the escape *suffix* bits — which are pure state advancement — use
`consume_cabac_bypass`.

### 4.6 `get_cabac_bypass_sign_x86` — bypass bin folded into a sign application

**Source:** [x86/cabac.h:221-264](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L221-L264)
· 0.66 s aggregate, 0.8% at t1 · upstream, used by
[`decode_cabac_mb_mvd_skip`](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_cabac.c#L1645)

Same bypass mechanics as §4.5, but instead of returning 0/1 it applies the
decoded sign directly to a caller-supplied value:

```asm
  xor    %%edx, %%ecx        ; val ^= mask
  sub    %%edx, %%ecx        ; val -= mask     -> val or -val, branchlessly
```

`%ecx` is bound `"+c"(val)`, so the caller passes the magnitude in and gets the
signed result out with no separate negate-and-branch. This is the sign of a
**motion vector difference** — one of the few bypass bins in MV-only mode whose
value genuinely matters, which is exactly why it uses this variant and not
`consume_cabac_bypass`.

### 4.7 `get_cabac_bypass_x86` — the stock bypass primitive

**Source:** [x86/cabac.h:267-304](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/cabac.h#L267-L304)
· 0.53 s aggregate, ≤0.6% per run

§4.5 with the `inc %edx` still present, returning the bin in `%edx`. Only
reached from the escape-prefix loop in `SKIP_BLOCK()`, i.e. coefficients with
`|level| ≥ 15`, which are rare at surveillance bitrates — hence the tiny share.
It shows up at all only on the school clip (1080p, higher bitrate).

### 4.8 `mid_pred` — median-of-3 with cmov

**Source:** [x86/mathops.h:83-99](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/x86/mathops.h#L83-L99)
· 3.1 s aggregate

H.264 predicts a motion vector as the **component-wise median of three
neighbours** (left, above, above-right). A median of 3 written in C is two or
three unpredictable branches. Here it is six flag-setting/cmov pairs, zero
branches:

```asm
  cmp    %2, %1 \ cmovg %1, %0 \ cmovg %2, %1     ; sort a,b
  cmp    %3, %1 \ cmovl %3, %1                    ; clamp against c
  cmp    %1, %0 \ cmovg %1, %0                    ; final select
```

Callers are exactly the MV prediction sites the extractor exists for:
[h264_mvpred.h:254](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_mvpred.h#L254),
[:461](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_mvpred.h#L461),
[h264_direct.c:251](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_direct.c#L251) —
which VTune shows around it as `pred_spatial_direct_motion` (11.2 s),
`pred_motion` (2.7 s) and `pred_pskip_motion` (2.0 s).

Gated on `HAVE_I686`, which is 1 in this build.

### 4.9 `AV_COPY128` / `AV_ZERO128` — the only SIMD in the hot path

**Source:** [libavutil/x86/intreadwrite.h:34-52](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavutil/x86/intreadwrite.h#L34-L52)
· `_mm_store_si128` 5.9 s, `_mm_load_si128` 1.2 s aggregate

Not inline asm — SSE2 **intrinsics**, compiled to `movdqa`. Three one-liners:

```c
AV_COPY128(d, s)  ->  _mm_store_si128(d, _mm_load_si128(s));   // 16-byte aligned copy
AV_COPY128U(d, s) ->  movdqu variant, unaligned
AV_ZERO128(d)     ->  _mm_store_si128(d, _mm_setzero_si128()); // 16-byte clear
```

Where they run in MV-only mode:

- **`write_back_motion_list()`** —
  [h264_mvpred.h:102-113](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_mvpred.h#L102-L113).
  Copies the macroblock's 4×4 grid of motion vectors from the working cache into
  the picture's `motion_val` array: four `AV_COPY128` calls, one per row of 4
  MVs (4 × 4 bytes = 16 bytes each), plus an `AV_ZERO128` to clear the MVD cache.
  **This is the write that produces the data the extractor exports** — 1.4% of
  t1 in the call tree.
- **`fill_decode_caches()`** —
  [h264_mvpred.h:751-757](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavcodec/h264_mvpred.h#L751-L757).
  Loads the top neighbour's 4 MVs into the prediction cache in one 16-byte move,
  or zeroes them when the neighbour is unavailable.

The `AV_COPY128` calls in `h264_slice.c` (the `top_border` backup, ~30 sites) are
**pixel** work and never execute in MV-only mode.

### 4.10 `av_bswap32` / `av_bswap64`

**Source:** [libavutil/x86/bswap.h:61-74](ffmpeg/FFmpeg-8.0-custom/FFmpeg/libavutil/x86/bswap.h#L61-L74)
— a bare `__asm__("bswap %0")`.

Byte-order conversion for reading big-endian bitstream fields. Its cost lands
inside NAL parsing (`ff_h2645_extract_rbsp`, 0.49 s) and the container demuxer.
Note that the `bswap` you see inside `BRANCHLESS_GET_CABAC` (§4.1) is *not* this
function — the macro emits its own, inline, as part of the refill.