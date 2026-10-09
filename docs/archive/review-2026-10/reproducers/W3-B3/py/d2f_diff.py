import random, struct, subprocess, sys, collections, math
from decimal import Decimal
probe = sys.argv[1]; rnd = random.Random(13)
FMTS = {"128": (34, -6176, 6111), "64": (16, -398, 369), "32": (7, -101, 90)}
cases = []
for f, (P, qmin, qmax) in FMTS.items():
    for _ in range(6000):
        c = rnd.randrange(1, 10**rnd.randrange(1, P+1))
        e = rnd.choice([rnd.randrange(max(qmin,-360), min(qmax,330)), rnd.randrange(-340, -300), rnd.randrange(-20, 20), rnd.randrange(qmin, qmax+1)])
        cases.append((f, rnd.choice(["", "-"]) + "%dE%d" % (c, e)))
out = subprocess.run([probe], input="".join("D2F %s NE %s\n" % c for c in cases).encode(), capture_output=True).stdout.decode().splitlines()
cnt = collections.Counter(); ex = {}
for (f, s), o in zip(cases, out):
    bits, fl, _ = o.split(" ")
    d = Decimal(s)
    try: x = float(d)
    except OverflowError: x = math.copysign(float("inf"), d)
    eb = "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]
    inexact = math.isinf(x) or Decimal(x) != d
    tiny = (x == 0 or (abs(x) < 2.2250738585072014e-308)) or abs(d) < Decimal(2.2250738585072014e-308)
    efl = ("O" if math.isinf(x) else "") + ("U" if tiny and inexact else "") + ("X" if inexact else "")
    efl = efl or "-"
    k = (f, "val_ok" if bits == eb else "VAL_BAD", "flags_ok" if fl == efl else "FLAGS %s vs %s" % (fl, efl))
    cnt[k] += 1
    ex.setdefault(k, (s, o))
for k in sorted(cnt): print(k, cnt[k], ex[k] if "BAD" in k[1] or "FLAGS" in k[2] else "")
