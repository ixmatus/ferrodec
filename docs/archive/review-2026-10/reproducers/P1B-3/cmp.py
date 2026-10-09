import sys, random, subprocess, decimal
from fractions import Fraction
from math import isqrt

FMT = {'128': (34, 6144, -6143), '64': (16, 384, -383), '32': (7, 96, -95)}
RM = {'ne': decimal.ROUND_HALF_EVEN, 'na': decimal.ROUND_HALF_UP, 'tz': decimal.ROUND_DOWN,
      'tp': decimal.ROUND_CEILING, 'tn': decimal.ROUND_FLOOR}
INV, DBZ, OVF, UNF, INX = 1, 2, 4, 8, 16

def ctx(f, rm):
    p, emax, emin = FMT[f]
    return decimal.Context(prec=p, Emax=emax, Emin=emin, rounding=RM[rm], clamp=1, traps=[])

def mpd(f, rm, op, args):
    c = ctx(f, rm)
    # operands must be exact: create via Decimal(str) (no context rounding)
    a = [decimal.Decimal(x) for x in args]
    if op == 'add': r = c.add(a[0], a[1])
    elif op == 'sub': r = c.subtract(a[0], a[1])
    elif op == 'mul': r = c.multiply(a[0], a[1])
    elif op == 'div': r = c.divide(a[0], a[1])
    elif op == 'rem': r = c.remainder_near(a[0], a[1])
    elif op == 'fma': r = c.fma(a[0], a[1], a[2])
    elif op == 'sqrt': r = c.sqrt(a[0])
    fl = 0
    F = c.flags
    if F[decimal.InvalidOperation]: fl |= INV
    if F[decimal.DivisionByZero]: fl |= DBZ
    if F[decimal.Overflow]: fl |= OVF | INX
    if F[decimal.Underflow]: fl |= UNF | INX
    if F[decimal.Inexact]: fl |= INX
    return r, fl, bool(F[decimal.Clamped])

def norm_dec(r):
    if r.is_nan(): return 'NaN'
    if r.is_infinite(): return ('-' if r.is_signed() else '') + 'Infinity'
    s, digs, e = r.as_tuple()
    coeff = int(''.join(map(str, digs))) if digs else 0
    return ('-' if s else '') + f'{coeff}E{e}'

def frac_of(s):
    d = decimal.Decimal(s)
    return Fraction(d)  # exact

def round_frac(v, f, rm):
    """Independent value rounding: returns (Fraction or 'inf', flags)."""
    p, emax, emin = FMT[f]
    qmin, qmax = emin - p + 1, emax - p + 1
    if v == 0: return Fraction(0), 0
    neg = v < 0; a = -v if neg else v
    # floor(log10(a))
    k = len(str(a.numerator)) - len(str(a.denominator))
    while Fraction(10) ** k > a: k -= 1
    while Fraction(10) ** (k + 1) <= a: k += 1
    q = max(k - p + 1, qmin)
    sc = a / (Fraction(10) ** q)
    fl_ = sc.numerator // sc.denominator
    frac = sc - fl_
    inexact = frac != 0
    up = False
    if inexact:
        if rm == 'tz': up = False
        elif rm == 'tp': up = not neg
        elif rm == 'tn': up = neg
        elif rm == 'na': up = frac >= Fraction(1, 2)
        elif rm == 'ne': up = frac > Fraction(1, 2) or (frac == Fraction(1, 2) and fl_ % 2 == 1)
    m = fl_ + (1 if up else 0)
    if m == 10 ** p: m //= 10; q += 1
    flags = INX if inexact else 0
    if q > qmax:
        to_inf = rm in ('ne', 'na') or (rm == 'tp' and not neg) or (rm == 'tn' and neg)
        if to_inf: return ('-inf' if neg else 'inf'), OVF | INX
        return (-1 if neg else 1) * Fraction(10 ** p - 1) * Fraction(10) ** qmax, OVF | INX
    if inexact and a < Fraction(10) ** emin: flags |= UNF
    return (-1 if neg else 1) * m * Fraction(10) ** q, flags

def sqrt_round(sx, f, rm):
    """Independent sqrt value rounding via integer sqrt bracketing."""
    p, emax, emin = FMT[f]
    qmin = emin - p + 1
    x = frac_of(sx)
    if x == 0: return Fraction(0), 0
    # find q: 10^q scale such that sqrt(x)/10^q in [10^(p-1), 10^p) or q=qmin
    # sqrt(x) >= 10^k  <=> x >= 10^(2k)
    k = 0
    while Fraction(10) ** (2 * k) > x: k -= 1
    while Fraction(10) ** (2 * (k + 1)) <= x: k += 1
    q = max(k - p + 1, qmin)
    # s = sqrt(x)/10^q ; s^2 = x / 10^(2q)
    t = x / Fraction(10) ** (2 * q)
    # floor(sqrt(t)) for rational t: isqrt(floor(t)) works since floor(sqrt(t)) = isqrt(floor(t))
    fl_ = isqrt(t.numerator // t.denominator)
    inexact = Fraction(fl_) ** 2 != t
    up = False
    if inexact:
        if rm == 'tp': up = True
        elif rm in ('ne', 'na'):
            # compare sqrt(t) with fl_+1/2  <=> t vs (fl_+1/2)^2 ; equality impossible for rational sqrt? possible if t is a perfect square of half-integer
            h = (Fraction(2 * fl_ + 1, 2)) ** 2
            if t > h: up = True
            elif t == h: up = (rm == 'na') or (fl_ % 2 == 1)
    m = fl_ + (1 if up else 0)
    if m == 10 ** p: m //= 10; q += 1
    flags = INX if inexact else 0
    if inexact and x < Fraction(10) ** (2 * emin): flags |= UNF
    return m * Fraction(10) ** q, flags

def exact_value(op, args):
    x = [frac_of(s) for s in args]
    if op == 'add': return x[0] + x[1]
    if op == 'sub': return x[0] - x[1]
    if op == 'mul': return x[0] * x[1]
    if op == 'div': return x[0] / x[1]
    if op == 'fma': return x[0] * x[1] + x[2]
    if op == 'rem':
        qq = x[0] / x[1]
        n = qq.numerator // qq.denominator
        fr = qq - n
        if fr > Fraction(1, 2) or (fr == Fraction(1, 2) and n % 2 == 1): n += 1
        return x[0] - n * x[1]
    return None

def run_oracle(lines):
    exe = sys.argv[1]
    p = subprocess.run([exe], input='\n'.join(lines) + '\n', capture_output=True, text=True)
    if p.returncode != 0:
        print('ORACLE CRASH', p.stderr[-2000:]); 
    return p.stdout.strip().split('\n')

def oracle_val(s):
    if s in ('Infinity', '-Infinity', 'NaN'): return s
    return Fraction(decimal.Decimal(s))

def check(cases, label):
    lines = [' '.join([f, rm, op] + args) for (f, rm, op, args) in cases]
    outs = run_oracle(lines)
    assert len(outs) == len(cases), (len(outs), len(cases))
    nbad_mpd = nbad_fr = 0; clamped_cases = 0
    for (case, out) in zip(cases, outs):
        f, rm, op, args = case
        ostr, ofl = out.split(); ofl = int(ofl)
        r, mfl, mcl = mpd(f, rm, op, args)
        mstr = norm_dec(r)
        if mcl: clamped_cases += 1
        skip_mpd = (op == 'sqrt' and rm != 'ne')
        if op == 'sqrt':
            sv, sfl = sqrt_round(args[0], f, rm)
            if oracle_val(ostr) != sv or ofl != sfl:
                nbad_fr += 1
                if nbad_fr <= 25: print(f'[{label}] SQRT-FRAC-DIFF {f} {rm} {args}: oracle={ostr} fl={ofl} frac={sv} fl={sfl}')
        if not skip_mpd and (ostr, ofl) != (mstr, mfl):
            nbad_mpd += 1
            if nbad_mpd <= 25:
                print(f'[{label}] MPD-DIFF {f} {rm} {op} {args}: oracle={ostr} fl={ofl}  mpd={mstr} fl={mfl} clamped={mcl}')
        # Fraction value check (not for sqrt, not for NaN)
        if op in ('add','sub','mul','div','fma','rem') and ostr != 'NaN':
            v = exact_value(op, args)
            if op == 'rem':
                fv, ffl = v, 0  # exact
                # rem may be subnormal but exact
            else:
                fv, ffl = round_frac(v, f, rm)
            ov = oracle_val(ostr)
            fvn = {'inf': 'Infinity', '-inf': '-Infinity'}.get(fv, fv) if isinstance(fv, str) else fv
            if ov != fvn or (op != 'rem' and ofl != ffl):
                nbad_fr += 1
                if nbad_fr <= 25:
                    print(f'[{label}] FRAC-DIFF {f} {rm} {op} {args}: oracle={ostr} fl={ofl} frac={fv} fl={ffl}')
    print(f'[{label}] {len(cases)} cases: mpd mismatches {nbad_mpd}, fraction mismatches {nbad_fr}, mpd-clamped cases {clamped_cases}')
