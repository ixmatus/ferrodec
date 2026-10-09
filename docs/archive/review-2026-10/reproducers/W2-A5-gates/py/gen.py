import random, sys
from decimal import Decimal as Dc, Context, ROUND_HALF_EVEN
random.seed(int(sys.argv[1]) if len(sys.argv) > 1 else 1)
NRAND = int(sys.argv[2]) if len(sys.argv) > 2 else 40
FMT = {'d128': (34, 6144, -6143), 'd64': (16, 384, -383), 'd32': (7, 96, -95)}
def q(fmt, v):
    P, emax, emin = FMT[fmt]
    c = Context(prec=P, rounding=ROUND_HALF_EVEN, Emax=emax, Emin=emin, clamp=1, traps=[])
    r = c.plus(Dc(v))
    if not r.is_finite() or r == 0: return None
    return str(r)
def rnd(fmt, lo, hi, neg=False):
    P = FMT[fmt][0]
    e = random.randint(lo, hi)
    c = random.randint(10**(P-1), 10**P - 1)
    return ('-' if neg else '') + f'{c}E{e-(P-1)}'
def structured(fmt):
    P, emax, emin = FMT[fmt]
    ulp = lambda k: Dc(10) ** (k - P + 1)
    out = []
    for k in list(range(-60, 8)) + [emin, emin + 1, emax - 1, emax, -P-2, -P-3, -P-4, -2*P]:
        for s in ['1E%d' % k, '5E%d' % k, str(Dc(10)**(k+1) - ulp(k))]:
            out.append(s)
    for base in ['1', '2', '0.5', '10', '0.1', '3']:
        b = Dc(base)
        k = b.adjusted()
        for j in [1, 2, 3, 5, 10, 1000]:
            out.append(str(b + j*ulp(k))); out.append(str(b - j*ulp(k - (1 if b == Dc(10)**k else 0))))
    out.append(str(Dc(10)**emax - ulp(emax)))  # max finite
    out.append('1E%d' % (emin - P + 1))        # min subnormal
    return [s for s in (q(fmt, v) for v in out) if s]
UNARY_DOMAIN = {
  'exp': (-30, 4), 'exp2': (-30, 4), 'exp10': (-30, 3), 'expm1': (-60, 4), 'exp2m1': (-60, 4), 'exp10m1': (-60, 3),
  'ln': None, 'log2': None, 'log10': None, 'log1p': None, 'log2p1': None, 'log10p1': None,
  'sin': (-60, 6), 'cos': (-60, 6), 'tan': (-60, 6), 'sinpi': (-60, 6), 'cospi': (-60, 6), 'tanpi': (-60, 6),
  'asin': (-60, -1), 'acos': (-60, -1), 'atan': None, 'asinpi': (-60, -1), 'acospi': (-60, -1), 'atanpi': None,
  'sinh': (-60, 3), 'cosh': (-60, 3), 'tanh': (-60, 2), 'asinh': None, 'acosh': (0, 40), 'atanh': (-60, -1), 'cbrt': None,
}
lines = []
for fmt, (P, emax, emin) in FMT.items():
    st = structured(fmt)
    for f, dom in UNARY_DOMAIN.items():
        xs = set()
        for s in st:
            xs.add(s); xs.add('-' + s if not s.startswith('-') else s[1:])
        lo, hi = dom if dom else (emin, emax)
        for i in range(NRAND):
            xs.add(rnd(fmt, lo, hi, random.random() < 0.5))
        # gates
        gates = {
          'tanh': ['44.99', '45', '45.01', '39', '40', '41'],
          'sinh': ['14149.9', '14150', '14150.0001', '14150.04', '14150.0785', '14150.0786', '887', '887.0001', '887.1', '887.1884', '887.19', '224', '224.0001', '224.04', '224.044'],
          'cosh': ['14149.9', '14150', '14150.0001', '14150.04', '14150.0785', '14150.0786', '887', '887.0001', '887.1', '887.1884', '887.19', '224', '224.0001', '224.04', '224.044'],
          'exp': ['14149.38', '14149.3853', '14149.39', '14150', '14150.01', '-14220.8', '-14221', '-14221.1', '886.49', '886.5', '887', '-916.9', '-917', '-918', '223.35', '223.36', '224', '-233.2', '-233.3', '-235'],
          'exp2': ['20413', '20413.5', '20414', '20420', '-20516', '-20517', '-20518', '1279', '1280', '-1322', '-1323', '322', '323', '-335', '-336'],
          'exp10': ['6144.99', '6145', '6145.0001', '-6176.3', '-6176.4', '384.99', '385.0001', '-398.3', '96.99', '97.0001', '-101.3'],
          'expm1': ['14149.38', '14149.39', '14150', '14150.01', '886.49', '886.5', '887', '223.35', '223.36', '224'],
          'expm1': ['-119.9', '-120', '-120.1', '-110', '-107', '-115'],
          'exp2m1': [str(Dc(-120)/Dc('0.6931471805599453')), '-173', '-172', '-160'],
          'exp10m1': ['-52.1', '-52.2', '-52', '-47', '-48'],
          'atanpi': ['1E%d' % (P+1), '1E%d' % (P+2), '9.9E%d' % (P+1), '-1E%d' % (P+2)],
          'acospi': ['1E-%d' % (P+2), '1E-%d' % (P+3), '9.9E-%d' % (P+3), '-1E-%d' % (P+3)],
          'atan': ['0.4142135623730950488016887242096980785696718753769', '0.41421356', '1.0000001', '0.99999999'],
          'log10p1': ['1E%d' % n for n in (P, P+1, 35, 36, 37, 49, 50, 51)] + [str(Dc(10)**n + 1) for n in (5, P-2)],
          'log2p1': ['1E40', '1E50', '2E50', '4294967296', '4294967295'],
        }.get(f, [])
        for g in gates:
            v = q(fmt, g)
            if v: xs.add(v)
        for x in sorted(xs):
            lines.append(f'{fmt} {f} {x}')
    # binary
    for f in ['pow', 'powr', 'atan2', 'atan2pi']:
        pairs = set()
        for i in range(NRAND * 3):
            x = rnd(fmt, -20, 20, f.startswith('atan') and random.random() < 0.5)
            y = rnd(fmt, -20, 20, random.random() < 0.5) if f.startswith('atan') else rnd(fmt, -3, 3, random.random() < 0.5)
            pairs.add((x, y))
        for x in ['10', '0.1', '2', '5', '1E10', '1.000000000000000000000000000000001'[:P+1], '0.9999999999999999999999999999999999'[:P+2]]:
            for y in ['0.5', '-0.5', '1.5', '257', '-257', '1E-30', '1E5', '33', '1E-%d' % (P+3)]:
                if q(fmt, x) and q(fmt, y): pairs.add((q(fmt, x), q(fmt, y)))
        if f.startswith('atan'):
            for (y, x) in [('1', '1E%d' % (P+1)), ('1', '-1E%d' % (P+1)), ('1E%d' % (P+1), '1'), ('1E%d' % (P+2), '-1'), ('-1E%d' % (P+2), '-1'), ('1', '-1E%d' % (P+3)), ('-1', '-1E%d' % (P+2)), ('3', '-4'), ('-3', '-4')]:
                pairs.add((y, x))
        for (x, y) in sorted(pairs):
            lines.append(f'{fmt} {f} {x} {y}')
    for n in [1, 2, 3, -1, -3, 100, -100, 10**6, 2147483647, -2147483648]:
        for x in ['1E-%d' % (P + 2 + len(str(abs(n)))), '1E-%d' % (P + 3 + len(str(abs(n)))), '1E-%d' % (P + 4 + len(str(abs(n)))), '-1E-%d' % (P + 4 + len(str(abs(n)))), '0.5', '-0.5', '1E10', rnd(fmt, -40, 3)]:
            if q(fmt, x): lines.append(f'{fmt} compound {q(fmt, x)} {n}')
print('\n'.join(lines))
