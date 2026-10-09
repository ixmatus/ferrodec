import sys, random
sys.path.insert(0, '.')
from oracle2 import *
random.seed(7)
out = []
def add(fm, op, args, modes=MODES):
    for m in modes: out.append(f"{fm} {op} {m} {' '.join(map(str, args))}")
def fits(fm, s):
    p, emax = FMT[fm]
    t = Decimal(s).as_tuple(); d = list(t.digits)
    while len(d) > 1 and d[-1] == 0: d.pop()
    adj = Decimal(s).adjusted()
    etiny = 2 - emax - p
    return len(d) <= p and adj <= emax and (Decimal(s).as_tuple().exponent >= etiny or True) and adj >= etiny
for fm in FMT:
    p, emax = FMT[fm]
    # pow with large b on powers of ten
    for k in (6000, 4096, 384, 96, 90, 1000, 2000, 6144):
        for b in (2, 4, 5, 8, 10, 16, 25, 32, 64, 100, 125, 128, 256, 1000, 1024, 2000, 4096, 6000):
            for a in (1, -1, 3, -3):
                x = f"1E+{k}"; y = str(Decimal(a) / Decimal(b))
                if fits(fm, x) and fits(fm, y): add(fm, 'pow', [x, y], ['NE', 'TZ'])
                x = f"1E-{k}"
                if fits(fm, x) and fits(fm, y): add(fm, 'pow', [x, y], ['NE', 'TZ'])
    for k in (6000, 3000, 384, 96, 90, 12):
        for n in (2, 3, 4, 5, 6, 10, 12, 1000, 3000, 6000, 1024, -2, -3, -6000, -3000):
            x = f"1E+{k}"
            if fits(fm, x): add(fm, 'rootn', [x, n], ['NE', 'TZ'])
            x = f"1E-{k}"
            if fits(fm, x): add(fm, 'rootn', [x, n], ['NE', 'TZ'])
    # compound ties via 1+x = 2^a 5^b / 10^d
    for x in ('0.024', '-0.36', '0.5625', '0.25', '-0.75', '0.6', '-0.9375', '0.28', '0.0016', '-0.96', '0.125', '0.3125', '0.5', '1.5', '0.05', '0.21', '24', '124', '-0.8', '0.44'):
        if not fits(fm, x): continue
        for n in list(range(-130, 131)):
            if n: add(fm, 'compound', [x, n], ['NE', 'NA', 'TZ', 'TP'])
    # pi family
    xs = ['0.5', '1.5', '2.5', '0.25', '0.75', '1.25', '1.75', '-0.5', '-1.5', '-0.25', '-0.75', '-1.25', '3', '-3', '2', '-2', '1', '-1', '0.125', '0.1', '1E+40', '1.0', '10E-1', '-10E-1', '0.50', '2.50E+1', '25E-1', '7.5', '3.25E+2', '1234567890123456.75', '1234567890123456.5', '0.3', '0.2', '1E-6176', '9999999999999999999999999999999.5', '999999999999999999999999999999.25', '0.0625']
    for x in xs:
        if not fits(fm, x): continue
        for op in ('sinpi', 'cospi', 'tanpi', 'asinpi', 'acospi', 'atanpi'):
            add(fm, op, [x], ['NE', 'TZ', 'TN'])
    for (y, x) in (('1', '1'), ('1', '-1'), ('-1', '1'), ('-1', '-1'), ('3', '3.0'), ('3E+5', '30E+4'), ('1', '2'), ('2', '1'), ('1E-6176', '1E-6176'), ('5', '-5.00')):
        if fits(fm, x) and fits(fm, y): add(fm, 'atan2pi', [y, x], ['NE', 'TZ', 'TN'])
open(sys.argv[1], 'w').write('\n'.join(out) + '\n'); print(len(out))
