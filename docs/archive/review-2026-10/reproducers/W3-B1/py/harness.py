import decimal, subprocess, sys
from decimal import Decimal, Context
from fractions import Fraction
DRV = "/private/tmp/claude-502/-Users-parnell-Development-ferrodec/7aa0469f-7024-4109-8d3d-fac13748b08d/scratchpad/target-B1/release/b1drv"
MODES = {"ne": decimal.ROUND_HALF_EVEN, "na": decimal.ROUND_HALF_UP, "tz": decimal.ROUND_DOWN,
         "tp": decimal.ROUND_CEILING, "tn": decimal.ROUND_FLOOR}
FLAGS = [(decimal.InvalidOperation, 1), (decimal.DivisionByZero, 2), (decimal.Overflow, 4),
         (decimal.Underflow, 8), (decimal.Inexact, 16), (decimal.Clamped, 32)]

def ctx(m):
    c = Context(prec=34, Emax=6144, Emin=-6143, clamp=1, rounding=MODES[m], traps=[])
    return c

def todec(o):
    p = o.split(",")
    s = int(p[1])
    if p[0] == "F":
        c = int(p[2]); e = int(p[3])
        return Decimal((s, tuple(int(ch) for ch in str(c)), e))
    if p[0] == "I":
        return Decimal("-Infinity" if s else "Infinity")
    pl = p[2] if p[2] != "0" else ""
    k = "NaN" if p[0] == "Q" else "sNaN"
    return Decimal(("-" if s else "") + k + pl)

def fromdec(d):
    t = d.as_tuple()
    s = t.sign
    if t.exponent == "F":
        return f"I,{s}"
    if t.exponent in ("n", "N"):
        pl = int("".join(map(str, t.digits))) if t.digits else 0
        return f"{'Q' if t.exponent=='n' else 'S'},{s},{pl}"
    c = int("".join(map(str, t.digits)))
    return f"F,{s},{c},{t.exponent}"

def flagbits(c):
    b = 0
    for f, v in FLAGS:
        if c.flags[f]:
            b |= v
    return b

def F(s, c, e):
    return f"F,{s},{c},{e}"

def run_driver(lines):
    p = subprocess.run([DRV], input="\n".join(lines) + "\n", capture_output=True, text=True)
    if p.returncode != 0:
        sys.stderr.write(p.stderr[-3000:])
        raise RuntimeError("driver failed")
    return p.stdout.split("\n")[:len(lines)]

ORD = {-1: "Less", 0: "Equal", 1: "Greater"}

def ref(op, m, ops):
    c = ctx(m)
    d = [todec(o) for o in ops if not o.lstrip('-').isdigit()]
    if op == "add": r = c.add(d[0], d[1])
    elif op == "sub": r = c.subtract(d[0], d[1])
    elif op == "mul": r = c.multiply(d[0], d[1])
    elif op == "div": r = c.divide(d[0], d[1])
    elif op == "fma": r = c.fma(d[0], d[1], d[2])
    elif op == "sqrt": r = c.sqrt(d[0])
    elif op == "divint": r = c.divide_int(d[0], d[1])
    elif op == "remnear_mpd": r = c.remainder_near(d[0], d[1])
    elif op == "remtrunc_mpd": r = c.remainder(d[0], d[1])
    elif op == "quantize": r = c.quantize(d[0], d[1])
    elif op == "rti": r = c.to_integral_value(d[0])
    elif op == "rtie": r = c.to_integral_exact(d[0])
    elif op == "reduce": r = c.normalize(d[0])
    elif op == "min": r = c.min(d[0], d[1])
    elif op == "max": r = c.max(d[0], d[1])
    elif op == "minmag": r = c.min_mag(d[0], d[1])
    elif op == "maxmag": r = c.max_mag(d[0], d[1])
    elif op == "scaleb": r = c.scaleb(d[0], Decimal(int(ops[1])))
    elif op == "logb": r = c.logb(d[0])
    elif op == "nextup": r = c.next_plus(d[0])
    elif op == "nextdown": r = c.next_minus(d[0])
    elif op == "nexttoward": r = c.next_toward(d[0], d[1])
    elif op == "rotate": r = c.rotate(d[0], d[1])
    elif op == "shift": r = c.shift(d[0], d[1])
    elif op == "and": r = c.logical_and(d[0], d[1])
    elif op == "or": r = c.logical_or(d[0], d[1])
    elif op == "xor": r = c.logical_xor(d[0], d[1])
    elif op == "invert": r = c.logical_invert(d[0])
    elif op == "abs": r = c.abs(d[0]) if not d[0].is_nan() else d[0].copy_abs()
    elif op == "totcmp":
        return ORD[int(d[0].compare_total(d[1]))]
    elif op == "totmag":
        return ORD[int(d[0].compare_total_mag(d[1]))]
    elif op == "samequantum":
        return "true" if d[0].same_quantum(d[1]) else "false"
    elif op in ("cmp", "cmpsig"):
        r = c.compare(d[0], d[1]) if op == "cmp" else c.compare_signal(d[0], d[1])
        o = "None" if r.is_nan() else f"Some({ORD[int(r)]})"
        return f"{o} {flagbits(c)}"
    else:
        raise ValueError(op)
    return f"{fromdec(r)} {flagbits(c)}"

def exact_rem(x, y, near):
    # x, y finite operand strings, y nonzero. returns encoding with status 0
    dx, dy = todec(x), todec(y)
    fx, fy = Fraction(dx), Fraction(dy)
    q = fx / fy
    n = q.numerator // q.denominator
    if not near:
        if n < 0 and n * q.denominator != q.numerator: n += 1  # trunc
    else:
        fr = q - n
        if fr > Fraction(1, 2) or (fr == Fraction(1, 2) and n % 2 == 1): n += 1
    r = fx - n * fy
    e = min(dx.as_tuple().exponent, dy.as_tuple().exponent)
    coef = r / (Fraction(10) ** e)
    assert coef.denominator == 1
    cc = abs(coef.numerator)
    s = (1 if r < 0 else 0) if r != 0 else dx.as_tuple().sign
    return f"F,{s},{cc},{e} 0"

def compare(cases, refn=None, show=20):
    lines = [" ".join(cs) for cs in cases]
    got = run_driver(lines)
    bad = []
    for cs, g in zip(cases, got):
        w = (refn or (lambda cs: ref(cs[0], cs[1], cs[2:])))(cs)
        if w is None: continue
        if g != w:
            bad.append((" ".join(cs), g, w))
    return len(cases), bad

ORC = DRV.replace("b1drv", "orc")
def run_orc(lines):
    p = subprocess.run([ORC], input="\n".join(lines) + "\n", capture_output=True, text=True)
    if p.returncode != 0:
        sys.stderr.write(p.stderr[-3000:]); raise RuntimeError("orc failed")
    return p.stdout.split("\n")[:len(lines)]

def compare_orc(cases):
    lines = [" ".join(cs) for cs in cases]
    got = run_driver(lines); want = run_orc(lines)
    bad = []
    for cs, g, w in zip(cases, got, want):
        gv, gs = g.rsplit(" ", 1); wv, ws = w.rsplit(" ", 1)
        if gv.startswith("Q") and wv.startswith("Q"): gv = wv
        if gv != wv or (int(gs) & ~32) != int(ws):
            bad.append((" ".join(cs), g, w))
    return len(cases), bad
