import random, subprocess, sys
from decimal import Decimal
probe = sys.argv[1]; rnd = random.Random(3)
FMTS = {"128": (34, -6176, 6111), "64": (16, -398, 369), "32": (7, -101, 90)}
cases = []
for f, (P, qmin, qmax) in FMTS.items():
    for _ in range(8000):
        k = rnd.randrange(4)
        c = 0 if k == 0 else rnd.randrange(10**rnd.randrange(1, P+1))
        e = rnd.choice([rnd.randrange(qmin, qmax+1), rnd.randrange(-12, 6), rnd.choice([qmin, qmax, qmin+1, qmax-1])])
        s = rnd.choice(["", "-"]) + str(c) + "E" + str(e)
        cases.append((f, s))
lines = []
for f, s in cases:
    lines.append("P %s NE %s" % (f, s)); lines.append("ENG %s NE %s" % (f, s))
out = subprocess.run([probe], input="".join(l + "\n" for l in lines).encode(), capture_output=True).stdout.decode().splitlines()
bad = 0
for i, (f, s) in enumerate(cases):
    sci = out[2*i].split(" ", 2)[2]; eng = out[2*i+1]; st = out[2*i].split(" ")[1]
    d = Decimal(s)
    if st != "-" or sci != str(d) or eng != d.to_eng_string():
        bad += 1
        if bad < 10: print("BAD", f, s, "|", out[2*i], "|", eng, "| py", str(d), d.to_eng_string())
print("checked", len(cases), "bad", bad)
