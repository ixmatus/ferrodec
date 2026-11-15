import sys, random
B = 10**9
def L(l): return sum(x * B**i for i, x in enumerate(l))
rng = random.Random(7)
cases = []
def sec(name, pairs):
    for a, b in pairs: cases.append(f"{a} {b}")
    cases.append(f"COUNT {name}")
# 1. the in-tree witness (June's directed operands)
sec("witness", [(L([0,0,500000000,499999999]), L([1,0,500000000]))])
# 2. random full-limb operands, widths 2..12 limbs
pairs = []
for _ in range(200000):
    n = rng.randrange(2, 8); m = rng.randrange(0, 6)
    v = L([rng.randrange(B) for _ in range(n-1)] + [rng.randrange(1, B)])
    u = L([rng.randrange(B) for _ in range(n+m)])
    pairs.append((u, v))
sec("random", pairs)
# 3. Knuth-derived family (TAOCP 4.3.1 D3/D4): with v1 >= B/2 (so d = 1),
# take the dividend window equal to qhat*(v1*B + v2)*B^(n-2) exactly, so the
# two-limb D3 test cannot reject qhat (rhat*B + u[j+n-2] equals qhat*v2),
# while the nonzero low limbs L of v make qhat*v exceed the window by
# qhat*L > 0: D4 goes negative and D6 adds back.
pairs = []
for _ in range(20000):
    n = rng.randrange(3, 9)
    v1 = rng.randrange(B//2, B); v2 = rng.randrange(B)
    low = [rng.randrange(B) for _ in range(n-2)]
    if all(x == 0 for x in low): low[0] = 1
    v = L(low + [v2, v1])
    qhat = rng.randrange(1, B)
    top = qhat * (v1 * B + v2)          # two-limb product, exact
    j = rng.randrange(0, 4)
    u = top * B**(n-2) * B**j + rng.randrange(0, B**j if j else 1)
    if u >= v * B**(j+1): continue      # keep the quotient limb in range
    pairs.append((u, v))
sec("knuth_family_d1", pairs)
# 4. the same family scaled so the normalisation factor d > 1 is in play:
# divide a normalised (u', v') pair by d when d divides both exactly.
pairs = []
for _ in range(20000):
    n = rng.randrange(3, 7)
    d = rng.randrange(2, 50)
    v1 = rng.randrange(B//(2*d), B//d)
    v = L([rng.randrange(1, B) for _ in range(n-2)] + [rng.randrange(B), v1])
    vp = v * d
    vl = [(vp // B**i) % B for i in range(n)]
    if vl[n-1] < B//2 or vp >= B**n or B // (v1 + 1) != d: continue
    qhat = rng.randrange(1, B)
    up = qhat * (vl[n-1] * B + vl[n-2]) * B**(n-2)
    if up % d: continue
    u = up // d
    pairs.append((u, v))
sec("knuth_family_dgt1", pairs)
print("\n".join(cases))
