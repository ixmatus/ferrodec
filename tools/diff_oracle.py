#!/usr/bin/env python3
"""Differential decimal oracle for the ferrodec family (Track 3,
ADR plan 2026-05-17). Local-only: invoked solely by the
`differential`-feature test binaries, never by CI or a default
`cargo test`.

CPython's stdlib `decimal` is libmpdec, a correctly-rounded
arbitrary-precision *decimal* implementation. It is a decimal-native
independent reference, distinct from the binary astro-float oracle and
from the fixed decTest vectors, so it catches a class of
spec-interpretation defect the existing surface cannot.

Protocol (line-delimited TSV on stdin, one response line per request):

  request : op TAB prec TAB Emax TAB Emin TAB round TAB a [TAB b [TAB c]]
  response: value TAB flagbits

`op` in {add,sub,mul,div,fma,sqrt,exp,ln,log10,pow}. The context is
built per line with the format's IEEE parameters (prec/Emax/Emin,
clamp=1, traps=[] so signals set flags instead of raising). `flagbits`
is a small bitfield: 1 InvalidOperation, 2 DivisionByZero, 4 Inexact,
8 Overflow, 16 Underflow. `value` is `str(Decimal)`.

The driver is pure stdlib; it requires no pip packages. It exits 0 on
clean EOF. `--selfcheck` prints `OK <libmpdec_version>` and exits, so
the Rust harness can verify a usable interpreter before batching.
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


def compute(op, ctx, args):
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


def flagbits(ctx):
    bits = 0
    for sig, bit in BITS:
        if ctx.flags[sig]:
            bits |= bit
    return bits


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--selfcheck":
        sys.stdout.write("OK %s\n" % getattr(decimal, "__libmpdec_version__", "n/a"))
        return 0
    out = sys.stdout
    for line in sys.stdin:
        line = line.rstrip("\n")
        if not line:
            continue
        f = line.split("\t")
        op, prec, emax, emin, rnd = f[0], int(f[1]), int(f[2]), int(f[3]), f[4]
        args = f[5:]
        ctx = decimal.Context(
            prec=prec,
            Emax=emax,
            Emin=emin,
            rounding=ROUND[rnd],
            clamp=1,
            traps=[],
        )
        try:
            res = compute(op, ctx, args)
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
