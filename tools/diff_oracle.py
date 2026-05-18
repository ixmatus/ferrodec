#!/usr/bin/env python3
"""Differential decimal oracle for the ferrodec family (Track 3,
ADR plan 2026-05-17; special-function surface extended under ADR-0026,
fd-cb6). Local-only: invoked solely by the `differential`-feature test
binaries, never by CI or a default `cargo test`.

Two independent references behind one protocol:

* **libmpdec** (CPython stdlib `decimal`): a correctly-rounded
  arbitrary-precision *decimal* implementation. It is decimal-native
  and serves `add sub mul div fma sqrt exp ln log10 pow`. It rounds at
  the format's IEEE context itself, so its response is the
  format-rounded value plus signal flags.

* **mpmath** (optional pip package): an independent arbitrary-precision
  reference for the special-function surface `decimal` lacks
  (`exp2 log2 cbrt sin cos tan asin acos atan atan2 sinh cosh tanh
  asinh acosh atanh`). It is structurally independent of both the
  Extended kernel and the astro-float oracle, so it breaks the
  correlated-failure surface (ADR-0026). For these ops the driver does
  **not** round to the format: it returns a high-precision decimal
  string and the Rust harness owns the rounding (no double rounding,
  the faithful contract stays on our side). Working precision is
  raised by the argument's decimal magnitude so trig argument
  reduction stays sound in the decades the astro-float oracle skips.

Protocol (line-delimited TSV on stdin, one response line per request):

  request : op TAB prec TAB Emax TAB Emin TAB round TAB a [TAB b [TAB c]]
  response: value TAB flagbits

`flagbits` is a small bitfield: 1 InvalidOperation, 2 DivisionByZero,
4 Inexact, 8 Overflow, 16 Underflow (0 for the mpmath ops, which the
Rust side checks by faithful band, not by signal). `value` is
`str(Decimal)` for libmpdec ops, a high-precision decimal string for
mpmath ops, `NaN` for an out-of-domain request, or `Skip` when an
mpmath op was requested but mpmath is not importable (the Rust side
treats `Skip` as skipped-with-diagnostic, never as a failure: the
differential is a corroborating local check, not a gate).

The libmpdec path is pure stdlib and requires no pip packages.
`--selfcheck` prints `OK <libmpdec_version> mpmath=<ver|none>` and
exits, so the Rust harness can verify a usable interpreter before
batching (it only requires the `OK ` prefix; mpmath absence degrades
to per-request `Skip`, it does not fail the batch).
"""

import sys
import decimal
from decimal import Decimal

ROUND = {
    "NearestEven": decimal.ROUND_HALF_EVEN,
    "NearestAway": decimal.ROUND_HALF_UP,
    "TowardZero": decimal.ROUND_DOWN,
    "TowardPositive": decimal.ROUND_CEILING,
    "TowardNegative": decimal.ROUND_FLOOR,
}

# Signal -> bit. Only the cross-implementation-unambiguous signals are
# emitted; the Rust side decides which to compare per op.
BITS = [
    (decimal.InvalidOperation, 1),
    (decimal.DivisionByZero, 2),
    (decimal.Inexact, 4),
    (decimal.Overflow, 8),
    (decimal.Underflow, 16),
]

# Special functions `decimal` lacks, served by mpmath. Disjoint from
# the libmpdec op set, so dispatch is unambiguous.
MPMATH_OPS = frozenset(
    [
        "exp2", "log2", "cbrt",
        "sin", "cos", "tan",
        "asin", "acos", "atan", "atan2",
        "sinh", "cosh", "tanh",
        "asinh", "acosh", "atanh",
    ]
)

try:
    import mpmath  # type: ignore

    _MPMATH_VER = getattr(mpmath, "__version__", "n/a")
except Exception:  # pragma: no cover - environment dependent
    mpmath = None
    _MPMATH_VER = None


def compute_libmpdec(op, ctx, args):
    a = Decimal(args[0])
    if op == "sqrt":
        return a.sqrt(ctx)
    if op == "exp":
        return a.exp(ctx)
    if op == "ln":
        return a.ln(ctx)
    if op == "log10":
        return a.log10(ctx)
    b = Decimal(args[1])
    if op == "add":
        return ctx.add(a, b)
    if op == "sub":
        return ctx.subtract(a, b)
    if op == "mul":
        return ctx.multiply(a, b)
    if op == "div":
        return ctx.divide(a, b)
    if op == "pow":
        return ctx.power(a, b)
    if op == "fma":
        return ctx.fma(a, b, Decimal(args[2]))
    raise ValueError("unknown op " + op)


def _mp_apply(op, xs):
    mp = mpmath
    if op == "exp2":
        return mp.power(2, xs[0])
    if op == "log2":
        return mp.log(xs[0], 2)
    if op == "cbrt":
        return mp.cbrt(xs[0])
    if op == "sin":
        return mp.sin(xs[0])
    if op == "cos":
        return mp.cos(xs[0])
    if op == "tan":
        return mp.tan(xs[0])
    if op == "asin":
        return mp.asin(xs[0])
    if op == "acos":
        return mp.acos(xs[0])
    if op == "atan":
        return mp.atan(xs[0])
    if op == "atan2":
        return mp.atan2(xs[0], xs[1])
    if op == "sinh":
        return mp.sinh(xs[0])
    if op == "cosh":
        return mp.cosh(xs[0])
    if op == "tanh":
        return mp.tanh(xs[0])
    if op == "asinh":
        return mp.asinh(xs[0])
    if op == "acosh":
        return mp.acosh(xs[0])
    if op == "atanh":
        return mp.atanh(xs[0])
    raise ValueError("unknown mpmath op " + op)


def compute_mpmath(op, prec, args):
    """High-precision true value as a parseable decimal string. The
    Rust harness rounds it to the format (ADR-0026: rounding owned by
    us, not the oracle). Returns `NaN` for an out-of-domain request
    (mpmath yields a complex value there)."""
    if mpmath is None:
        return "Skip"
    # Guard digits over the format precision, plus the argument's
    # decimal magnitude so trig argument reduction stays sound past the
    # astro-float skip decades (sin(1e300) needs ~300 guard digits to
    # survive the reduction cancellation). Capped so a pathological
    # request cannot wedge the batch.
    adj = 0
    for a in args:
        try:
            adj = max(adj, Decimal(a).adjusted())
        except (ValueError, decimal.InvalidOperation):
            return "NaN"
    extra = min(max(adj, 0), 7000)
    with decimal.localcontext() as dc:
        dc.prec = prec + 30
        mpmath.mp.dps = prec + 40 + extra
        try:
            xs = [mpmath.mpf(a) for a in args]
            r = _mp_apply(op, xs)
        except (ValueError, ZeroDivisionError, ArithmeticError):
            return "NaN"
        # Out of domain: mpmath returns a complex result (e.g. ln of a
        # negative, asin |x|>1, acosh x<1). Treat as a special, not a
        # value mismatch — the kernel returns NaN there and the
        # property/conformance suites already cover special values.
        if isinstance(r, mpmath.mpc) or getattr(r, "imag", 0) != 0:
            return "NaN"
        if not mpmath.isfinite(r):
            return "Infinity" if r > 0 else "-Infinity"
        # mpf -> decimal digits -> canonical Decimal string. Reparsing
        # through Decimal guarantees a form `Decimal128::parse_str`
        # accepts; the digit count exceeds the format precision by the
        # guard, so the Rust-side rounding is the only rounding.
        digits = mpmath.nstr(
            r, prec + 25, strip_zeros=False, min_fixed=0, max_fixed=0
        )
        return str(+Decimal(digits))


def flagbits(ctx):
    bits = 0
    for sig, bit in BITS:
        if ctx.flags[sig]:
            bits |= bit
    return bits


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--selfcheck":
        sys.stdout.write(
            "OK %s mpmath=%s\n"
            % (
                getattr(decimal, "__libmpdec_version__", "n/a"),
                _MPMATH_VER if _MPMATH_VER is not None else "none",
            )
        )
        return 0
    out = sys.stdout
    for line in sys.stdin:
        line = line.rstrip("\n")
        if not line:
            continue
        f = line.split("\t")
        op, prec, emax, emin, rnd = f[0], int(f[1]), int(f[2]), int(f[3]), f[4]
        args = f[5:]
        if op in MPMATH_OPS:
            try:
                val = compute_mpmath(op, prec, args)
            except (ValueError, IndexError):
                val = "NaN"
            out.write("%s\t0\n" % val)
            continue
        ctx = decimal.Context(
            prec=prec,
            Emax=emax,
            Emin=emin,
            rounding=ROUND[rnd],
            clamp=1,
            traps=[],
        )
        try:
            res = compute_libmpdec(op, ctx, args)
            val = str(res)
        except (decimal.InvalidOperation, ValueError, IndexError):
            # traps=[] keeps signals non-raising; a raise here means a
            # genuinely undefined request (e.g. malformed). Report it
            # as NaN so the Rust side treats it as a special, not a
            # value mismatch.
            val = "NaN"
        out.write("%s\t%d\n" % (val, flagbits(ctx)))
    out.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
