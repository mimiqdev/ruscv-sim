/*
 * Deterministic F/D reference-vector generator built on Berkeley SoftFloat 3e.
 *
 * Emits one tab-separated case per line:
 *
 *     op <TAB> rm <TAB> a <TAB> b <TAB> c <TAB> result <TAB> flags
 *
 * `op` names the operation, `rm` is RNE/RTZ/RDN/RUP/RMM (or `-` for the
 * comparisons), operands are the format-width hexadecimal bit patterns (`-`
 * when absent) and `result`/`flags` are what the RISC-V F/D instruction must
 * produce. Flags use `fflags` bit order (NV=1, DZ=2, OF=4, UF=8, NX=16).
 *
 * RISC-V adjustments applied on top of SoftFloat, both documented in
 * `tools/fpu/README.md`:
 *
 *   - NaN results of arithmetic, square root, fused multiply-add and
 *     float-to-float conversion are replaced with the canonical NaN
 *     (0x7fc00000 / 0x7ff8000000000000);
 *   - invalid float-to-integer conversions use the RISC-V saturation table
 *     (SoftFloat's platform layer instead follows the x86 SSE convention);
 *     32-bit destinations are sign-extended into the 64-bit result.
 *
 * Build and run (see `tools/fpu/gen_fpu_vectors.sh`), with the pinned
 * SoftFloat 3e tree extracted next to TestFloat 3e.
 */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include "softfloat.h"

/* Deterministic xorshift64*; no libc rand(), so output is host independent. */
static uint64_t rng;
static uint64_t next64(void)
{
    rng ^= rng << 13;
    rng ^= rng >> 7;
    rng ^= rng << 17;
    return rng * 0x2545F4914F6CDD1DULL;
}
static uint32_t next32(void) { return (uint32_t)(next64() >> 16); }

static const char *rm_name[] = {"RNE", "RTZ", "RDN", "RUP", "RMM"};
static const uint8_t rm_val[] = {
    softfloat_round_near_even, softfloat_round_minMag, softfloat_round_min,
    softfloat_round_max, softfloat_round_near_maxMag
};

/* RISC-V fflags bit order. */
static unsigned rv_flags(void)
{
    unsigned sf = softfloat_exceptionFlags, f = 0;
    if (sf & softfloat_flag_invalid)   f |= 1;
    if (sf & softfloat_flag_infinite)  f |= 2;
    if (sf & softfloat_flag_overflow)  f |= 4;
    if (sf & softfloat_flag_underflow) f |= 8;
    if (sf & softfloat_flag_inexact)   f |= 16;
    return f;
}

static void row(const char *op, const char *rm, int w,
                uint64_t a, uint64_t b, uint64_t c, uint64_t r, unsigned f)
{
    printf("%s\t%s\t", op, rm);
    /* `a` is always present; `b`/`c` use UINT64_MAX to mean "not used",
       which is safe because only immediate integer operands can equal it and
       those always occupy `a`. */
    if (w == 8) {
        printf("%08llx\t", (unsigned long long)a);
        printf(b == UINT64_MAX ? "-\t" : "%08llx\t", (unsigned long long)b);
    } else {
        printf("%016llx\t", (unsigned long long)a);
        printf(b == UINT64_MAX ? "-\t" : "%016llx\t", (unsigned long long)b);
    }
    if (c == UINT64_MAX) {
        printf("-\t");
    } else {
        if (w == 8) printf("%08llx\t", (unsigned long long)c);
        else printf("%016llx\t", (unsigned long long)c);
    }
    printf("%016llx\t%02x\n", (unsigned long long)r, f);
}

/* ------------------------------------------------------------------ */
/* Curated edge values                                                */
/* ------------------------------------------------------------------ */

/* Full list for unary, conversion and compare cases. */
static const uint32_t s32[] = {
    0x00000000, 0x80000000,             /* +0, -0 */
    0x00000001, 0x80000001,             /* min subnormal */
    0x007fffff, 0x00800000,             /* max subnormal, min normal */
    0x00800001, 0x3f7fffff,             /* min normal + ulp, 1 - ulp */
    0x3f800000, 0xbf800000,             /* +1, -1 */
    0x3f000000, 0x40000000,             /* 0.5, 2 */
    0x4b800000,                         /* 2^24 */
    0x4effffff, 0x4f000000, 0x4f800000, /* below 2^31, 2^31, 2^32 */
    0x5f000000, 0x5f800000,             /* 2^63, 2^64 */
    0x7f7fffff, 0xff7fffff,             /* +/- max finite */
    0x7f800000, 0xff800000,             /* +/- infinity */
    0x7fc00000, 0x7fc00001,             /* canonical qNaN, qNaN payload */
    0x7f800001, 0xff800001,             /* +/- signaling NaN */
    0x40490fdb, 0x3eaaaaab,             /* pi, 1/3 */
};
#define NS32 (sizeof s32 / sizeof s32[0])

static const uint64_t s64[] = {
    0x0000000000000000ULL, 0x8000000000000000ULL,
    0x0000000000000001ULL, 0x8000000000000001ULL,
    0x000fffffffffffffULL, 0x0010000000000000ULL,
    0x0010000000000001ULL, 0x3fefffffffffffffULL,
    0x3ff0000000000000ULL, 0xbff0000000000000ULL,
    0x3fe0000000000000ULL, 0x4000000000000000ULL,
    0x4340000000000000ULL,
    0x41dfffffffc00000ULL, 0x41e0000000000000ULL, 0x41f0000000000000ULL,
    0x43e0000000000000ULL, 0x43f0000000000000ULL,
    0x7fefffffffffffffULL, 0xffefffffffffffffULL,
    0x7ff0000000000000ULL, 0xfff0000000000000ULL,
    0x7ff8000000000000ULL, 0x7ff8000000000001ULL,
    0x7ff0000000000001ULL, 0xfff0000000000001ULL,
    0x400921fb54442d18ULL, 0x3fd5555555555555ULL,
};
#define NS64 (sizeof s64 / sizeof s64[0])

/* Small class-covering subset for the binary-operation matrices. */
static const unsigned b32[] = {0, 1, 2, 5, 8, 9, 11, 14, 20, 21, 22, 24, 25, 26};
static const unsigned b64[] = {0, 1, 2, 5, 8, 9, 11, 14, 20, 21, 22, 24, 25, 26};
#define NB32 (sizeof b32 / sizeof b32[0])
#define NB64 (sizeof b64 / sizeof b64[0])

static const unsigned fma32[] = {0, 2, 5, 8, 11, 14, 20, 22};
static const unsigned fma64[] = {0, 2, 5, 8, 11, 14, 20, 22};
#define NFMA32 (sizeof fma32 / sizeof fma32[0])
#define NFMA64 (sizeof fma64 / sizeof fma64[0])

/* Signed and unsigned integer operands for FCVT.{S,D}.{W,WU,L,LU}. */
static const uint64_t int_vals64[] = {
    0x0000000000000000ULL, 0x0000000000000001ULL, 0xffffffffffffffffULL,
    0x7fffffffffffffffULL, 0x8000000000000000ULL, 0x00000000ffffffffULL,
    0x000000007fffffffULL, 0x0000000080000000ULL, 0x000000007ffffffeULL,
    0x0000000080000001ULL, 0x0000000000ffffffULL, 0x0000000001000000ULL,
    0x0000000001000001ULL, 0x00000000fffffffeULL, 0x00000000ffffffffULL,
    0x0000000080000000ULL, 0x00000000c0000000ULL, 0x0000000040000000ULL,
    0x00000000000003ffULL, 0x0000000000000400ULL, 0x0000000000000401ULL,
    0x0000000000001001ULL,
};
#define NINT (sizeof int_vals64 / sizeof int_vals64[0])

/* ------------------------------------------------------------------ */
/* Biased random patterns                                             */
/* ------------------------------------------------------------------ */

static uint32_t rand_s32(void)
{
    uint32_t r = next32();
    uint32_t sign = r & 0x80000000u, mag = r & 0x7fffffffu;
    switch (r % 8) {
    case 0: return sign | (mag & 0x007fffffu);                     /* subnormal */
    case 1: return sign | (0x00800000u | (mag & 0x000fffffu));     /* min normal */
    case 2: return sign | (0x7f000000u | (mag & 0x00ffffffu));     /* huge */
    case 3: return sign | (0x7f7fffffu - (mag & 0x000fffffu));     /* near max */
    case 4: return sign | (0x3f800000u | (mag & 0x0000fffffu));    /* near 1 */
    case 5: return sign | (0x3f000000u | (mag & 0x0000fffffu));    /* near 0.5 */
    default: return r;
    }
}

static uint64_t rand_s64(void)
{
    uint64_t r = next64();
    uint64_t sign = r & 0x8000000000000000ULL, mag = r & 0x7fffffffffffffffULL;
    switch (r % 8) {
    case 0: return sign | (mag & 0x000fffffffffffffULL);
    case 1: return sign | (0x0010000000000000ULL | (mag & 0x0001ffffffffffffULL));
    case 2: return sign | (0x7fe0000000000000ULL | (mag & 0x001fffffffffffffULL));
    case 3: return sign | (0x7fefffffffffffffULL - (mag & 0x000fffffffffffffULL));
    case 4: return sign | (0x3ff0000000000000ULL | (mag & 0x0000ffffffffffffULL));
    case 5: return sign | (0x3fe0000000000000ULL | (mag & 0x0000ffffffffffffULL));
    default: return r;
    }
}

/* Random-case count per (operation, rounding mode). The committed vector set
   uses the defaults; a maintainer can compile with -DNRAND_BINARY=<n> (and
   the other two) for a deeper local differential run. */
/* Fixed PRNG seed (see README). */
#ifndef FPU_SEED
#define FPU_SEED 0x0146000000000001ULL
#endif

#ifndef NRAND_BINARY
#define NRAND_BINARY 48
#endif
#ifndef NRAND_UNARY
#define NRAND_UNARY 48
#endif
#ifndef NRAND_FMA
#define NRAND_FMA 48
#endif

/* ------------------------------------------------------------------ */
/* Arithmetic                                                         */
/* ------------------------------------------------------------------ */

static uint32_t canon32(uint32_t r)
{
    if ((r & 0x7f800000u) == 0x7f800000u && (r & 0x007fffffu))
        return 0x7fc00000u;
    return r;
}
static uint64_t canon64(uint64_t r)
{
    if ((r & 0x7ff0000000000000ULL) == 0x7ff0000000000000ULL
        && (r & 0x000fffffffffffffULL))
        return 0x7ff8000000000000ULL;
    return r;
}

static float32_t f32_op(const char *name, float32_t a, float32_t b)
{
    switch (name[4]) {
    case 'a': return f32_add(a, b);
    case 's': return f32_sub(a, b);
    case 'm': return f32_mul(a, b);
    default:  return f32_div(a, b);
    }
}
static float64_t f64_op(const char *name, float64_t a, float64_t b)
{
    switch (name[4]) {
    case 'a': return f64_add(a, b);
    case 's': return f64_sub(a, b);
    case 'm': return f64_mul(a, b);
    default:  return f64_div(a, b);
    }
}

static void gen_binary(const char *op, int w)
{
    for (int m = 0; m < 5; m++) {
        softfloat_roundingMode = rm_val[m];
        if (w == 8) {
            for (unsigned i = 0; i < NB32; i++)
                for (unsigned j = 0; j < NB32; j++) {
                    union { uint32_t ui; float32_t f; } a = {s32[b32[i]]}, b = {s32[b32[j]]}, z;
                    softfloat_exceptionFlags = 0;
                    z.f = f32_op(op, a.f, b.f);
                    row(op, rm_name[m], 8, a.ui, b.ui, UINT64_MAX, canon32(z.ui), rv_flags());
                }
            for (int k = 0; k < NRAND_BINARY; k++) {
                uint32_t x = rand_s32(), y = rand_s32();
                union { uint32_t ui; float32_t f; } a = {x}, b = {y}, z;
                softfloat_exceptionFlags = 0;
                z.f = f32_op(op, a.f, b.f);
                row(op, rm_name[m], 8, x, y, UINT64_MAX, canon32(z.ui), rv_flags());
            }
        } else {
            for (unsigned i = 0; i < NB64; i++)
                for (unsigned j = 0; j < NB64; j++) {
                    union { uint64_t ui; float64_t f; } a = {s64[b64[i]]}, b = {s64[b64[j]]}, z;
                    softfloat_exceptionFlags = 0;
                    z.f = f64_op(op, a.f, b.f);
                    row(op, rm_name[m], 16, a.ui, b.ui, UINT64_MAX, canon64(z.ui), rv_flags());
                }
            for (int k = 0; k < NRAND_BINARY; k++) {
                uint64_t x = rand_s64(), y = rand_s64();
                union { uint64_t ui; float64_t f; } a = {x}, b = {y}, z;
                softfloat_exceptionFlags = 0;
                z.f = f64_op(op, a.f, b.f);
                row(op, rm_name[m], 16, x, y, UINT64_MAX, canon64(z.ui), rv_flags());
            }
        }
    }
}

static void gen_sqrt(int w)
{
    for (int m = 0; m < 5; m++) {
        softfloat_roundingMode = rm_val[m];
        if (w == 8) {
            for (unsigned i = 0; i < NS32; i++) {
                union { uint32_t ui; float32_t f; } a = {s32[i]}, z;
                softfloat_exceptionFlags = 0;
                z.f = f32_sqrt(a.f);
                row("f32_sqrt", rm_name[m], 8, a.ui, UINT64_MAX, UINT64_MAX,
                    canon32(z.ui), rv_flags());
            }
            for (int k = 0; k < NRAND_UNARY; k++) {
                uint32_t x = rand_s32();
                union { uint32_t ui; float32_t f; } a = {x}, z;
                softfloat_exceptionFlags = 0;
                z.f = f32_sqrt(a.f);
                row("f32_sqrt", rm_name[m], 8, x, UINT64_MAX, UINT64_MAX,
                    canon32(z.ui), rv_flags());
            }
        } else {
            for (unsigned i = 0; i < NS64; i++) {
                union { uint64_t ui; float64_t f; } a = {s64[i]}, z;
                softfloat_exceptionFlags = 0;
                z.f = f64_sqrt(a.f);
                row("f64_sqrt", rm_name[m], 16, a.ui, UINT64_MAX, UINT64_MAX,
                    canon64(z.ui), rv_flags());
            }
            for (int k = 0; k < NRAND_UNARY; k++) {
                uint64_t x = rand_s64();
                union { uint64_t ui; float64_t f; } a = {x}, z;
                softfloat_exceptionFlags = 0;
                z.f = f64_sqrt(a.f);
                row("f64_sqrt", rm_name[m], 16, x, UINT64_MAX, UINT64_MAX,
                    canon64(z.ui), rv_flags());
            }
        }
    }
}

static void gen_fma(int w)
{
    for (int m = 0; m < 5; m++) {
        softfloat_roundingMode = rm_val[m];
        if (w == 8) {
            for (unsigned i = 0; i < NFMA32; i++)
                for (unsigned j = 0; j < NFMA32; j++) {
                    uint32_t cbits = s32[fma32[(i + j + (unsigned)m) % NFMA32]];
                    union { uint32_t ui; float32_t f; } a = {s32[fma32[i]]},
                        b = {s32[fma32[j]]}, c = {cbits}, z;
                    softfloat_exceptionFlags = 0;
                    z.f = f32_mulAdd(a.f, b.f, c.f);
                    row("f32_fmadd", rm_name[m], 8, a.ui, b.ui, c.ui,
                        canon32(z.ui), rv_flags());
                }
            for (int k = 0; k < NRAND_FMA; k++) {
                uint32_t x = rand_s32(), y = rand_s32(), zc = rand_s32();
                union { uint32_t ui; float32_t f; } a = {x}, b = {y}, c = {zc}, z;
                softfloat_exceptionFlags = 0;
                z.f = f32_mulAdd(a.f, b.f, c.f);
                row("f32_fmadd", rm_name[m], 8, x, y, zc, canon32(z.ui), rv_flags());
            }
            /* 0 * inf with a quiet-NaN addend must still raise NV (F 2.2). */
            {
                union { uint32_t ui; float32_t f; } a = {0x7f800000u}, b = {0x00000000u},
                    c = {0x7fc00000u}, z;
                softfloat_exceptionFlags = 0;
                z.f = f32_mulAdd(a.f, b.f, c.f);
                row("f32_fmadd", rm_name[m], 8, a.ui, b.ui, c.ui, canon32(z.ui), rv_flags());
            }
        } else {
            for (unsigned i = 0; i < NFMA64; i++)
                for (unsigned j = 0; j < NFMA64; j++) {
                    uint64_t cbits = s64[fma64[(i + j + (unsigned)m) % NFMA64]];
                    union { uint64_t ui; float64_t f; } a = {s64[fma64[i]]},
                        b = {s64[fma64[j]]}, c = {cbits}, z;
                    softfloat_exceptionFlags = 0;
                    z.f = f64_mulAdd(a.f, b.f, c.f);
                    row("f64_fmadd", rm_name[m], 16, a.ui, b.ui, c.ui,
                        canon64(z.ui), rv_flags());
                }
            for (int k = 0; k < NRAND_FMA; k++) {
                uint64_t x = rand_s64(), y = rand_s64(), zc = rand_s64();
                union { uint64_t ui; float64_t f; } a = {x}, b = {y}, c = {zc}, z;
                softfloat_exceptionFlags = 0;
                z.f = f64_mulAdd(a.f, b.f, c.f);
                row("f64_fmadd", rm_name[m], 16, x, y, zc, canon64(z.ui), rv_flags());
            }
            {
                union { uint64_t ui; float64_t f; } a = {0x7ff0000000000000ULL},
                    b = {0x0000000000000000ULL}, c = {0x7ff8000000000000ULL}, z;
                softfloat_exceptionFlags = 0;
                z.f = f64_mulAdd(a.f, b.f, c.f);
                row("f64_fmadd", rm_name[m], 16, a.ui, b.ui, c.ui,
                    canon64(z.ui), rv_flags());
            }
        }
    }
}

/* ------------------------------------------------------------------ */
/* Conversions                                                        */
/* ------------------------------------------------------------------ */

/* RISC-V saturation table applied to a SoftFloat invalid conversion. */
static uint64_t rv_int_result(uint64_t sf_result, unsigned flags, bool sign,
                              bool is_nan, int width, bool is_signed)
{
    if (flags & 1) {
        if (is_nan) sign = false;
        if (width == 32) {
            if (!is_signed) return sign ? 0 : 0xffffffffffffffffULL;
            return sign ? 0xffffffff80000000ULL : 0x000000007fffffffULL;
        }
        if (!is_signed) return sign ? 0 : 0xffffffffffffffffULL;
        return sign ? 0x8000000000000000ULL : 0x7fffffffffffffffULL;
    }
    return sf_result;
}

static void gen_float_to_int(int w)
{
    for (int m = 0; m < 5; m++) {
        softfloat_roundingMode = rm_val[m];
        if (w == 8) {
        for (unsigned i = 0; i < NS32 + NRAND_UNARY; i++) {
            uint32_t x = i < NS32 ? s32[i] : rand_s32();
            union { uint32_t ui; float32_t f; } a = {x};
            unsigned f;
            bool sign = (x >> 31) != 0;
            bool is_nan = (x & 0x7f800000u) == 0x7f800000u && (x & 0x007fffffu);
            uint64_t r;

            softfloat_exceptionFlags = 0;
            r = (uint64_t)(int64_t)f32_to_i32(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 32, true);
            row("f32_to_i32", rm_name[m], 8, x, UINT64_MAX, UINT64_MAX, r, f);

            softfloat_exceptionFlags = 0;
            r = (uint64_t)(int64_t)f32_to_i64(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 64, true);
            row("f32_to_i64", rm_name[m], 8, x, UINT64_MAX, UINT64_MAX, r, f);

            softfloat_exceptionFlags = 0;
            r = (uint64_t)(int64_t)(int32_t)f32_to_ui32(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 32, false);
            row("f32_to_ui32", rm_name[m], 8, x, UINT64_MAX, UINT64_MAX, r, f);

            softfloat_exceptionFlags = 0;
            r = (uint64_t)f32_to_ui64(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 64, false);
            row("f32_to_ui64", rm_name[m], 8, x, UINT64_MAX, UINT64_MAX, r, f);
        }
        } else {
        for (unsigned i = 0; i < NS64 + NRAND_UNARY; i++) {
            uint64_t x = i < NS64 ? s64[i] : rand_s64();
            union { uint64_t ui; float64_t f; } a = {x};
            unsigned f;
            bool sign = (x >> 63) != 0;
            bool is_nan = (x & 0x7ff0000000000000ULL) == 0x7ff0000000000000ULL
                          && (x & 0x000fffffffffffffULL);
            uint64_t r;

            softfloat_exceptionFlags = 0;
            r = (uint64_t)(int64_t)f64_to_i32(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 32, true);
            row("f64_to_i32", rm_name[m], 16, x, UINT64_MAX, UINT64_MAX, r, f);

            softfloat_exceptionFlags = 0;
            r = (uint64_t)(int64_t)f64_to_i64(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 64, true);
            row("f64_to_i64", rm_name[m], 16, x, UINT64_MAX, UINT64_MAX, r, f);

            softfloat_exceptionFlags = 0;
            r = (uint64_t)(int64_t)(int32_t)f64_to_ui32(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 32, false);
            row("f64_to_ui32", rm_name[m], 16, x, UINT64_MAX, UINT64_MAX, r, f);

            softfloat_exceptionFlags = 0;
            r = (uint64_t)f64_to_ui64(a.f, softfloat_roundingMode, true);
            f = rv_flags();
            r = rv_int_result(r, f, sign, is_nan, 64, false);
            row("f64_to_ui64", rm_name[m], 16, x, UINT64_MAX, UINT64_MAX, r, f);
        }
        }
    }
}

static void gen_int_to_float(int w)
{
    for (int m = 0; m < 5; m++) {
        softfloat_roundingMode = rm_val[m];
        for (unsigned i = 0; i < NINT; i++) {
            uint64_t bits = int_vals64[i];
            int32_t si32 = (int32_t)(uint32_t)bits;
            uint32_t ui32 = (uint32_t)bits;
            int64_t si64 = (int64_t)bits;
            union { uint32_t ui; float32_t f; } z32;
            union { uint64_t ui; float64_t f; } z64;

            if (w == 8) {
                softfloat_exceptionFlags = 0;
                z32.f = i32_to_f32(si32);
                row("i32_to_f32", rm_name[m], 8, bits, UINT64_MAX, UINT64_MAX, z32.ui, rv_flags());
                softfloat_exceptionFlags = 0;
                z32.f = ui32_to_f32(ui32);
                row("ui32_to_f32", rm_name[m], 8, bits, UINT64_MAX, UINT64_MAX, z32.ui, rv_flags());
                softfloat_exceptionFlags = 0;
                z32.f = i64_to_f32(si64);
                row("i64_to_f32", rm_name[m], 8, bits, UINT64_MAX, UINT64_MAX, z32.ui, rv_flags());
                softfloat_exceptionFlags = 0;
                z32.f = ui64_to_f32(bits);
                row("ui64_to_f32", rm_name[m], 8, bits, UINT64_MAX, UINT64_MAX, z32.ui, rv_flags());
            } else {
                softfloat_exceptionFlags = 0;
                z64.f = i32_to_f64(si32);
                row("i32_to_f64", rm_name[m], 16, bits, UINT64_MAX, UINT64_MAX, z64.ui, rv_flags());
                softfloat_exceptionFlags = 0;
                z64.f = ui32_to_f64(ui32);
                row("ui32_to_f64", rm_name[m], 16, bits, UINT64_MAX, UINT64_MAX, z64.ui, rv_flags());
                softfloat_exceptionFlags = 0;
                z64.f = i64_to_f64(si64);
                row("i64_to_f64", rm_name[m], 16, bits, UINT64_MAX, UINT64_MAX, z64.ui, rv_flags());
                softfloat_exceptionFlags = 0;
                z64.f = ui64_to_f64(bits);
                row("ui64_to_f64", rm_name[m], 16, bits, UINT64_MAX, UINT64_MAX, z64.ui, rv_flags());
            }
        }
    }
}

static void gen_float_to_float(void)
{
    for (int m = 0; m < 5; m++) {
        softfloat_roundingMode = rm_val[m];
        for (unsigned i = 0; i < NS32 + NRAND_UNARY; i++) {
            uint32_t x = i < NS32 ? s32[i] : rand_s32();
            union { uint32_t ui; float32_t f; } a = {x};
            union { uint64_t ui; float64_t f; } z;
            softfloat_exceptionFlags = 0;
            z.f = f32_to_f64(a.f);
            row("f32_to_f64", rm_name[m], 8, x, UINT64_MAX, UINT64_MAX,
                canon64(z.ui), rv_flags());
        }
        for (unsigned i = 0; i < NS64 + NRAND_UNARY; i++) {
            uint64_t x = i < NS64 ? s64[i] : rand_s64();
            union { uint64_t ui; float64_t f; } a = {x};
            union { uint32_t ui; float32_t f; } z;
            softfloat_exceptionFlags = 0;
            z.f = f64_to_f32(a.f);
            row("f64_to_f32", rm_name[m], 16, x, UINT64_MAX, UINT64_MAX,
                canon32(z.ui), rv_flags());
        }
    }
}

/* ------------------------------------------------------------------ */
/* Comparisons                                                        */
/* ------------------------------------------------------------------ */

static void gen_compare(const char *op, int w)
{
    unsigned nb = w == 8 ? NB32 : NB64;
    for (unsigned i = 0; i < nb; i++)
        for (unsigned j = 0; j < nb; j++) {
            uint64_t x = w == 8 ? s32[b32[i]] : s64[b64[i]];
            uint64_t y = w == 8 ? s32[b32[j]] : s64[b64[j]];
            bool r;
            softfloat_exceptionFlags = 0;
            if (w == 8) {
                union { uint32_t ui; float32_t f; } a = {(uint32_t)x}, b = {(uint32_t)y};
                if (op[4] == 'e') r = f32_eq(a.f, b.f);
                else if (op[4] == 'l' && op[5] == 't') r = f32_lt(a.f, b.f);
                else r = f32_le(a.f, b.f);
            } else {
                union { uint64_t ui; float64_t f; } a = {x}, b = {y};
                if (op[4] == 'e') r = f64_eq(a.f, b.f);
                else if (op[4] == 'l' && op[5] == 't') r = f64_lt(a.f, b.f);
                else r = f64_le(a.f, b.f);
            }
            row(op, "-", w, x, y, UINT64_MAX, r ? 1 : 0, rv_flags());
        }
}

int main(void)
{
    softfloat_detectTininess = softfloat_tininess_afterRounding;
    rng = FPU_SEED;

    gen_binary("f32_add", 8);
    gen_binary("f32_sub", 8);
    gen_binary("f32_mul", 8);
    gen_binary("f32_div", 8);
    gen_binary("f64_add", 16);
    gen_binary("f64_sub", 16);
    gen_binary("f64_mul", 16);
    gen_binary("f64_div", 16);
    gen_sqrt(8);
    gen_sqrt(16);
    gen_fma(8);
    gen_fma(16);
    gen_float_to_int(8);
    gen_float_to_int(16);
    gen_int_to_float(8);
    gen_int_to_float(16);
    gen_float_to_float();
    gen_compare("f32_eq", 8);
    gen_compare("f32_lt", 8);
    gen_compare("f32_le", 8);
    gen_compare("f64_eq", 16);
    gen_compare("f64_lt", 16);
    gen_compare("f64_le", 16);
    return 0;
}
