import sys, random
sys.path.insert(0, '.')
from oracle2 import *
random.seed(int(sys.argv[2]))
out = []
def rep(fm, m, e, sg=0):
    p, emax = FMT[fm]
    m, e = strip(m, e)
    if m == 0 or len(str(m)) > p: return None
    etiny = 2 - emax - p
    if e < etiny or e + len(str(m)) - 1 > emax: return None
    return ('-' if sg else '') + f"{m}E{e:+d}"
def add(fm, op, args, modes):
    for md in modes: out.append(f"{fm} {op} {md} {' '.join(map(str, args))}")
def rmodes(): return random.sample(MODES, 2)
for fm in FMT:
    p, emax = FMT[fm]
    for _ in range(6000):
        # pow: x = (s * 2^i * 5^j)^b * 10^(b k), y = a/b
        b = random.choice([1, 2, 4, 5, 8, 10, 16, 20, 25, 32, 40, 50, 64, 80, 100, 125])
        s = random.choice([1, 1, 1, 3, 7, 9, 11, 13, 21, 99, 3**5, 7**3])
        i = random.randint(0, 8); j = random.randint(0, 8) if i == 0 else 0
        base = s * 2 ** i * 5 ** j
        if len(str(base ** b)) > p: continue
        k = random.randint(-(emax // b) - 2, emax // b)
        x = rep(fm, base ** b, b * k)
        a = random.choice([1, 2, 3, 5, 7, 11, 49, 50, -1, -2, -3, -7, -49, random.randint(-200, 200)])
        if a == 0: continue
        yd = Decimal(a) / Decimal(b)
        if yd.as_tuple().exponent < -30: continue
        y = str(yd.normalize()) if random.random() < .5 else str(yd)
        if x and len(Decimal(y).as_tuple().digits) <= p: add(fm, 'pow', [x, y], rmodes())
    for _ in range(4000):
        # rootn: x = r^n
        n = random.randint(2, 60); r = random.choice([2, 3, 5, 7, 10, 12, 15, 25, 99, 125, 1024, 4, 8, 16, 32])
        if len(str(r ** n)) > p: continue
        k = random.randint(-(emax // n), emax // n)
        x = rep(fm, r ** n, n * k)
        if x: add(fm, 'rootn', [x, n * random.choice([1, -1])], rmodes())
    for _ in range(6000):
        # compound: 1 + x = 2^i 5^j t / 10^d
        d = random.randint(0, 2 * p + 4)
        t = random.choice([1, 1, 3, 7, 9, 11, 21, 101])
        i = random.randint(0, 60); j = random.randint(0, 25) if random.random() < .5 else 0
        if i and j: i = 0
        N = 2 ** i * 5 ** j * t
        if N == 10 ** d: continue
        mx = N - 10 ** d
        x = rep(fm, abs(mx), -d, 1 if mx < 0 else 0)
        n = random.choice([random.randint(-60, 60), random.randint(-130, 130), random.randint(-6, 6)])
        if x and n: add(fm, 'compound', [x, n], rmodes())
    for _ in range(3000):
        # powi random s, exponent
        s = random.choice([1, 2, 3, 5, 7, 4, 8, 25, 125, 15, 35, 11, 2 ** 10, 5 ** 7])
        k = random.randint(-emax - p, emax)
        x = rep(fm, s, k)
        n = random.choice([random.randint(-130, 130), random.randint(-6, 6), random.randint(-6200, 6200)])
        if x and n: add(fm, 'powi', [x, n], rmodes())
    for _ in range(3000):
        # hypot: random triples
        m = random.randint(2, 10 ** (p // 2 + 1)); nn = random.randint(1, m - 1)
        A, B = m * m - nn * nn, 2 * m * nn
        g = random.choice([1, 1, 3, 5, 7, 25])
        e = random.randint(-emax - p, emax - p)
        sa, sb = rep(fm, A * g, e), rep(fm, B * g, e + random.choice([0, 0, 1, -1, 2]))
        if sa and sb: add(fm, 'hypot', [sa, sb], rmodes())
    for _ in range(2000):
        i = random.randint(0, 120); j = random.randint(0, 50)
        if i and j: i = 0
        m = 4 ** i * 25 ** j
        k = random.randint(-emax // 2, emax // 2)
        x = rep(fm, m, 2 * k + random.choice([0, 0, 0, 1]))
        if x: add(fm, 'rsqrt', [x], rmodes())
open(sys.argv[1], 'w').write('\n'.join(out) + '\n'); print(len(out))
