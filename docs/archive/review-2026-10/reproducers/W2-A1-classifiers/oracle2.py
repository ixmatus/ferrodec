"""A1 oracle v2: exact decimal rationals as (sign, m, e) with integer
arithmetic derived from the definitions; rounding by CPython decimal
(libmpdec). Value kinds:
  ('dec', sign, m, e)       terminating value (-1)^sign * m * 10^e, exact
  ('quot', sign, num, den)  non-terminating rational num/den (ints, den not 2^i5^j after reduction)
  ('irr',)                  irrational (by the rational-root / Niven arguments)
  ('wide',)                 terminating or not, stripped coefficient provably > 3000 digits (neither exact nor tie)
  ('huge', s)               |log10 value| > 2e5, s = sign of log10
"""
import sys, decimal, math
from decimal import Decimal, Context
FMT = {'128': (34, 6144), '64': (16, 384), '32': (7, 96)}
RM = {'NE': decimal.ROUND_HALF_EVEN, 'NA': decimal.ROUND_HALF_UP, 'TZ': decimal.ROUND_DOWN,
      'TP': decimal.ROUND_CEILING, 'TN': decimal.ROUND_FLOOR}
MODES = list(RM)

def ctx(fm, rounding):
    p, emax = FMT[fm]
    return Context(prec=p, Emax=emax, Emin=1 - emax, rounding=rounding, clamp=1, traps=[])

def iroot(n, k):
    if n < 2 or k == 1: return n
    if k >= n.bit_length(): return 1
    x = 1 << ((n.bit_length() + k - 1) // k)
    while True:
        y = ((k - 1) * x + n // x ** (k - 1)) // k
        if y >= x: break
        x = y
    while x ** k > n: x -= 1
    while (x + 1) ** k <= n: x += 1
    return x

def strip(m, e):
    if m == 0: return 0, e
    while m % 10 == 0: m //= 10; e += 1
    return m, e

def parse(s):
    t = Decimal(s).as_tuple()
    m = int(''.join(map(str, t.digits))) if t.digits else 0
    return t.sign, m, t.exponent

def v25(m):
    i = 0
    while m % 2 == 0: m //= 2; i += 1
    j = 0
    while m % 5 == 0: m //= 5; j += 1
    return i, j, m

def droot(m, e, k):
    """k-th root of positive m*10^e, as stripped (s, q) or None if irrational."""
    m, e = strip(m, e)
    r = e % k
    M = m * 10 ** r
    s = iroot(M, k)
    if s ** k != M: return None
    return strip(s, (e - r) // k)

def dinv(sign, m, e):
    """1/(m*10^e)."""
    i, j, t = v25(m)
    if t != 1: return ('quot', sign, 10 ** max(-e, 0), m * 10 ** max(e, 0))
    if i >= j:   # m = 2^i (j = 0 when stripped) -> 5^i / 10^i
        return ('dec', sign, 5 ** i * 2 ** 0 * (1 if j == 0 else 2 ** 0), -e - i) if j == 0 else ('dec', sign, 5 ** (i - j) * 1, -e - i)
    return ('dec', sign, 2 ** (j - i), -e - j)

def dpow(m, e, n):
    """(m*10^e)^n for positive stripped m, integer n."""
    m, e = strip(m, e)
    if m == 1:
        k = e * n
        return ('huge', 1 if k > 0 else -1) if abs(k) > 200000 else ('dec', 0, 1, k)
    lg = math.log10(m) + e
    if abs(lg * n) > 200000: return ('huge', 1 if lg * n > 0 else -1)
    if abs(n) * math.log10(m) > 3000: return ('wide',)
    if n > 0: return ('dec', 0, m ** n, e * n)
    mm = m ** (-n)
    return dinv(0, mm, e * (-n))

def true_value(fm, op, a):
    if op in ('cbrt', 'rsqrt', 'rootn'):
        sg, m, e = parse(a[0])
        k = 3 if op == 'cbrt' else 2 if op == 'rsqrt' else abs(int(a[1]))
        neg = op == 'rsqrt' or (op == 'rootn' and int(a[1]) < 0)
        r = droot(m, e, k)
        if r is None: return ('irr',)
        return dinv(0, *r) if neg else ('dec', 0, r[0], r[1])
    if op in ('pow', 'powi', 'compound'):
        sg, m, e = parse(a[0])
        if op == 'compound':
            if e >= 0: m, e = m * 10 ** e + (-1 if sg else 1), 0
            else: m, e = 10 ** (-e) + (-m if sg else m), e
            sg = 0
        m, e = strip(m, e)
        if op == 'pow':
            ys, ym, ye = parse(a[1])
            ym, ye = strip(ym, ye)
            if ye >= 0: num, den = ym * 10 ** ye, 1
            else:
                i, j, t = v25(ym); d = -ye
                ci, cj = min(i, d), min(j, d)
                num = ym // (2 ** ci * 5 ** cj); den = 2 ** (d - ci) * 5 ** (d - cj)
            if ys: num = -num
        else:
            num, den = int(a[1]), 1
        if m == 1 and e == 0: return ('dec', 0, 1, 0)
        if den > 1:
            if den.bit_length() > 64 and not (m == 1): return ('irr',) if droot(m, e, den) is None else ('?',)
            r = droot(m, e, den)
            if r is None: return ('irr',)
            m, e = r
        if abs(num) > 10 ** 9 and m != 1: return ('huge', 0)
        return dpow(m, e, num)
    if op in ('exp2', 'exp2m1', 'exp10', 'exp10m1'):
        sg, m, e = parse(a[0])
        m, e = strip(m, e)
        if e < 0: return ('irr',)
        if e > 6: return ('huge', -1 if sg else 1)
        n = m * 10 ** e * (-1 if sg else 1)
        if abs(n) > 200000: return ('huge', 1 if n > 0 else -1)
        if op == 'exp10': return ('dec', 0, 1, n)
        if op == 'exp2': return ('dec', 0, 2 ** n, 0) if n >= 0 else ('dec', 0, 5 ** -n, n)
        if op == 'exp2m1':
            return ('dec', 0, 2 ** n - 1, 0) if n >= 0 else ('dec', 1, 10 ** -n - 5 ** -n, n)
        return ('nines', 0, n, 0) if n > 0 else ('nines', 1, -n, n)
    if op in ('log2', 'log10', 'log2p1', 'log10p1'):
        sg, m, e = parse(a[0])
        if op.endswith('p1'):
            if e >= 0: m, e = m * 10 ** e + (-1 if sg else 1), 0
            else: m, e = 10 ** (-e) + (-m if sg else m), e
        m, e = strip(m, e)
        if op.startswith('log10'):
            return ('dec', 1 if e < 0 else 0, abs(e), 0) if m == 1 else ('irr',)
        if e == 0 and m & (m - 1) == 0: return ('dec', 0, m.bit_length() - 1, 0)
        if e < 0 and m == 5 ** (-e): return ('dec', 1, -e, 0)
        return ('irr',)
    if op in ('sinpi', 'cospi', 'tanpi', 'asinpi', 'acospi', 'atanpi', 'atan2pi'):
        from fractions import Fraction
        import math as _m
        x = Fraction(Decimal(a[-1])); neg = Decimal(a[-1]).is_signed()
        if op == 'atan2pi':
            y = Fraction(Decimal(a[0])); yneg = Decimal(a[0]).is_signed()
            if abs(y) == abs(x): return ('dec', 1 if yneg else 0, 25 if x > 0 else 75, -2)
            return ('irr',)
        if op in ('sinpi', 'cospi', 'tanpi'):
            fl = _m.floor(x); frac = x - fl
            if frac == 0:
                odd = fl % 2 == 1
                if op == 'sinpi': return ('zero', neg)
                if op == 'cospi': return ('dec', 1 if odd else 0, 1, 0)
                return ('zero', (odd and not neg) or (not odd and neg))
            if frac == Fraction(1, 2):
                odd = fl % 2 == 1
                if op == 'sinpi': return ('dec', 1 if odd else 0, 1, 0)
                if op == 'cospi': return ('zero', False)
                return ('inf', odd)
            if op == 'tanpi' and frac in (Fraction(1, 4), Fraction(3, 4)):
                return ('dec', 1 if frac == Fraction(3, 4) else 0, 1, 0)
            return ('irr',)
        if abs(x) == 1:
            if op == 'asinpi': return ('dec', 1 if neg else 0, 5, -1)
            if op == 'acospi': return ('dec', 0, 1, 0) if neg else ('zero', False)
            return ('dec', 1 if neg else 0, 25, -2)
        if abs(x) == Fraction(1, 2) and op != 'atanpi': return ('quot', 0, 1, 6)
        return ('irr',)
    if op == 'hypot':
        _, m1, e1 = parse(a[0]); _, m2, e2 = parse(a[1])
        q = min(e1, e2)
        if abs(e1 - e2) > 400: return ('?',)
        A = m1 * 10 ** (e1 - q); B = m2 * 10 ** (e2 - q)
        S = A * A + B * B
        w = iroot(S, 2)
        return ('dec', 0, w, q) if w * w == S else ('irr',)
    raise ValueError(op)

def to_dec(v):
    if v[0] == 'dec':
        _, sg, m, e = v
        return Decimal((sg, tuple(int(c) for c in str(m)), e))
    if v[0] == 'nines':
        _, sg, cnt, e = v
        return Decimal((sg, (9,) * cnt, e))
    raise ValueError

def round_v(fm, v, mode):
    c = ctx(fm, RM[mode])
    if v[0] == 'quot':
        _, sg, num, den = v
        d = c.divide(Decimal(num), Decimal(den))
        if sg: d = -d  # sign symmetric only for nearest; callers use sg=0
    else:
        d = c.plus(to_dec(v))
        if to_dec(v).is_zero(): d = to_dec(v)
    fl = c.flags
    return d, bool(fl[decimal.Inexact]), bool(fl[decimal.Overflow]), bool(fl[decimal.Underflow])

def exact_or_tie(fm, v):
    if v[0] not in ('dec', 'nines'): return (False, False)
    D = to_dec(v)
    c = ctx(fm, decimal.ROUND_DOWN); c.plus(D); exact = not c.flags[decimal.Inexact]
    up = ctx(fm, decimal.ROUND_HALF_UP).plus(D)
    dn = ctx(fm, decimal.ROUND_HALF_DOWN).plus(D)
    return exact, (up != dn) or (up.is_infinite() != dn.is_infinite())
