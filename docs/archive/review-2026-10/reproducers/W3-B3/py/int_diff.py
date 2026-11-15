import decimal, random, subprocess, sys
from decimal import Decimal, Context, ROUND_HALF_EVEN, ROUND_HALF_UP, ROUND_DOWN, ROUND_CEILING, ROUND_FLOOR
probe = sys.argv[1]; rnd = random.Random(7)
FMTS = {"128": (34, -6143, 6144), "64": (16, -383, 384), "32": (7, -95, 96)}
RMS = {"NE": ROUND_HALF_EVEN, "NA": ROUND_HALF_UP, "TZ": ROUND_DOWN, "TP": ROUND_CEILING, "TN": ROUND_FLOOR}
TYPES = [("i32", -2**31, 2**31-1), ("i64", -2**63, 2**63-1), ("u64", 0, 2**64-1), ("i128", -2**127, 2**127-1), ("u128", 0, 2**128-1), ("u32", 0, 2**32-1)]
def gen(P):
    k = rnd.randrange(6); s = rnd.choice(["", "-"])
    if k == 0: return s + str(rnd.randrange(10**P)) + "E" + str(rnd.randrange(-P-3, 3))
    if k == 1:
        b = rnd.choice([2**31, 2**32, 2**63, 2**64, 2**127, 2**128]); v = b + rnd.randrange(-3, 3)
        d = str(v); return s + d[:P] + ("." + d[P:] if len(d) > P else "") if rnd.random()<0.5 else s + d[:P] + "E+" + str(max(0,len(d)-P))
    if k == 2: return s + str(rnd.randrange(1, 10**P)) + "E" + str(rnd.randrange(-6200, 6200))
    if k == 3: return s + rnd.choice(["0.5", "1.5", "2.5", "0.4999999", "0.5000001", "0", "0E+50", "0E-50", "Inf", "NaN", "sNaN", "0.0000001"])
    if k == 4: return s + str(rnd.randrange(10**(P-1), 10**P)) + "E" + str(rnd.randrange(-P-2, 40-P))
    return s + "1E" + str(rnd.randrange(0, 45))
cases = []
for _ in range(30000):
    f = rnd.choice(list(FMTS)); rm = rnd.choice(list(RMS)); P = FMTS[f][0]
    cases.append((f, rm, gen(P)))
out = subprocess.run([probe], input="".join("INT %s %s %s\n" % c for c in cases).encode(), capture_output=True).stdout.decode().splitlines()
bad = 0
for (f, rm, s), o in zip(cases, out):
    P, emin, emax = FMTS[f]
    ctx = Context(prec=P, Emin=emin, Emax=emax, rounding=ROUND_HALF_EVEN, clamp=1, traps=[])
    d = ctx.create_decimal(s)
    toks = o.split(" ")
    got = {toks[i].split("=")[0]: (toks[i].split("=")[1], toks[i+1]) for i in range(0, len(toks), 2)}
    for name, lo, hi in TYPES:
        if d.is_nan() or d.is_infinite():
            exp_flags = "I"; exp_val = None
        else:
            r = d.to_integral_value(rounding=RMS[rm])
            iv = int(r)
            if iv < lo or iv > hi: exp_flags = "I"; exp_val = None
            else:
                exp_val = str(iv); exp_flags = "X" if r != d else "-"
        gv, gf = got[name]
        if gf != exp_flags or (exp_val is not None and gv != exp_val):
            bad += 1
            if bad < 15: print("BAD", f, rm, s, name, "got", gv, gf, "exp", exp_val, exp_flags)
print("checked", len(cases)*6, "bad", bad)
