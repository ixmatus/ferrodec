import sys, random
sys.path.insert(0, sys.argv[0].rsplit('/', 1)[0])
from oracle import *
random.seed(20261008)
out = []

def rep(fm, fr, extra_zeros=0):
    """Exact decimal string of fr if representable in fm (optionally with trailing zeros), else None."""
    p, emax = FMT[fm]
    if not terminating(fr) or fr == 0: return None
    d = exact_decimal(fr)
    sign, digits, exp = d.as_tuple()
    digits = list(digits)
    while len(digits) > 1 and digits[-1] == 0: digits.pop(); exp += 1
    for _ in range(extra_zeros):
        if len(digits) < p: digits.append(0); exp -= 1
    if len(digits) > p: return None
    etiny = 1 - emax - p + 1 - 1 + 1  # = 2 - emax - p ... computed below
    etiny = (1 - emax) - p + 1
    if exp < etiny: return None
    adj = exp + len(digits) - 1
    if adj > emax: return None
    # clamp: large exponents need padding; parse handles by fold-down; ensure exp <= emax-p+1 or padable
    if exp > emax - p + 1 and len(digits) + (exp - (emax - p + 1)) > p: return None
    return str(Decimal((sign, tuple(digits), exp)))

def add(fm, op, args, modes=MODES):
    for m in modes: out.append(f"{fm} {op} {m} {' '.join(str(a) for a in args)}")

def variants(fm, fr):
    """Spellings: stripped, padded with zeros, plus its two neighbours one unit in the last place."""
    vs = []
    for z in (0, 1, 3):
        s = rep(fm, fr, z)
        if s and s not in vs: vs.append(s)
    return vs

def neighbours(fm, fr):
    p, _ = FMT[fm]
    s = rep(fm, fr)
    if not s: return []
    d = Decimal(s)
    c = ctx(fm, decimal.ROUND_HALF_EVEN)
    res = []
    for nb in (c.next_plus(d), c.next_minus(d)):
        if nb.is_finite() and nb != 0: res.append(str(nb))
    return res

for fm in FMT:
    p, emax = FMT[fm]
    # exp2 / exp2m1 / exp10 / exp10m1 integer families and edges
    for n in list(range(-135, 136)) :
        for op in ('exp2', 'exp2m1'):
            for s in variants(fm, Fraction(n)) or [str(n)]:
                if n == 0: continue
                add(fm, op, [s])
    for op in ('exp2', 'exp2m1', 'exp10', 'exp10m1'):
        for s in ('0.5', '1.5', '-0.5', '2.50', '1E+1', '10E-1', '-1E+1', '123E-2', '99999', '-99999', '1E+5', '-1E+5', '100000', '1E+6', '-1E+6', '99998', '1.0000001', '3E+2'):
            if rep(fm, frac(s)): add(fm, op, [s])
    ranges = [range(-20, 60), range(emax - 40, emax + 40), range(-(emax + p + 40), -(emax + p - 40)), range(99990, 100001), range(-100000, -99989)]
    for rg in ranges:
        for n in rg:
            if n == 0: continue
            for op in ('exp10', 'exp10m1'):
                s = rep(fm, Fraction(n))
                if s: add(fm, op, [s])
    # log2 / log10
    for k in range(-60, 125):
        fr = Fraction(2) ** k
        for s in variants(fm, fr) + neighbours(fm, fr): add(fm, 'log2', [s])
    for k in list(range(-(emax + p + 2), -(emax + p) + 40)) + list(range(-40, 40)) + list(range(emax - 40, emax + 2)):
        fr = Fraction(10) ** k
        for s in variants(fm, fr)[:2] + neighbours(fm, fr): add(fm, 'log10', [s], modes=['NE', 'TN'])
    # log2p1 / log10p1
    for k in range(1, 125):
        fr = Fraction(2) ** k - 1
        for s in variants(fm, fr) + neighbours(fm, fr): add(fm, 'log2p1', [s])
    for m in range(1, 45):
        fr = Fraction(1, 2 ** m) - 1
        for s in variants(fm, fr) + neighbours(fm, fr): add(fm, 'log2p1', [s])
    for k in range(1, p + 3):
        for fr in (Fraction(10) ** k - 1, Fraction(1, 10 ** k) - 1):
            for s in variants(fm, fr) + neighbours(fm, fr): add(fm, 'log10p1', [s])
    # cbrt
    tmax = 10 ** ((p + 2) // 3 + 1)
    roots = [2, 3, 5, 7, 10, 21, 99, 125, 999, 1001, 46415, 99999, 215443, 999999, 2154434, 9999999, 21544346, 99999999, 215443469, 999999999, 2154434690, 9999999999, 21544346900, 99999999999, 215443469003, 999999999999, 2154434690031]
    roots += [random.randrange(2, tmax) for _ in range(40)]
    for t in roots:
        for u in (-(emax // 3) - 5, -(emax // 3) + 3, -7, -1, 0, 1, 5, emax // 3 - 12):
            fr = Fraction(t) ** 3 * Fraction(10) ** (3 * u)
            for s in variants(fm, fr)[:2] + neighbours(fm, fr): add(fm, 'cbrt', [s], modes=['NE', 'TZ', 'TP'])
    # subnormal cube inputs
    etiny = (1 - emax) - p + 1
    for cexp in range(etiny, etiny + 12):
        for c in (1, 8, 27, 1000, 125, 343):
            fr = Fraction(c) * Fraction(10) ** cexp
            s = rep(fm, fr)
            if s: add(fm, 'cbrt', [s], modes=['NE', 'TZ'])
    # rsqrt
    for i in range(0, 120):
        for fr in (Fraction(2) ** i, Fraction(1, 2 ** i), Fraction(4) ** i / 10 ** (2 * (i // 3))):
            for s in variants(fm, fr)[:2] + neighbours(fm, fr): add(fm, 'rsqrt', [s])
    for j in range(0, 60):
        for e in (-2 * j, 0, 2 * j, -2 * j - 1):
            fr = Fraction(5) ** j * Fraction(10) ** e
            for s in variants(fm, fr)[:1] + neighbours(fm, fr): add(fm, 'rsqrt', [s], modes=['NE', 'TZ', 'TP'])
    for q in (3, 7, 9, 11, 13, 99, 1001):
        for s in (rep(fm, Fraction(q * q)), rep(fm, Fraction(4 * q * q, 100)), rep(fm, Fraction(1, 4) * q * q)):
            if s: add(fm, 'rsqrt', [s], modes=['NE'])
    for e in (etiny, etiny + 1, emax - p + 1, emax - p):
        for c in (1, 4, 16, 25, 100):
            s = rep(fm, Fraction(c) * Fraction(10) ** e)
            if s: add(fm, 'rsqrt', [s], modes=['NE', 'TZ'])
    # powi
    bases = [Fraction(2), Fraction(5), Fraction(3), Fraction(7), Fraction(1, 5), Fraction(1, 2), Fraction(3, 2), Fraction(25), Fraction(125), Fraction(10), Fraction(1, 10), Fraction(21, 20), Fraction(15, 10), Fraction(99), Fraction(11), Fraction(4), Fraction(1, 4), Fraction(1, 8), Fraction(16)]
    for b in bases:
        for n in list(range(-130, 131)) + [255, 256, 257, -255, -256, 6144, 6145, 6176, 6177, -6176, -6177, 99999, 100000, -99999, -100000, 2**31 - 1, -2**31]:
            if n == 0: continue
            s = rep(fm, b)
            if s: add(fm, 'powi', [s, n], modes=['NE', 'NA', 'TZ', 'TP'])
    for j in (1, 2, 3, 100, 1000, emax // 2):
        for n in (1, 2, 3, emax // j, emax // j + 1, -(emax // j), -(emax // j) - 1, (emax + p) // j + 1, -((emax + p) // j) - 1):
            if n == 0: continue
            for b in (Fraction(10) ** j, Fraction(1, 10 ** j)):
                s = rep(fm, b)
                if s: add(fm, 'powi', [s, n], modes=['NE', 'TZ', 'TP'])
    # subnormal ties: 5E-k cubed, etc.
    for (c, n) in ((5, 3), (5, 2), (15, 3), (25, 3), (5, 5), (35, 3), (5, 7)):
        for k in range(-((etiny - 3) // n) - 3, -((etiny - 3) // n) + 3):
            s = rep(fm, Fraction(c) * Fraction(10) ** (-k))
            if s: add(fm, 'powi', [s, n])
    # rootn
    for n in list(range(2, 40)) + [64, 100, 112, 113, 127, 128, 200, 2**31 - 1]:
        for t in (2, 3, 5, 7, 10, 12, 99):
            for sgn in (1, -1):
                for u in (0, 1, -1):
                    if n * t.bit_length() > 400: continue
                    fr = Fraction(t) ** n * Fraction(10) ** (n * u)
                    for s in variants(fm, fr)[:2] + neighbours(fm, fr)[:1]:
                        add(fm, 'rootn', [s, sgn * n], modes=['NE', 'TZ', 'TP'])
        for c in (1, 10, '1E+100', '1E-100'):
            add(fm, 'rootn', [c, n], modes=['NE'])
            add(fm, 'rootn', [c, -n], modes=['NE'])
    for k in range(1, 60):
        fr = Fraction(4) ** k
        for s in variants(fm, fr)[:1]:
            add(fm, 'rootn', [s, -2])
        fr = Fraction(8) ** k
        for s in variants(fm, fr)[:1]:
            add(fm, 'rootn', [s, -3], modes=['NE', 'TZ'])
    # pow
    ys = ['0.5', '1.5', '-0.5', '-1.5', '0.25', '-0.25', '0.2', '-0.2', '2.5', '24.5', '-24.5', '49', '-49', '50', '0.125', '0.04', '1E+1', '10E-1', '1E+4', '0.0625', '3', '-3', '1E-5', '2E-5', '1E+40', '1E+38', '6144', '6145', '-6176', '-6177', '99999', '100000', '-100000']
    xs = [Fraction(4), Fraction(25), Fraction(16), Fraction(1, 4), Fraction(1, 25), Fraction(5), Fraction(2), Fraction(10), Fraction(1, 10), Fraction(100), Fraction(9, 4), Fraction(225, 100), Fraction(10 ** 30), Fraction(2 ** 100), Fraction(5 ** 40), Fraction(3 ** 20), Fraction(32), Fraction(1, 32), Fraction(1024), Fraction(10) ** emax, Fraction(10) ** (-emax), Fraction(7)]
    for x in xs:
        sx = rep(fm, x)
        if not sx: continue
        for y in ys:
            if rep(fm, frac(y)): add(fm, 'pow', [sx, y], modes=['NE', 'NA', 'TZ', 'TP'])
    for (c, n) in ((5, 3), (15, 3), (25, 3)):
        for k in range(-((etiny - 3) // n) - 2, -((etiny - 3) // n) + 2):
            s = rep(fm, Fraction(c) * Fraction(10) ** (-k))
            if s: add(fm, 'pow', [s, str(n)])
    # compound
    cx = [Fraction(4), Fraction(1), Fraction(1, 2), Fraction(-1, 2), Fraction(1, 20), Fraction(-3, 4), Fraction(9), Fraction(99), Fraction(-9, 10), Fraction(-99, 100), Fraction(3), Fraction(15), Fraction(1, 4), Fraction(-1, 5), Fraction(24), Fraction(1, 1000), Fraction(-1, 10 ** (p)), Fraction(1, 10 ** p), Fraction(10 ** p - 1), Fraction(1, 10 ** (p + 3)), Fraction(-1, 10 ** (2 * p + 2)), Fraction(1, 10 ** (2 * p + 3))]
    cx += [Fraction(10 ** k - 1) for k in (p - 1, p)] + [Fraction(1, 10 ** k) - 1 for k in (p - 1, p)]
    for x in cx:
        s = rep(fm, x)
        if not s: continue
        ns = list(range(-60, 61)) + [100, 127, 128, -127, -128, 180, 181, 182, -180, -181, -182, emax // p, emax // p + 1, (emax + p) // p + 1, 196, 197, -196, -197, 1000000 // p, 1000000 // p + 1, 999999, 1000000, 1000001, -1000000, -1000001, 2**31 - 1, -2**31]
        for n in ns:
            if n == 0: continue
            add(fm, 'compound', [s, n], modes=['NE', 'NA', 'TZ', 'TP'])
    # hypot
    trip = [(3, 4), (5, 12), (8, 15), (7, 24), (20, 21), (9, 40), (119, 120), (696, 697)]
    for (a, b) in trip:
        for e1 in (0, 3, -5, emax - p, etiny):
            for e2 in (0, 1, -1):
                xa = Fraction(a) * Fraction(10) ** e1
                xb = Fraction(b) * Fraction(10) ** (e1 + e2)
                sa, sb = rep(fm, xa), rep(fm, xb)
                if sa and sb: add(fm, 'hypot', [sa, sb], modes=['NE', 'TZ', 'TP'])
    # m^2-1, 2m family across decades, and scaled 3-4-5 ties at P+1
    for j in range(1, p):
        m = 10 ** j
        for (A, B) in ((m * m - 1, 2 * m), (m * m - 4, 4 * m)):
            sa, sb = rep(fm, Fraction(A)), rep(fm, Fraction(B))
            if sa and sb: add(fm, 'hypot', [sa, sb])
    lo = 2 * 10 ** (p - 1); hi = (10 ** p) // 4
    for g in [lo + 1, lo + 3, lo + 2, hi - 1, hi - 2, (lo + hi) // 2 | 1] + [random.randrange(lo, hi) for _ in range(20)]:
        sa, sb = rep(fm, Fraction(3 * g)), rep(fm, Fraction(4 * g))
        if sa and sb: add(fm, 'hypot', [sa, sb])
    for k in [1, 2, 3, 10, 99, 10 ** 5, 10 ** 8, 10 ** (p // 2), 10 ** (p // 2) + 1, 10 ** (p - 1) - 1]:
        sa = rep(fm, Fraction(k))
        if sa: add(fm, 'hypot', [sa, '1'], modes=['NE', 'TZ'])

open(sys.argv[1], 'w').write('\n'.join(out) + '\n')
print(len(out))
