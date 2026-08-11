#!/usr/bin/env python3
"""S3 planted rung-2-forcing corpus generator (fd-4zo.20, ADR-0059).

Plants Decimal128 inputs whose true results sit at CHOSEN distances
from a format rounding boundary, so the escalation ladder's rung 2
receives natural, Arb-certified traffic instead of only the
`force_escalate` differential. Every emitted row is forward-certified
by `gen_transcend_vectors.solve` (the same Arb enclosure discipline as
the sampled corpus) and asserted canonically representable.

## Why d128 only

The escalation predicate's unit is one unit in the 50th significant
digit of the working value (`near_rounding_boundary`), so a rung 1
budget B escalates inputs whose true result lies within

    t = B x 10^(P - 50)   fractional ULP of the format

of a grid point or midpoint. At P = 34 the per-op thresholds t span
~1e-13 .. 3e-2 and are reachable by choosing the input's trailing
digits. At P = 16 and P = 7 the same formula gives t <= ~1e-20:
below the resolution the format's own coefficient lattice can
express (min achievable distance ~ 1/|lattice| ~ 1e-15). The sibling
formats' entire input spaces therefore sit inside rung 1's
competence, which the telemetry test pins as `rung2_entries == 0`
over their whole corpora. Planting is a Decimal128 activity.

## Grades and the soundness of the contract

Per operation the generator plants, at each of two boundary-family
seeds (one midpoint-targeted, one grid-targeted):

    control  distance in [ 8t, 40t]  -- must NOT escalate
    entry    distance in [t/40, t/8] -- must escalate
    deep     distance in [max(t/4000, 1.5e-14),
                          max(t/800,  7.5e-14)]  -- must escalate

The contract survives the whole budget pad being consumed: the
predicate tests the RUNG 1 COMPUTED value, which differs from the
true value by the real rung 1 error e1 <= B units (the budgets are
x10-padded bounds, observed error is under a tenth of B). Even at
e1 = B: an entry row's computed distance is <= t/8 + t < 2t...
strictly: <= t/8 + e1(in ULP) = t/8 + t, and the predicate compares
against t, so escalation needs computed distance < t. With the
documented pad (e1 <= t/10): entry <= t/8 + t/10 < t escalates,
control >= 8t - t/10 > t does not. The deep floor 1.5e-14 keeps the
Ostrowski search inside Python-feasible window counts; for
loose-budget ops the deep grade is t/4000.

## The planter

Boundary inversion at 300 dps (mpmath) picks a base input near a
chosen output boundary; the last-digit lattice x + k*ulp is then
searched for the k whose output distance lands in the grade band.
Distances follow the exact quadratic model

    d(k) = d0 + k*s + k^2*q  (mod 1, fractional ULP of the output)

whose cubic remainder over the full |k| <= ~1e14 range is < 1e-25
fractional ULP (|k*ulp_x| <= 1e-19 relative, times |f'''/f|/6 ~ O(1),
over ulp_y ~ 1e-33 relative). The search tiles k into windows small
enough that within one window the model is affine, and runs an exact
integer Ostrowski (three-distance) argmin per window at 1e-40
resolution. A found k is verified by a true 300-dps evaluation before
certification; model and truth agreeing is asserted, not assumed.

## What is deliberately NOT planted (no silent caps)

* `rsqrt`, `hypot`, and `powi`'s |n| <= 6 powering arm: rung 1
  budgets 400 / 250 / 200 units put t at ~2e-14 .. 4e-14 and the
  ENTRY grade below the search's feasible floor. These are the
  ADR-0060 adjudicated operations: every battery already routes
  their whole corpus through the boundary machinery
  (`force_adjudicate`), which is stronger coverage than planting.
  `powi`/`rootn`/`compound` ARE planted at n = 11, outside the
  adjudicable ranges, exercising their plain-ladder deliveries.
* `powr`: no corpus replay harness exists for it (it is
  differential-tested against `pow`, whose planted rows exercise the
  shared kernel body); its divergences from `pow` are special-value
  rows, not near-boundary deliveries.
* `sqrt`: not a ladder operation (format-native §5 rounder).
* Large-magnitude trig: the S1 witness corpus already pins the
  reduction thin spot with 1,819 certified misround witnesses;
  planted trig rows target the moderate band the witnesses do not.

## Regeneration

    tools/gen_planted_hardcases.py [--selftest] [--ops sin,exp,...]

Budgets are parsed from `ferrodec-transcend/src/ladder.rs` at run
time (no second copy of the constants to drift); the parsed table is
recorded in each .prov header. After a budget change, rerun and diff:
each .prov row carries `planted=` (the signed min-family distance in
fractional ULP) so re-deriving which rows sit under a NEW threshold
is mechanical, and the telemetry pins in
`tests/ladder_telemetry.rs` are re-authored from the regenerated
margins. Requires python-flint (Arb) and mpmath.
"""

import os
import re
import sys
from fractions import Fraction

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_transcend_vectors as gtv  # noqa: E402

from mpmath import mp, mpf  # noqa: E402
import mpmath  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LADDER_RS = os.path.join(ROOT, "ferrodec-transcend", "src", "ladder.rs")
OUT_DIR = os.path.join(ROOT, "tests", "vectors", "transcend", "planted")

FMT = gtv.FORMATS["d128"]
P = FMT["prec"]  # 34
MODES = ["NearestEven", "NearestAway", "TowardZero", "TowardPositive",
         "TowardNegative"]

# Exact integer scale for the Ostrowski search: distances and slopes
# are represented as integers in units of 1e-SCALE_DIGITS fractional
# ULP. 40 digits leaves ~1e-26 of slack under the deepest grade floor
# (1.5e-14) across the largest |k| the search reaches (~1e14).
SCALE_DIGITS = 40
SCALE = 10 ** SCALE_DIGITS

mp.dps = 300


# --- rung 1 budget table, parsed from ladder.rs (single source) ---

def parse_rung1_budgets():
    src = open(LADDER_RS).read()
    pat = re.compile(
        r"pub\(crate\) const ([A-Z0-9_]+): Budget = Budget \{\s*"
        r"rung1: ([0-9_]+),", re.S)
    out = {}
    for name, val in pat.findall(src):
        out[name] = int(val.replace("_", ""))
    if len(out) < 30:
        raise SystemExit("budget parse found only %d entries" % len(out))
    return out


# --- operation table ---
#
# fwd/inv/d1/d2 are mpmath callables (300 dps): the function, its
# inverse (used once, for the base point), and its first and second
# derivatives (the exact linear and quadratic model coefficients).
# x0: two base seeds, one per boundary family (midpoint, grid).
# budget: the ladder.rs constant name whose rung1 sets the threshold.

def OPS(b):
    ln2 = mp.log(2)
    ln10 = mp.log(10)
    pi = mp.pi

    def U(budget, fwd, inv, d1, d2, x0):
        return dict(kind="unary", budget=budget, fwd=fwd, inv=inv,
                    d1=d1, d2=d2, x0=x0)

    ops = {
        "exp": U("EXP", mp.exp, mp.log, mp.exp, mp.exp,
                 [mpf("1.234567"), mpf("2.171828")]),
        "exp2": U("EXP2", lambda x: 2 ** mpf(x),
                  lambda y: mp.log(y) / ln2,
                  lambda x: ln2 * 2 ** mpf(x),
                  lambda x: ln2 ** 2 * 2 ** mpf(x),
                  [mpf("1.318"), mpf("2.707")]),
        "exp10": U("EXP10", lambda x: 10 ** mpf(x),
                   lambda y: mp.log(y) / ln10,
                   lambda x: ln10 * 10 ** mpf(x),
                   lambda x: ln10 ** 2 * 10 ** mpf(x),
                   [mpf("0.87"), mpf("1.443")]),
        "expm1": U("EXPM1", mp.expm1, mp.log1p, mp.exp, mp.exp,
                   [mpf("0.913"), mpf("1.727")]),
        "exp2m1": U("EXP2M1", lambda x: 2 ** mpf(x) - 1,
                    lambda y: mp.log2(1 + mpf(y)),
                    lambda x: ln2 * 2 ** mpf(x),
                    lambda x: ln2 ** 2 * 2 ** mpf(x),
                    [mpf("1.207"), mpf("2.313")]),
        "exp10m1": U("EXP10M1", lambda x: 10 ** mpf(x) - 1,
                     lambda y: mp.log10(1 + mpf(y)),
                     lambda x: ln10 * 10 ** mpf(x),
                     lambda x: ln10 ** 2 * 10 ** mpf(x),
                     [mpf("0.813"), mpf("1.531")]),
        "ln": U("LN", mp.log, mp.exp, lambda x: 1 / mpf(x),
                lambda x: -1 / mpf(x) ** 2,
                [mpf("2.34"), mpf("7.13")]),
        "log2": U("LOG2", lambda x: mp.log(x) / ln2,
                  lambda y: 2 ** mpf(y),
                  lambda x: 1 / (mpf(x) * ln2),
                  lambda x: -1 / (mpf(x) ** 2 * ln2),
                  [mpf("2.93"), mpf("5.31")]),
        "log10": U("LOG10", lambda x: mp.log(x) / ln10,
                   lambda y: 10 ** mpf(y),
                   lambda x: 1 / (mpf(x) * ln10),
                   lambda x: -1 / (mpf(x) ** 2 * ln10),
                   [mpf("3.71"), mpf("8.23")]),
        "logp1": U("LOGP1", lambda x: mp.log1p(mpf(x)), mp.expm1,
                   lambda x: 1 / (1 + mpf(x)),
                   lambda x: -1 / (1 + mpf(x)) ** 2,
                   [mpf("1.91"), mpf("4.27")]),
        "log2p1": U("LOG2P1",
                    lambda x: mp.log1p(mpf(x)) / ln2,
                    lambda y: 2 ** mpf(y) - 1,
                    lambda x: 1 / ((1 + mpf(x)) * ln2),
                    lambda x: -1 / ((1 + mpf(x)) ** 2 * ln2),
                    [mpf("2.23"), mpf("5.87")]),
        "log10p1": U("LOG10P1",
                     lambda x: mp.log1p(mpf(x)) / ln10,
                     lambda y: 10 ** mpf(y) - 1,
                     lambda x: 1 / ((1 + mpf(x)) * ln10),
                     lambda x: -1 / ((1 + mpf(x)) ** 2 * ln10),
                     [mpf("3.17"), mpf("7.79")]),
        "sin": U("SIN", mp.sin, mp.asin, mp.cos,
                 lambda x: -mp.sin(x),
                 [mpf("0.731"), mpf("1.213")]),
        "cos": U("COS", mp.cos, mp.acos, lambda x: -mp.sin(x),
                 lambda x: -mp.cos(x),
                 [mpf("0.733"), mpf("1.217")]),
        "tan": U("TAN", mp.tan, mp.atan,
                 lambda x: 1 / mp.cos(x) ** 2,
                 lambda x: 2 * mp.tan(x) / mp.cos(x) ** 2,
                 [mpf("0.737"), mpf("1.219")]),
        "asin": U("ASIN", mp.asin, mp.sin,
                  lambda x: 1 / mp.sqrt(1 - mpf(x) ** 2),
                  lambda x: mpf(x) / mp.sqrt(1 - mpf(x) ** 2) ** 3,
                  [mpf("0.613"), mpf("0.877")]),
        "acos": U("ACOS", mp.acos, mp.cos,
                  lambda x: -1 / mp.sqrt(1 - mpf(x) ** 2),
                  lambda x: -mpf(x) / mp.sqrt(1 - mpf(x) ** 2) ** 3,
                  [mpf("0.317"), mpf("0.571")]),
        "atan": U("ATAN", mp.atan, mp.tan,
                  lambda x: 1 / (1 + mpf(x) ** 2),
                  lambda x: -2 * mpf(x) / (1 + mpf(x) ** 2) ** 2,
                  [mpf("0.837"), mpf("1.771")]),
        "sinh": U("SINH", mp.sinh, mp.asinh, mp.cosh, mp.sinh,
                  [mpf("1.117"), mpf("2.313")]),
        "cosh": U("COSH", mp.cosh, mp.acosh, mp.sinh, mp.cosh,
                  [mpf("1.313"), mpf("2.117")]),
        "tanh": U("TANH", mp.tanh, mp.atanh,
                  lambda x: 1 / mp.cosh(x) ** 2,
                  lambda x: -2 * mp.tanh(x) / mp.cosh(x) ** 2,
                  [mpf("0.713"), mpf("1.131")]),
        "asinh": U("ASINH", mp.asinh, mp.sinh,
                   lambda x: 1 / mp.sqrt(1 + mpf(x) ** 2),
                   lambda x: -mpf(x) / mp.sqrt(1 + mpf(x) ** 2) ** 3,
                   [mpf("1.713"), mpf("3.317")]),
        "acosh": U("ACOSH", mp.acosh, mp.cosh,
                   lambda x: 1 / mp.sqrt(mpf(x) ** 2 - 1),
                   lambda x: -mpf(x) / mp.sqrt(mpf(x) ** 2 - 1) ** 3,
                   [mpf("1.913"), mpf("3.137")]),
        "atanh": U("ATANH", mp.atanh, mp.tanh,
                   lambda x: 1 / (1 - mpf(x) ** 2),
                   lambda x: 2 * mpf(x) / (1 - mpf(x) ** 2) ** 2,
                   [mpf("0.531"), mpf("0.813")]),
        # Pi-scaled family (ADR-0061). cosPi shares SINPI's budget
        # (one kernel body); base points sit inside the principal
        # octant, away from the classifier's rational sets and (for
        # tanpi) the 1/4 anchor neighborhood.
        "sinpi": U("SINPI", lambda x: mp.sinpi(x),
                   lambda y: mp.asin(y) / pi,
                   lambda x: pi * mp.cospi(x),
                   lambda x: -pi ** 2 * mp.sinpi(x),
                   [mpf("0.171"), mpf("0.293")]),
        "cospi": U("SINPI", lambda x: mp.cospi(x),
                   lambda y: mp.acos(y) / pi,
                   lambda x: -pi * mp.sinpi(x),
                   lambda x: -pi ** 2 * mp.cospi(x),
                   [mpf("0.173"), mpf("0.311")]),
        "tanpi": U("TANPI", lambda x: mp.sinpi(x) / mp.cospi(x),
                   lambda y: mp.atan(y) / pi,
                   lambda x: pi / mp.cospi(x) ** 2,
                   lambda x: 2 * pi ** 2 * mp.sinpi(x) / mp.cospi(x) ** 3,
                   [mpf("0.131"), mpf("0.187")]),
        "asinpi": U("ASINPI", lambda x: mp.asin(x) / pi,
                    lambda y: mp.sin(pi * mpf(y)),
                    lambda x: 1 / (pi * mp.sqrt(1 - mpf(x) ** 2)),
                    lambda x: mpf(x) / (pi * mp.sqrt(1 - mpf(x) ** 2) ** 3),
                    [mpf("0.537"), mpf("0.791")]),
        "acospi": U("ACOSPI", lambda x: mp.acos(x) / pi,
                    lambda y: mp.cos(pi * mpf(y)),
                    lambda x: -1 / (pi * mp.sqrt(1 - mpf(x) ** 2)),
                    lambda x: -mpf(x) / (pi * mp.sqrt(1 - mpf(x) ** 2) ** 3),
                    [mpf("0.293"), mpf("0.613")]),
        "atanpi": U("ATANPI", lambda x: mp.atan(x) / pi,
                    lambda y: mp.tan(pi * mpf(y)),
                    lambda x: 1 / (pi * (1 + mpf(x) ** 2)),
                    lambda x: -2 * mpf(x) / (pi * (1 + mpf(x) ** 2) ** 2),
                    [mpf("0.871"), mpf("1.931")]),
    }

    # Binary surface, planted in one operand with the other fixed at
    # an exactly representable constant. Replay order mirrors
    # tests/transcend_vectors.rs: pow(input1=x planted, input2=y
    # fixed); atan2/atan2pi(input1=x fixed, input2=y planted, self=y).
    POW_Y = mpf("2.718281828459045")  # 16 digits, exactly representable
    A2_X = mpf("1.25")

    ops["pow"] = dict(
        kind="binary_x", budget="POW", fixed="2.718281828459045",
        fwd=lambda x: mpf(x) ** POW_Y,
        inv=lambda y: mp.exp(mp.log(y) / POW_Y),
        d1=lambda x: POW_Y * mpf(x) ** (POW_Y - 1),
        d2=lambda x: POW_Y * (POW_Y - 1) * mpf(x) ** (POW_Y - 2),
        x0=[mpf("1.713"), mpf("3.917")])
    ops["atan2"] = dict(
        kind="binary_y", budget="ATAN2", fixed="1.25",
        fwd=lambda y: mp.atan2(mpf(y), A2_X),
        inv=lambda th: A2_X * mp.tan(th),
        d1=lambda y: A2_X / (A2_X ** 2 + mpf(y) ** 2),
        d2=lambda y: -2 * A2_X * mpf(y) / (A2_X ** 2 + mpf(y) ** 2) ** 2,
        x0=[mpf("0.913"), mpf("2.317")])
    ops["atan2pi"] = dict(
        kind="binary_y", budget="ATAN2PI", fixed="1.25",
        fwd=lambda y: mp.atan2(mpf(y), A2_X) / pi,
        inv=lambda th: A2_X * mp.tan(pi * mpf(th)),
        d1=lambda y: A2_X / (pi * (A2_X ** 2 + mpf(y) ** 2)),
        d2=lambda y: -2 * A2_X * mpf(y) / (pi * (A2_X ** 2 + mpf(y) ** 2) ** 2),
        x0=[mpf("0.917"), mpf("2.331")])

    # Integer-operand trio at n = 11: outside every ADR-0060
    # adjudicable range, so these exercise the plain guarded ladder.
    N = 11
    ops["powi"] = dict(
        kind="int", budget="POWI", n=N,
        fwd=lambda x: mpf(x) ** N,
        inv=lambda y: mpf(y) ** (mpf(1) / N),
        d1=lambda x: N * mpf(x) ** (N - 1),
        d2=lambda x: N * (N - 1) * mpf(x) ** (N - 2),
        x0=[mpf("1.317"), mpf("2.713")])
    ops["rootn"] = dict(
        kind="int", budget="ROOTN", n=N,
        fwd=lambda x: mpf(x) ** (mpf(1) / N),
        inv=lambda y: mpf(y) ** N,
        d1=lambda x: mpf(x) ** (mpf(1) / N - 1) / N,
        d2=lambda x: (mpf(1) / N) * (mpf(1) / N - 1) * mpf(x) ** (mpf(1) / N - 2),
        x0=[mpf("4321.7"), mpf("698741.3")])
    ops["compound"] = dict(
        kind="int", budget="COMPOUND", n=N,
        fwd=lambda x: (1 + mpf(x)) ** N,
        inv=lambda y: mpf(y) ** (mpf(1) / N) - 1,
        d1=lambda x: N * (1 + mpf(x)) ** (N - 1),
        d2=lambda x: N * (N - 1) * (1 + mpf(x)) ** (N - 2),
        x0=[mpf("0.371"), mpf("1.731")])

    for name, op in ops.items():
        if op["budget"] not in b:
            raise SystemExit("no budget constant %s (op %s)"
                             % (op["budget"], name))
    return ops


# --- decimal lattice helpers ---

def to_parts(x):
    """Round positive mpf x to exactly P significant digits; return
    (coef, exp) with coef exactly P digits (no trailing-zero strip:
    the planted lattice IS the P-digit lattice)."""
    assert x > 0
    e = int(mp.floor(mp.log10(x)))
    # coef = round(x * 10^(P-1-e)) with a correction loop for the
    # floor(log10) edge.
    scale = mpf(10) ** (P - 1 - e)
    coef = int(mp.nint(x * scale))
    while coef >= 10 ** P:
        coef //= 10  # crossed a decade upward
        e += 1
    while coef < 10 ** (P - 1):
        e -= 1
        coef = int(mp.nint(x * mpf(10) ** (P - 1 - e)))
    return coef, e - (P - 1)


def parts_to_mpf(coef, exp):
    return mpf(coef) * mpf(10) ** exp


def ulp_exp_of(y):
    """Exponent of one ULP at P digits for magnitude |y|."""
    e = int(mp.floor(mp.log10(abs(y))))
    return e - (P - 1)


def dist_to_boundary(y):
    """Signed distances of y to the nearest P-digit grid point and
    midpoint, in fractional ULP: (d_grid, d_mid, ulp_exp). Positive
    means y is above the boundary."""
    ue = ulp_exp_of(y)
    z = y * mpf(10) ** (-ue)  # y in ULP units, magnitude ~ 1e33..1e34
    zf = mp.floor(z)
    frac = z - zf
    d_grid = frac if frac <= mpf("0.5") else frac - 1
    d_mid = frac - mpf("0.5")
    return d_grid, d_mid, ue


# --- exact integer Ostrowski argmin over lattice windows ---

def cf_levels(s, kmax):
    """Continued-fraction levels of s/SCALE: [(q_i, signed residue)]
    with q increasing, |residue| decreasing, q_i <= kmax; the signed
    residue is q_i*s mod SCALE reduced to (-SCALE/2, SCALE/2]."""
    M = SCALE
    levels = []
    r_prev, r_cur = M, s
    q_prev, q_cur = 0, 1
    i = 0
    while r_cur > 0 and q_cur <= kmax:
        levels.append((q_cur, r_cur if i % 2 == 0 else -r_cur))
        a = r_prev // r_cur
        r_prev, r_cur = r_cur, r_prev - a * r_cur
        q_prev, q_cur = q_cur, q_prev + a * q_cur
        i += 1
    return levels


def ostrowski_band(c, s, kmax, lo, hi):
    """Find integer 0 <= k <= kmax with lo <= |c + k*s mod M| <= hi
    (M = SCALE; c reduced to (-M/2, M/2]). Runs the coarse-to-fine
    Ostrowski greedy descent toward the minimum, checking EVERY
    visited value for band membership (the descent visits ~one value
    per CF level, and for deep bands the hit arrives when a window's
    near-minimum lands in the band by equidistribution across the
    caller's window tiling). When a level's residue is finer than the
    band width and the current value sits above the band with the
    right sign, single steps walk |v| down through the band -- the
    shallow-band guaranteed hit. Returns (k, v) exact, or None."""
    M = SCALE
    s %= M
    if s == 0:
        return None
    if lo <= abs(c) <= hi:
        return 0, c
    width = hi - lo
    k, v, budget = 0, c, kmax
    for (q, th) in cf_levels(s, kmax):
        if budget < q:
            break
        if abs(v) < lo and abs(th) <= width:
            # Overshot below the band: grow |v| back into it with a
            # band-fine level stepping AWAY from zero (v == 0 accepts
            # either sign; CF signs alternate so one arrives).
            if v == 0 or (v > 0) == (th > 0):
                m = (lo - abs(v)) // abs(th) + 1
                if budget >= m * q:
                    k += m * q
                    v += m * th
                    budget -= m * q
        if lo <= abs(v) <= hi:
            return k, v
        if abs(v) > hi and (v > 0) != (th > 0):
            if abs(th) <= width:
                # Fine enough to land inside -- but only if the walk
                # actually fits the budget. A near-rational slope puts
                # a giant CF quotient here (th collapses many orders
                # below v); walking then burns the whole budget on an
                # unreachable staircase, so skip and let coarser
                # levels / other windows / the base nudge handle it.
                m_need = (abs(v) - hi) // abs(th)
                if m_need > budget // q:
                    continue
                m = m_need
                k += m * q
                v += m * th
                budget -= m * q
                while budget >= q and abs(v) > hi:
                    k += q
                    v += th
                    budget -= q
                if lo <= abs(v) <= hi:
                    return k, v
            else:
                m = min(abs(v) // abs(th), budget // q)
                if m < budget // q and \
                        abs(v + (m + 1) * th) < abs(v + m * th):
                    m += 1
                k += m * q
                v += m * th
                budget -= m * q
        if lo <= abs(v) <= hi:
            return k, v
    return None


def search_band(d0_int, s_int, q_int, kmax, lo_int, hi_int):
    """Find integer k, |k| <= kmax, with lo <= |d0 + k*s + k^2*q| <= hi
    (integer units of 1e-SCALE fractional ULP). Tiles k into windows
    small enough that the in-window m^2*q term stays under lo/4 (the
    cross term rides in the per-window slope), and runs the exact
    integer Ostrowski band walk in both lattice directions per
    window. Returns (k, model_val) or None."""
    import math as _math
    M = SCALE
    if q_int != 0:
        W = max(4096, _math.isqrt(max(1, lo_int // (4 * abs(q_int)))))
    else:
        W = kmax
    W = min(W, max(1, kmax))
    n_windows = max(1, (kmax + W - 1) // W)
    # Cap the tiling walk: the guaranteed-hit structure means the
    # first few windows nearly always land; a cap keeps a degenerate
    # slope from spinning (logged by the caller as a planting miss).
    for j in range(min(n_windows, 500_000)):
        for sign in (1, -1):
            base = sign * j * W
            c_j = (d0_int + base * s_int + base * base * q_int) % M
            if c_j > M // 2:
                c_j -= M
            s_j = (s_int + 2 * base * q_int) % M
            for mdir in (1, -1):
                sj = (mdir * s_j) % M
                got = ostrowski_band(c_j, sj, W - 1, lo_int, hi_int)
                if got is not None:
                    k_off, val = got
                    k = base + mdir * k_off
                    if abs(k) <= kmax:
                        return k, val
            if j == 0:
                break  # base 0: both signs identical
    return None


def plant_one(op, name, x0, family, lo, hi):
    """Plant one input for `op` near boundary family `family`
    ("mid" | "grid") with |true distance| in [lo, hi] fractional ULP.
    Returns (coef, exp, planted_dist_fracupl) or None."""
    f, d1, d2 = op["fwd"], op["d1"], op["d2"]
    y0 = f(x0)
    assert y0 > 0, (name, x0)
    # base x on the lattice near the inverse of a nearby boundary
    coef, exp = to_parts(x0)
    ulp_x = mpf(10) ** exp

    for attempt in range(24):
        x = parts_to_mpf(coef, exp)
        y = f(x)
        d_grid, d_mid, ue = dist_to_boundary(y)
        d = d_mid if family == "mid" else d_grid
        ulp_y = mpf(10) ** ue
        adist = abs(d)
        if lo <= adist <= hi:
            return coef, exp, d
        # exact model coefficients in integer 1e-SCALE units
        s = d1(x) * ulp_x / ulp_y
        q = d2(x) * ulp_x * ulp_x / (2 * ulp_y)
        d_int = int(mp.nint(d * SCALE))
        s_int = int(mp.nint((s - mp.floor(s)) * SCALE))
        q_int = int(mp.nint(q * SCALE))
        lo_int, hi_int = int(lo * SCALE), int(hi * SCALE)
        # target the geometric middle of the band; cap k by both the
        # model validity and the coefficient's trailing-digit room
        kmax = min(10 ** 14, coef - 10 ** (P - 1), 10 ** P - 1 - coef)
        found = search_band(d_int, s_int, q_int, kmax,
                            lo_int, hi_int)
        if found is None:
            # Nudge the base point and retry. The jump lands in the
            # coefficient's middle digits: a last-digit nudge leaves a
            # near-rational slope's CF wall intact (the atanh 0.813
            # lesson: 3-digit seeds make algebraic derivatives
            # low-denominator rationals), while a ~1e-9-relative shift
            # re-rolls the slope's CF structure entirely.
            coef += (10 ** 25 + 10 ** 19 + 9973) * (attempt + 1)
            continue
        k, _model = found
        coef2 = coef + k
        if not (10 ** (P - 1) <= coef2 < 10 ** P):
            coef += (10 ** 25 + 10 ** 19 + 9973) * (attempt + 1)
            continue
        # verify with truth; accept only in band
        x2 = parts_to_mpf(coef2, exp)
        y2 = f(x2)
        g2, m2, _ = dist_to_boundary(y2)
        d2v = m2 if family == "mid" else g2
        if lo <= abs(d2v) <= hi:
            return coef2, exp, d2v
        # model/truth drift: restart the loop from the new point
        coef = coef2
    return None


# --- certification + emission ---

def certify(name, op, coef, exp, mode):
    """Route to the right gtv solver; returns (line_txt, line_prov)
    or raises."""
    if op["kind"] == "unary":
        r = gtv.solve(name, gtv.FUNCS[name], coef, exp, False, FMT,
                      mode=mode)
        if r is None:
            raise RuntimeError("solve None: %s %s %se%d"
                               % (name, mode, coef, exp))
        out, pbits, rad, margin = r
        inp = "%de%d" % (coef, exp)
        return ("34 %s %s %s" % (mode, inp, out),
                "34 %s %s P=%d rad=%.3e margin=%.3e decisive"
                % (mode, inp, pbits, rad, margin))
    if op["kind"] in ("binary_x", "binary_y"):
        fixed = op["fixed"]
        fc, fe = parse_dec(fixed)
        if op["kind"] == "binary_x":
            xt, yt = (coef, exp, False), (fc, fe, False)
        else:
            xt, yt = (fc, fe, False), (coef, exp, False)
        r = gtv.solve_binary(name, gtv.BIN_FUNCS[name], xt, yt, FMT, mode)
        if r is None:
            raise RuntimeError("solve_binary None: %s %s" % (name, mode))
        out, pbits, rad, margin = r
        i1 = "%de%d" % (xt[0], xt[1])
        i2 = "%de%d" % (yt[0], yt[1])
        return ("34 %s %s %s %s" % (mode, i1, i2, out),
                "34 %s %s %s P=%d rad=%.3e margin=%.3e decisive"
                % (mode, i1, i2, pbits, rad, margin))
    if op["kind"] == "int":
        xt = (coef, exp, False)
        r = gtv.solve_int(name, gtv.INT_FUNCS[name], xt, op["n"],
                          FMT, mode)
        if r is None:
            raise RuntimeError("solve_int None: %s %s" % (name, mode))
        out, pbits, rad, margin = r
        inp = "%de%d" % (coef, exp)
        return ("34 %s %s %d %s" % (mode, inp, op["n"], out),
                "34 %s %s %d P=%d rad=%.3e margin=%.3e decisive"
                % (mode, inp, op["n"], pbits, rad, margin))
    raise AssertionError(op["kind"])


def parse_dec(s):
    """'2.718281828459045' -> (coef, exp) exact."""
    if "." in s:
        a, b = s.split(".")
        return int(a + b), -len(b)
    return int(s), 0


GRADES = [
    # (name, lo(t), hi(t), floor_lo, floor_hi, escalates)
    ("control", lambda t: 8 * t, lambda t: 40 * t, 0, 0, False),
    ("entry", lambda t: t / 40, lambda t: t / 8, 0, 0, True),
    ("deep", lambda t: t / 4000, lambda t: t / 800,
     mpf("1.5e-14"), mpf("7.5e-14"), True),
]


def main():
    gtv._require_flint()  # binds arb/ctx/fmpq inside gtv (lazy import)
    budgets = parse_rung1_budgets()
    ops = OPS(budgets)
    only = None
    if "--ops" in sys.argv:
        only = set(sys.argv[sys.argv.index("--ops") + 1].split(","))
        unknown = only - set(ops)
        if unknown:
            raise SystemExit("--ops unknown: %s" % ",".join(unknown))
    os.makedirs(OUT_DIR, exist_ok=True)
    summary = []
    for name in sorted(ops):
        if only and name not in only:
            continue
        op = ops[name]
        t = mpf(budgets[op["budget"]]) * mpf(10) ** (P - 50)
        rows = []  # (grade, family, coef, exp, planted)
        for (gname, flo, fhi, floor_lo, floor_hi, esc) in GRADES:
            lo, hi = flo(t), fhi(t)
            if floor_lo and lo < floor_lo:
                lo, hi = floor_lo, floor_hi
            if esc and hi >= t / 4:
                raise SystemExit(
                    "%s %s: band [%s, %s] not clear of t=%s"
                    % (name, gname, lo, hi, t))
            for fam, x0 in (("mid", op["x0"][0]), ("grid", op["x0"][1])):
                got = plant_one(op, name, x0, fam, lo, hi)
                if got is None:
                    raise SystemExit("planting failed: %s %s %s"
                                     % (name, gname, fam))
                coef, exp, dist = got
                assert gtv.representable(coef, exp, FMT), (name, coef, exp)
                rows.append((gname, fam, coef, exp, dist))
        # emit
        txt_lines, prov_lines = [], []
        n_esc_inputs = 0
        for (gname, fam, coef, exp, dist) in rows:
            esc = gname != "control"
            n_esc_inputs += 1 if esc else 0
            for mode in MODES:
                lt, lp = certify(name, op, coef, exp, mode)
                txt_lines.append(lt)
                prov_lines.append(
                    "%s planted=%.3e grade=%s family=%s budget=%s@%d "
                    "escalates=%s"
                    % (lp, dist, gname, fam, op["budget"],
                       budgets[op["budget"]], "yes" if esc else "no"))
        write_out(name, op, t, txt_lines, prov_lines, budgets)
        summary.append((name, len(txt_lines), n_esc_inputs * len(MODES)))
        print("%-10s %3d lines, %3d expected rung-2 entries"
              % (name, len(txt_lines), n_esc_inputs * len(MODES)))
    print("\nplanted corpus: %d files, %d lines, %d expected entries"
          % (len(summary), sum(s[1] for s in summary),
             sum(s[2] for s in summary)))


def write_out(name, op, t, txt_lines, prov_lines, budgets):
    hdr_txt = (
        "# S3 planted rung-2-forcing vectors for `%s` (fd-4zo.20,\n"
        "# ADR-0059). Same line grammar as the sampled corpus. Every\n"
        "# input is constructed so the true result sits at a chosen\n"
        "# distance from a rounding boundary (see %s.prov); rows below\n"
        "# the rung-1 escalation threshold MUST enter rung 2, pinned\n"
        "# by tests/ladder_telemetry.rs. Regenerate:\n"
        "# tools/gen_planted_hardcases.py --ops %s\n" % (name, name, name))
    hdr_prov = (
        "# Provenance for planted/%s.txt. Base fields as the sampled\n"
        "# corpus .prov. planted= signed distance (fractional ULP of\n"
        "# the output) to the targeted boundary family at generation\n"
        "# time; grade/family/budget document the construction;\n"
        "# escalates= the generator's threshold verdict\n"
        "# (t = rung1 x 10^(P-50) = %.3e fractional ULP, rung1 %s=%d\n"
        "# parsed from ladder.rs at generation).\n"
        % (name, float(t), op["budget"], budgets[op["budget"]]))
    with open(os.path.join(OUT_DIR, "%s.txt" % name), "w") as fh:
        fh.write(hdr_txt)
        fh.write("\n".join(txt_lines) + "\n")
    with open(os.path.join(OUT_DIR, "%s.prov" % name), "w") as fh:
        fh.write(hdr_prov)
        fh.write("\n".join(prov_lines) + "\n")


def selftest():
    """Brute-force cross-check of ostrowski_band on small scales:
    every return must be exact and in band; when brute force finds a
    band member, the walk (over both directions, as the caller runs
    it) must find one too."""
    import random
    rng = random.Random(0xF00D)
    M = SCALE
    found = misses = 0
    for trial in range(400):
        s = rng.randrange(1, M)
        c = rng.randrange(-M // 2, M // 2)
        kmax = rng.randrange(50, 4000)
        mag = 10 ** rng.randrange(30, 39)
        lo, hi = 2 * mag, 20 * mag
        brute_hit = any(
            lo <= abs(_red(c + kk * s, M)) <= hi
            for kk in range(kmax + 1))
        ours = None
        for mdir in (1, -1):
            got = ostrowski_band(c, (mdir * s) % M, kmax, lo, hi)
            if got is not None:
                k, v = got
                want = _red(c + mdir * k * s, M)
                assert v == want, (trial, v, want)
                assert lo <= abs(v) <= hi, (trial, v, lo, hi)
                assert 0 <= k <= kmax, (trial, k)
                ours = got
                break
        if brute_hit:
            found += 1
            if ours is None:
                misses += 1
    assert found > 100, "selftest bands too rarely feasible: %d" % found
    # A single-window walk misses when the CF sign structure locks
    # before the band (~1 in 6 here, at the selftest's deliberately
    # tiny kmax of 50..4000 where levels are coarse). Production runs
    # tile up to 500k windows per attempt and nudge the base point
    # across 24 attempts, so per-window misses are recovered; the
    # binding asserts above are exactness, band membership, and the
    # k bound, which admit no slack.
    assert misses * 4 <= found, "walk missed %d of %d feasible bands" \
        % (misses, found)
    print("selftest ok: %d feasible, %d missed by the walk"
          % (found, misses))


def _red(x, M):
    x %= M
    return x - M if x > M // 2 else x


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
