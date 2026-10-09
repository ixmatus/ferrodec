import decimal, random, subprocess, sys
from decimal import Context, ROUND_HALF_EVEN, ROUND_HALF_UP, ROUND_DOWN, ROUND_CEILING, ROUND_FLOOR
PROBE = sys.argv[1]
FMTS = {"128": (34, -6143, 6144), "64": (16, -383, 384), "32": (7, -95, 96)}
RMS = {"NE": ROUND_HALF_EVEN, "NA": ROUND_HALF_UP, "TZ": ROUND_DOWN, "TP": ROUND_CEILING, "TN": ROUND_FLOOR}
rnd = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 1)
def gen():
    k = rnd.randrange(14)
    sign = rnd.choice(["", "-", "+"])
    if k == 0:
        return sign + "0"*rnd.randrange(0, 3000) + str(rnd.randrange(10**rnd.randrange(1,40))) + rnd.choice(["", "." + "0"*rnd.randrange(0,50)])
    if k == 1:
        return sign + "0." + "0"*rnd.randrange(0, 7000) + str(rnd.randrange(1,10**rnd.randrange(1,40)))
    if k == 2:
        digs = "".join(rnd.choice("0123456789") for _ in range(rnd.randrange(30, 200)))
        p = rnd.randrange(len(digs)+1)
        return sign + digs[:p] + "." + digs[p:] + rnd.choice(["", "E%+d" % rnd.randrange(-7000, 7000)])
    if k == 3:
        return sign + str(rnd.randrange(10**rnd.randrange(1,40))) + "E" + rnd.choice(["", "+", "-"]) + str(rnd.choice([rnd.randrange(10**12), rnd.randrange(6100, 6200), rnd.randrange(360, 420), rnd.randrange(80, 120), 999999, 1000000, 1000001, 2147483647, 2147483648, 99999999999]))
    if k == 4:
        return sign + "0" + rnd.choice(["", ".", ".000"]) + "E" + rnd.choice(["+", "-", ""]) + str(rnd.choice([0,1,2,3,95,96,101,102,383,384,398,399,6143,6144,6176,6177,6111,6112,90,91,369,370,10**11, 999999, 1000000, 1000001]))
    if k == 5:
        P, emin, emax = FMTS[rnd.choice(list(FMTS))]
        # near boundaries: coefficient P or P+1 digits, exponent near emax/etiny
        c = str(rnd.randrange(10**(P-1), 10**(P+2)))
        e = rnd.choice([emax - len(c) + rnd.randrange(-2, 3), emin - P + 1 - rnd.randrange(-3, 4) - len(c) + 1, emin - rnd.randrange(-2,3)])
        return sign + c + "E" + ("%+d" % e)
    if k == 6:
        return sign + rnd.choice(["NaN", "nan", "sNaN", "SNAN", "Inf", "inFinity"]) + rnd.choice(["", str(rnd.randrange(10**rnd.randrange(1, 36))), "0"*rnd.randrange(50) + "1"])
    if k == 7:
        P = rnd.choice([7, 16, 34])
        return sign + rnd.choice(["NaN", "sNaN"]) + str(rnd.choice([10**(P-1), 10**(P-1) - 1, 2**{7:20,16:50,34:110}[P] - 1, rnd.randrange(10**(P-1), 2**{7:20,16:50,34:110}[P])]))
    if k == 8:
        # sticky tails past 76 digits
        return sign + "1" + "0"*rnd.randrange(70, 90) + rnd.choice(["1", "0", "5", "50", "49"]) + rnd.choice(["", "E-80", "E+5"])
    if k == 9:
        return sign + "9"*rnd.randrange(30, 90) + rnd.choice(["", ".5", ".49999", "E+6100", "E-6200", "E+360", "E-400"])
    if k == 10:
        return rnd.choice([".", "+.", "-", "+", "1e", "1e+", "e5", ".e5", "1..2", "1 ", " 1", "1\x00", "1e1.5", "Infinit", "NaNx", "-NaN-1", "1_000", "١", "1E++5", "0x10", "Inf0", "infinityx", "1.e5", ".5e-3", "5.", "+0.E+0", "-Inf", "-sNaN0", "sNaN-", "NaN.", "1E5E5", ""])
    if k == 11:
        P, emin, emax = FMTS[rnd.choice(list(FMTS))]
        # subnormal edge with half-way tails
        etiny = emin - P + 1
        c = str(rnd.randrange(1, 10**rnd.randrange(1, P+3))) + rnd.choice(["5", "50", "51", "49", "0"])
        return sign + c + "E" + str(etiny - len(c) + rnd.randrange(-2, 3))
    if k == 12:
        # integer digits past 76 then fraction
        return sign + "1"*rnd.randrange(77, 100) + "." + "9"*rnd.randrange(1,10) + rnd.choice(["", "E-6250", "E-100"])
    return sign + "0"*rnd.randrange(80, 120) + "." + "0"*rnd.randrange(0, 50) + rnd.choice(["", "1", "E+5", "E-99999999999"])

N = int(sys.argv[3]) if len(sys.argv) > 3 else 20000
cases = []
for _ in range(N):
    s = gen()
    f = rnd.choice(list(FMTS)); rm = rnd.choice(list(RMS))
    if "\n" in s or " " in s and False: pass
    cases.append((f, rm, s))
inp = "".join("P %s %s %s\n" % c for c in cases)
if len(sys.argv) > 4: open(sys.argv[4], "w").write(inp); sys.exit(0)
out = subprocess.run([PROBE], input=inp.encode("utf-8"), capture_output=True).stdout.decode().splitlines()
assert len(out) == len(cases), (len(out), len(cases))
mism = 0
cls = {}
for (f, rm, s), o in zip(cases, out):
    P, emin, emax = FMTS[f]
    ctx = Context(prec=P, Emin=emin, Emax=emax, rounding=RMS[rm], clamp=1, traps=[])
    try:
        if any(ord(ch) > 127 for ch in s) or s != s.strip() or "_" in s: raise decimal.InvalidOperation
        r = ctx.create_decimal(s)
        conv = ctx.flags[decimal.InvalidOperation]
    except Exception:
        conv = True
    if conv:
        exp = "ERR"
    else:
        fl = ""
        if ctx.flags[decimal.Overflow]: fl += "O"
        if ctx.flags[decimal.Underflow]: fl += "U"
        if ctx.flags[decimal.Inexact]: fl += "X"
        if ctx.flags[decimal.Clamped]: fl += "C"
        exp = "%s %s" % (fl or "-", r)
    if o.startswith("ERR") or o.startswith("PANIC"):
        got = o if o.startswith("PANIC") else "ERR"
    else:
        parts = o.split(" ", 2)
        got = "%s %s" % (parts[1], parts[2])
    if got != exp:
        mism += 1
        import re as _re
        nangap = _re.match(r'^[+-]?s?NaN\d+$', s, _re.I) and exp == "ERR"
        cls[nangap and "nan_payload_gap" or "other"] = cls.get(nangap and "nan_payload_gap" or "other", 0) + 1
        if not nangap and cls["other"] <= 40:
            print("MISMATCH", f, rm, repr(s[:120]), len(s), "| got:", o[:150], "| exp:", exp[:150])
print("cases", len(cases), "mismatches", mism, cls)
