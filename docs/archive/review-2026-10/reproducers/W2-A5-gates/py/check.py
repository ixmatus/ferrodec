"""Five-mode differential: reads driver output lines, computes mpmath truth at 120 dps,
rounds with CPython decimal at the target format, reports mismatches."""
import sys, decimal, mpmath
from decimal import Decimal as Dc
mpmath.mp.dps = 130
FMT = {'d128': (34, 6144, -6143), 'd64': (16, 384, -383), 'd32': (7, 96, -95)}
MODES = [decimal.ROUND_HALF_EVEN, decimal.ROUND_HALF_UP, decimal.ROUND_DOWN, decimal.ROUND_CEILING, decimal.ROUND_FLOOR]
MN = ['NE', 'NA', 'TZ', 'TP', 'TN']
mp = mpmath.mpf
def truth(func, x, y):
    X = mp(x)
    f = {
      'exp': lambda: mpmath.exp(X), 'exp2': lambda: mpmath.power(2, X), 'exp10': lambda: mpmath.power(10, X),
      'expm1': lambda: mpmath.expm1(X), 'exp2m1': lambda: mpmath.expm1(X*mpmath.ln2), 'exp10m1': lambda: mpmath.expm1(X*mpmath.ln10),
      'ln': lambda: mpmath.log(X), 'log2': lambda: mpmath.log(X)/mpmath.ln2, 'log10': lambda: mpmath.log10(X),
      'log1p': lambda: mpmath.log1p(X), 'log2p1': lambda: mpmath.log1p(X)/mpmath.ln2, 'log10p1': lambda: mpmath.log1p(X)/mpmath.ln10,
      'sin': lambda: mpmath.sin(X), 'cos': lambda: mpmath.cos(X), 'tan': lambda: mpmath.tan(X),
      'sinpi': lambda: mpmath.sinpi(X), 'cospi': lambda: mpmath.cospi(X), 'tanpi': lambda: mpmath.sinpi(X)/mpmath.cospi(X),
      'asin': lambda: mpmath.asin(X), 'acos': lambda: mpmath.acos(X), 'atan': lambda: mpmath.atan(X),
      'asinpi': lambda: mpmath.asin(X)/mpmath.pi, 'acospi': lambda: mpmath.acos(X)/mpmath.pi, 'atanpi': lambda: mpmath.atan(X)/mpmath.pi,
      'sinh': lambda: mpmath.sinh(X), 'cosh': lambda: mpmath.cosh(X), 'tanh': lambda: mpmath.tanh(X),
      'asinh': lambda: mpmath.asinh(X), 'acosh': lambda: mpmath.acosh(X), 'atanh': lambda: mpmath.atanh(X),
      'cbrt': lambda: mpmath.sign(X)*mpmath.cbrt(abs(X)),
      'pow': lambda: (mpmath.power(X, mp(y)) if X > 0 else (mpmath.power(-X, mp(y)) * (-1 if int(mp(y)) % 2 else 1))),
      'powr': lambda: mpmath.power(X, mp(y)),
      'atan2': lambda: mpmath.atan2(X, mp(y)), 'atan2pi': lambda: mpmath.atan2(X, mp(y))/mpmath.pi,
      'compound': lambda: mpmath.power(1+X, int(y)),
    }[func]
    return f()
def to_dec(v):
    if v == 0: return Dc(0)
    e = int(mpmath.floor(mpmath.log10(abs(v))))
    if e > 7000: return Dc('1E7000') * (1 if v > 0 else -1)
    if e < -7000: return Dc('1E-7000') * (1 if v > 0 else -1)
    m = v / mpmath.power(10, e)
    s = mpmath.nstr(m, 125, strip_zeros=False, min_fixed=-1, max_fixed=3)
    return Dc(s).scaleb(e)
def expo(s):
    try:
        d = Dc(s)
        return abs(d.adjusted()) if d != 0 and d.is_finite() else 0
    except Exception:
        return 0
DECIMAL_CTX = decimal.Context(prec=200, Emax=999999999, Emin=-999999999, traps=[])
decimal.setcontext(DECIMAL_CTX)
TRANSC = {'exp','expm1','ln','log1p','sin','cos','tan','asin','acos','atan','sinh','cosh','tanh','asinh','acosh','atanh','atan2'}
def rnd(ctx, d):
    ctx.clear_flags()
    return ctx.plus(d)
bad = amb = n = 0
for line in sys.stdin:
    parts = line.rstrip('\n').split('\t')
    head = parts[0].split()
    fmt, func, x = head[0], head[1], head[2]
    y = head[3] if len(head) > 3 else None
    cells = parts[1:]
    P, emax, emin = FMT[fmt]
    try:
        base = 130
        if func in ('sin', 'cos', 'tan'):
            base = 140 + expo(x)
        with mpmath.workdps(base):
            v = truth(func, x, y)
        if isinstance(v, mpmath.mpc) or not mpmath.isfinite(v):
            continue
        d = to_dec(v)
    except Exception as e:
        print('TRUTH-ERR', line.strip(), e); continue
    n += 1
    # snap to a short exact decimal when the truth is a (P+2)-digit decimal to 1e-110
    exact = None
    if d != 0:
        c = decimal.Context(prec=P + 2, rounding=decimal.ROUND_HALF_EVEN, Emax=999999999, Emin=-999999999, traps=[])
        s = c.plus(d)
        if abs(d.adjusted()) < 7000 and abs(DECIMAL_CTX.divide(d - s, d)) < Dc('1e-110'):
            dps = 140 + 3 * max(expo(x), expo(y) if y and func != 'compound' else 0) + abs(s.adjusted())
            if func in ('sin', 'cos', 'tan'):
                dps += expo(x)
            if func in ('exp', 'exp2', 'exp10', 'expm1', 'exp2m1', 'exp10m1', 'sinh', 'cosh', 'tanh', 'pow', 'powr', 'compound'):
                try:
                    dps += int(abs(Dc(x)) * 2) if abs(Dc(x)) < 100000 else 10**6
                except Exception:
                    pass
            if dps > 40000:
                amb += 1; print('AMBIG-DPS', fmt, func, x, y, cells); continue
            if func == 'compound':
                dps = min(30000, 140 + 3 * expo(x) + 12)
            with mpmath.workdps(dps):
                v2 = truth(func, x, y)
                rel = abs((v2 - mpmath.mpf(str(s))) / v2) if v2 != 0 else 0
                if rel < mpmath.power(10, -(dps - 20)):
                    if func in TRANSC or (func in ('exp2m1','exp10m1','exp2','exp10') and expo(x) > 3):
                        amb += 1; print('AMBIG-EXACT', fmt, func, x, y, cells); continue
                    exact = s
                else:
                    # not exact: perturb d toward the true side by a sub-ulp amount at 125 digits
                    side = 1 if v2 > mpmath.mpf(str(s)) else -1
                    d = s + side * abs(s) * Dc('1e-118')
    else:
        exact = Dc(0)
    for mi, (m, cell) in enumerate(zip(MODES, cells)):
        got, st = cell.split('#')
        st = int(st)
        ctx = decimal.Context(prec=P, rounding=m, Emax=emax, Emin=emin, clamp=1, traps=[])
        if exact is not None:
            want = rnd(ctx, exact)
            ok_amb = True
        else:
            eps = abs(d) * Dc('1e-122')
            lo, hi = rnd(ctx, d - eps), rnd(ctx, d + eps)
            want = rnd(ctx, d)
            if lo != hi or lo.compare_total(hi) != 0 and lo != hi:
                amb += 1; print('AMBIG', fmt, func, x, y, MN[mi]); continue
        want_inexact = (exact is None) or (want != exact)
        if got == 'PANIC':
            bad += 1; print('PANIC', fmt, func, x, y, MN[mi]); continue
        try:
            g = Dc(got.replace('Infinity', 'Infinity'))
        except Exception:
            bad += 1; print('PARSE', fmt, func, x, y, MN[mi], got); continue
        val_ok = (g == want)
        inexact_ok = bool(st & 16) == want_inexact
        if not val_ok or not inexact_ok:
            bad += 1
            print('MISMATCH', fmt, func, x, y or '', MN[mi], 'got', got, st, 'want', want, 'inexact_want', want_inexact, 'truth', mpmath.nstr(v, 45))
print(f'# checked {n} inputs, {bad} mismatching cells, {amb} ambiguous', file=sys.stderr)
