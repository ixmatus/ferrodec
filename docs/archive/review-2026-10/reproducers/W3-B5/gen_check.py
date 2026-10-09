"""Generate multiword differential cases (Python int oracle) and check results.
usage: gen_check.py gen SEED N > cases.txt ; gen_check.py check cases.txt results.txt"""
import sys, random, math

def H(x): return hex(x)
def isqrt(n): return math.isqrt(n)
def digits(n): return 1 if n == 0 else len(str(n))

SPECIAL_LIMBS = [0, 1, 2, (1<<128)-1, (1<<128)-2, 1<<127, (1<<127)-1, 1<<64, (1<<64)-1, 10**38, 10**38-1]

def operand(rng, w):
    nl = w // 128
    k = rng.random()
    if k < 0.3:
        return sum(rng.choice(SPECIAL_LIMBS) << (128*i) for i in range(nl))
    if k < 0.5:
        e = rng.randrange(0, int(w*0.30103)+1)
        return max(0, 10**e + rng.choice([-1, 0, 1])) % (1<<w)
    if k < 0.6:
        b = rng.randrange(0, w+1)
        return ((1 << b) - rng.choice([0, 1])) % (1 << w) if b > 0 else rng.choice([0,1])
    b = rng.randrange(1, w+1)
    return rng.getrandbits(b)

def u128op(rng):
    k = rng.random()
    if k < 0.3: return rng.choice(SPECIAL_LIMBS[1:])
    if k < 0.5: return 10**rng.randrange(0,39)
    return rng.getrandbits(rng.randrange(1,129)) or 1

def gen(seed, n):
    rng = random.Random(seed)
    out = []
    M = lambda w: (1<<w)
    for _ in range(n):
        w = rng.choice([256,384,512,768,1024])
        a, b = operand(rng, w), operand(rng, w)
        out.append(f"{w}add {H(a)} {H(b)}")
        hi, lo = max(a,b), min(a,b)
        out.append(f"{w}sub {H(hi)} {H(lo)}")
        out.append(f"{w}cmp {H(a)} {H(b)}")
        out.append(f"{w}dr10 {H(a)}")
        out.append(f"{w}dig {H(a)}")
        m = u128op(rng)
        if w in (256, 768, 1024):
            x = a
            while x * m >= M(w): x >>= rng.randrange(1, 64)
            out.append(f"{w}mulu {H(x)} {H(m)}")
        if w in (256, 384, 512):
            x = a
            while x*10 >= M(w): x >>= 1
            out.append(f"{w}mul10 {H(x)}")
        if w in (256, 384):
            k = rng.randrange(0, 77 if w == 256 else 116)
            x = a
            while x * 10**k >= M(w): x >>= rng.randrange(1, 32)
            out.append(f"{w}mulp {H(x)} {k}")
            out.append(f"{w}divu {H(a)} {H(m)}")
        if w in (384, 512):
            out.append(f"{w}shr {H(a)} {rng.choice([0,1])}")
        if w == 768:
            out.append(f"768shr {H(a)} {rng.choice([0,1])}")
            out.append(f"768x {H(operand(rng,384))} {H(operand(rng,384))}")
        if w == 1024:
            out.append(f"1024x {H(operand(rng,768))} {H(m)}")
            c = operand(rng, 1024); d = operand(rng, 1024)
            # bias checked_mul toward the boundary
            if rng.random() < 0.7:
                c = rng.getrandbits(rng.randrange(1, 1024)); d = rng.getrandbits(min(1024, max(1, 1024 - c.bit_length() + rng.randrange(-3, 4))))
            out.append(f"1024cm {H(c)} {H(d)}")
        if w == 256:
            x = a
            if x.bit_length() > 254: x >>= (x.bit_length()-254)
            out.append(f"256isqrt {H(x)}")
            out.append(f"wmul {H(u128op(rng))} {H(u128op(rng))}")
    print("\n".join(out))

def check(cases, results):
    bad = 0; n = 0
    cl = open(cases).read().split("\n"); rl = open(results).read().split("\n")
    for c, r in zip(cl, rl):
        if not c: continue
        f = c.split(); op = f[0]; n += 1
        w = int(''.join(ch for ch in op if ch.isdigit()) or 0) if op[0].isdigit() else 256
        args = [int(x, 16) if x.startswith("0x") else int(x) for x in f[1:]]
        M = 1 << w
        rr = r.split()
        def hv(i): return int(rr[i], 16)
        kind = op.lstrip("0123456789")
        ok = True
        if kind == "add": ok = hv(0) == (args[0]+args[1]) % M
        elif kind == "sub": ok = hv(0) == (args[0]-args[1]) % M
        elif kind == "cmp": ok = int(rr[0]) == (args[0] > args[1]) - (args[0] < args[1])
        elif kind == "dr10": ok = hv(0) == args[0]//10 and int(rr[1]) == args[0] % 10
        elif kind == "dig": ok = int(rr[0]) == digits(args[0])
        elif kind == "mulu": ok = hv(0) == args[0]*args[1]
        elif kind == "mul10": ok = hv(0) == args[0]*10
        elif kind == "mulp": ok = hv(0) == args[0]*10**args[1]
        elif kind == "divu": ok = hv(0) == args[0]//args[1] and hv(1) == args[0] % args[1]
        elif kind == "shr":
            a, ps = args; s = 0; st = bool(ps)
            limit = 1 << (256 if w in (384,512) else 384)
            while a >= limit:
                st = st or (a % 10 != 0); a //= 10; s += 1
            ok = hv(0) == a and int(rr[1]) == s and bool(int(rr[2])) == st
        elif op == "768x": ok = hv(0) == args[0]*args[1]
        elif op == "1024x": ok = hv(0) == args[0]*args[1]
        elif op == "1024cm":
            p = args[0]*args[1]
            ok = (rr[0] == "None") if p >= M else (rr[0] != "None" and hv(0) == p)
        elif op == "256isqrt":
            s = isqrt(args[0]); ok = hv(0) == s and hv(1) == args[0]-s*s
        elif op == "wmul": ok = hv(0) == args[0]*args[1]
        else: raise SystemExit("unknown "+op)
        if not ok:
            bad += 1
            if bad < 20: print("MISMATCH", c, "->", r)
    print(f"checked {n} cases, {bad} mismatches")

if sys.argv[1] == "gen": gen(int(sys.argv[2]), int(sys.argv[3]))
else: check(sys.argv[2], sys.argv[3])
