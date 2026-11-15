import re, sys, mpmath
mpmath.mp.dps = 8300
def budget(tag, fn):
    if tag == "SC2": return 10000
    p = int(tag[3:]); return 10*(6*p+80)
for line in open(sys.argv[1]):
    m = re.match(r"(SC2|SCD\d+) (\d+) (-?\d+) (\w+) ([SC]) (?:sign=(\w+) coef=(\d+) exp=(-?\d+)|ExtendedDyn \{ coef: (\d+), exp: (-?\d+), sign: (\w+) \})", line)
    if not m: continue
    tag, c, q, neg, fn = m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)=="true", m.group(5)
    if m.group(6): sign, coef, e = m.group(6)=="true", int(m.group(7)), int(m.group(8))
    else: coef, e, sign = int(m.group(9)), int(m.group(10)), m.group(11)=="true"
    p = 110 if tag=="SC2" else int(tag[3:])
    x = mpmath.mpf(c)*mpmath.mpf(10)**q*(-1 if neg else 1)
    t = mpmath.sin(x) if fn=="S" else mpmath.cos(x)
    got = mpmath.mpf(coef)*mpmath.mpf(10)**e*(-1 if sign else 1)
    # predicate unit: one unit of the p-digit working coefficient of got
    nd = len(str(coef)); ulp = mpmath.mpf(10)**(e + nd - p)
    u = (got - t)/ulp
    B = budget(tag, fn)
    flag = "OVER" if abs(u) > B else ""
    print(f"{tag:7s} q={q:5d} neg={neg!s:5s} {fn} |val|~{mpmath.nstr(abs(t),3):>9s} err={mpmath.nstr(u,4):>10s} ulp  budget={B:>7d} {flag}")
