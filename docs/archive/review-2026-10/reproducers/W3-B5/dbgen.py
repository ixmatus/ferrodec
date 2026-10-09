import sys, random, math
B = 10**9
def limbs_to_int(l): return sum(x * B**i for i, x in enumerate(l))
SPEC = [0, 1, 2, B-1, B-2, B//2, B//2-1, B//2+1, 999999998, 10**8]
def num(rng, maxl):
    n = rng.randrange(1, maxl+1)
    k = rng.random()
    if k < 0.35: return limbs_to_int([rng.choice(SPEC) for _ in range(n)])
    if k < 0.5: e = rng.randrange(0, 9*maxl); return max(0, 10**e + rng.choice([-1,0,1]))
    return limbs_to_int([rng.randrange(B) for _ in range(n)])
def lead0(rng, x):
    s = str(x)
    return "0"*rng.choice([0,0,0,1,8,9,10]) + s
def gen(seed, n):
    rng = random.Random(seed); out = []
    for _ in range(n):
        maxl = rng.choice([2,3,5,8,40,80])
        a, b = num(rng, maxl), num(rng, maxl)
        out.append(f"dbadd {lead0(rng,a)} {lead0(rng,b)}")
        out.append(f"dbsub {max(a,b)} {min(a,b)}")
        out.append(f"dbmul {a} {b}")
        out.append(f"dbcmp {lead0(rng,a)} {b}")
        d = b or 1
        out.append(f"dbdiv {a*rng.choice([1, B, B**2]) + rng.randrange(0, 10)} {d}")
        k = rng.randrange(0, 60)
        out.append(f"dbmulp {a} {k}")
        out.append(f"dbdrp {a} {rng.randrange(0, 9*maxl+20)}")
        out.append(f"dbisqrt {a}")
        out.append(f"dbdig {lead0(rng,a)}")
        out.append(f"dbu128 {a}")
        out.append(f"dbdr10 {a}")
    for k in range(0, 400):
        out.append(f"dbpow10 {k}")
        for v in (10**k-1, 10**k, 10**k+1):
            if v >= 0: out.append(f"dbdig {v}")
    print("\n".join(out))
def check(cf, rf):
    bad=0; n=0
    for c, r in zip(open(cf).read().split("\n"), open(rf).read().split("\n")):
        if not c: continue
        n+=1; f=c.split(); op=f[0]; a=[int(x) for x in f[1:]]; rr=r.split()
        if op=="dbadd": ok = int(rr[0])==a[0]+a[1]
        elif op=="dbsub": ok = int(rr[0])==a[0]-a[1]
        elif op=="dbmul": ok = int(rr[0])==a[0]*a[1]
        elif op=="dbcmp": ok = int(rr[0])==(a[0]>a[1])-(a[0]<a[1])
        elif op=="dbdiv": ok = int(rr[0])==a[0]//a[1] and int(rr[1])==a[0]%a[1]
        elif op=="dbmulp": ok = int(rr[0])==a[0]*10**a[1]
        elif op=="dbdrp": ok = int(rr[0])==a[0]//10**a[1] and int(rr[1])==a[0]%10**a[1]
        elif op=="dbisqrt": s=math.isqrt(a[0]); ok = int(rr[0])==s and int(rr[1])==a[0]-s*s
        elif op=="dbdig": ok = int(rr[0])==(len(str(a[0])) if a[0] else 1)
        elif op=="dbu128": ok = (rr[0]=="None") if a[0]>=1<<128 else int(rr[0])==a[0]
        elif op=="dbdr10": ok = int(rr[0])==a[0]//10 and int(rr[1])==a[0]%10
        elif op=="dbpow10": ok = rr[0]==str(10**a[0])
        # Display canonical form: no leading zeros
        ok = ok and all(t=="None" or t=="0" or not t.startswith("0") or t.startswith("-") for t in rr)
        if not ok:
            bad+=1
            if bad<10: print("MISMATCH", c[:200], "->", r[:200])
    print(f"checked {n} cases, {bad} mismatches")
if sys.argv[1]=="gen": gen(int(sys.argv[2]), int(sys.argv[3]))
else: check(sys.argv[2], sys.argv[3])
