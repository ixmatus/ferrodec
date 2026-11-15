"""Expected cohort: exact value v (Fraction), preferred Qp from the
§9.2.2 formulas as the brief states them; delivered quantum is the
attainable exponent closest to Qp (§6.3), within [etiny, emax-P+1]."""
import sys, math
from fractions import Fraction
from decimal import Decimal
FMT = {'d32': (7, 96), 'd64': (16, 384), 'd128': (34, 6144)}
def Q(s):
    d = Decimal(s); return d.as_tuple().exponent
def isqrt_frac(fr, n):
    # exact n-th root of a positive Fraction or None
    def iroot(k, n):
        r = int(round(k ** (1.0 / n))) if k < 10**300 else int(Decimal(k) ** (Decimal(1) / n))
        for c in range(max(r - 2, 0), r + 3):
            if c ** n == k: return c
        return None
    a, b = iroot(fr.numerator, n), iroot(fr.denominator, n)
    return Fraction(a, b) if a is not None and b is not None else None
def terminating(fr):
    d = fr.denominator
    while d % 2 == 0: d //= 2
    while d % 5 == 0: d //= 5
    return d == 1
def expect(fmt, v, qp):
    P, emax = FMT[fmt]
    if v == 0:
        q = max(min(qp, emax - P + 1), 1 - emax - P + 1)
        return Decimal((0, (0,), q))
    sign = 1 if v < 0 else 0
    v = abs(v)
    # stripped c * 10^e
    e = 0
    while v.denominator != 1:
        v *= 10; e -= 1
    c = v.numerator
    while c % 10 == 0:
        c //= 10; e += 1
    dc = len(str(c))
    if dc > P: return None
    lo, hi = e - (P - dc), e
    q = min(max(qp, lo), hi)
    q = max(min(q, emax - P + 1), 1 - emax - P + 1)
    return Decimal((sign, tuple(int(ch) for ch in str(c * 10 ** (e - q))), q))
for line in open(sys.argv[1]):
    f, op, x, a, r, st = line.strip().split('|')
    X = Fraction(Decimal(x)); qx = Q(x)
    if op == 'powi':
        n = int(a); v = X ** n if X != 0 or n > 0 else None; qp = n * qx
    elif op == 'rsqrt':
        v = isqrt_frac(1 / X, 2); qp = -math.floor(qx / 2)
    elif op == 'rootn':
        n = int(a)
        if X == 0: v = Fraction(0) if n > 0 else None
        else:
            rt = isqrt_frac(abs(X), abs(n))
            v = None if rt is None else (rt if n > 0 else 1 / rt) * (-1 if X < 0 else 1)
        qp = math.floor(qx / n)
    elif op == 'compound':
        n = int(a); v = (1 + X) ** n; qp = math.floor(n * min(0, qx))
    elif op == 'hypot':
        Y = Fraction(Decimal(a)); v = isqrt_frac(X * X + Y * Y, 2) if X or Y else Fraction(0); qp = min(qx, Q(a))
    if v is None or not terminating(v):
        continue
    want = expect(f, v, qp)
    if want is None: continue
    got = Decimal(r)
    tag = 'ok ' if (str(got) == str(want) and got.is_signed() == want.is_signed()) else 'BAD'
    if v == 0 and got.is_signed() != want.is_signed():
        tag = 'ok?'  # zero sign judged by the specials checker
    print(tag, line.strip(), 'want', want, 'Qp', qp)
