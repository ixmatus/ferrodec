import decimal, random, struct, subprocess, sys
from decimal import Decimal, Context, ROUND_HALF_EVEN, ROUND_HALF_UP, ROUND_DOWN, ROUND_CEILING, ROUND_FLOOR
probe = sys.argv[1]; rnd = random.Random(11)
FMTS = {"128": (34, -6143, 6144), "64": (16, -383, 384), "32": (7, -95, 96)}
RMS = {"NE": ROUND_HALF_EVEN, "NA": ROUND_HALF_UP, "TZ": ROUND_DOWN, "TP": ROUND_CEILING, "TN": ROUND_FLOOR}
def f2b(x): return struct.unpack("<Q", struct.pack("<d", x))[0]
cases = []
# targeted: f64 nearest to P-digit midpoints (double-rounding bait)
for P in (7, 16):
    for _ in range(4000):
        e = rnd.randrange(-20, 20)
        c = rnd.randrange(10**(P-1), 10**P)
        mid = Decimal(c * 10 + 5).scaleb(e)
        x = float(mid)
        cases.append(({7:"32",16:"64"}[P], rnd.choice(list(RMS)), x))
for _ in range(6000):
    x = struct.unpack("<d", struct.pack("<Q", rnd.getrandbits(64)))[0]
    if x != x or x in (float("inf"), float("-inf")): continue
    cases.append((rnd.choice(list(FMTS)), rnd.choice(list(RMS)), x))
out = subprocess.run([probe], input="".join("F2D %s %s %016x\n" % (f, rm, f2b(x)) for f, rm, x in cases).encode(), capture_output=True).stdout.decode().splitlines()
stats = {}
ex = {}
for (f, rm, x), o in zip(cases, out):
    P, emin, emax = FMTS[f]
    ctx = Context(prec=P, Emin=emin, Emax=emax, rounding=RMS[rm], clamp=1, traps=[])
    exact = ctx.create_decimal(Decimal(x))   # correctly rounded from exact binary value
    fl = ("O" if ctx.flags[decimal.Overflow] else "") + ("U" if ctx.flags[decimal.Underflow] else "") + ("X" if ctx.flags[decimal.Inexact] else "") + ("C" if ctx.flags[decimal.Clamped] else "")
    toks = o.split(" ", 2)
    got = Decimal(toks[2]); gf = toks[1]
    short = repr(x)
    sd = len(Decimal(short).as_tuple().digits)
    key = (f, "value_ok" if got.compare_total(exact) == 0 or got == exact else "value_diff", "short>P" if sd > P else "short<=P")
    stats[key] = stats.get(key, 0) + 1
    if key[1] == "value_diff" and key[2] == "short>P" and key not in ex:
        ex[key] = (rm, short, toks[2], str(exact), gf, fl or "-")
for k in sorted(stats): print(k, stats[k])
for k, v in ex.items(): print("EXAMPLE", k, "rm=%s f64=%s got=%s correctly_rounded=%s gotflags=%s expflags=%s" % v)
