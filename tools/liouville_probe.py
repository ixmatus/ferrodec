"""Falsification probe for the ADR-0060 Liouville floors (fd-4zo.25).

Usage: python3 tools/liouville_probe.py   (stdlib only; ~2 min; exits
nonzero on any floor violation, which is a lane-stopping finding).
Deterministic: fixed seeds, no wall-clock or platform dependence.

The ADR-0060 lemma claims per-operation lower bounds ("floors") on the relative
distance between a true result and every format rounding boundary
(grid point or nearest-mode midpoint). The real formats cannot be
enumerated, so this probe checks the SAME floor formulas on scaled-down
precisions P where exhaustive enumeration is affordable. A single input
below its floor falsifies the lemma; near-floor inputs confirm the
bound is close to sharp rather than vacuous.

Floors under test (P = format precision in digits; boundaries are
grid values with <= P significant digits and midpoints with exactly
P+1 digits ending in 5):

  rSqrt : y0 = m^(-1/2), m in [2, 10^(P+1)), y0 in (10^(-(P+1)/2), 1]
          G_min = -(ceil((P+1)/2) + (P+1))
          rel dist >= 10^(2*G_min) / 2.01
  rootn3: y0 = m^(-1/3), G_min = -(ceil((P+1)/3) + (P+1))
          rel dist >= 10^(3*G_min) / 3.01
  pown3 : y = a^3, a in [2, 10^P): rel dist >= 10^-(3P+1)
  pown-3: y = a^-3: rel dist >= 10^-(4P+2)
  hypot : per-case: rel dist >= 1 / (8*S*1.01), S = a1^2*10^(2D) + a2^2
          (from |4S - (2M+1)^2| >= 1 resp. |S - M^2| >= 1)

Boundary distances are measured against the half-quantum lattice of the
value's own decade and both adjacent decades, keeping only lattice
points that are genuine boundaries of their own decade.
"""

from decimal import Decimal, getcontext
import math
import random

getcontext().prec = 150
D = Decimal


def rel_dist_to_boundaries(y: Decimal, P: int):
    """Min relative distance from y > 0 to any grid/midpoint boundary.

    Returns (rel, exact_hit). Lattice: in decade t (values in
    [10^t, 10^(t+1))) boundaries are the multiples of 10^(t-P+1)/2.
    A lattice point from a coarser decade inside a finer one is still a
    boundary (coarse lattice is a sublattice); the converse is not, so
    candidates are kept only inside their own decade [10^t, 10^(t+1)].
    """
    t = y.adjusted()
    best = None
    for tt in (t - 1, t, t + 1):
        h = D(10) ** (tt - P + 1) / 2  # half-quantum, exact decimal
        lo, hi = D(10) ** tt, D(10) ** (tt + 1)
        j = int(y / h)
        for dj in (-2, -1, 0, 1, 2):
            b = (j + dj) * h
            if b <= 0 or b < lo or b > hi:
                continue
            d = abs(y - b)
            if best is None or d < best:
                best = d
    rel = best / y
    return rel, rel < D(10) ** -120


def strip25(m: int) -> int:
    while m % 2 == 0:
        m //= 2
    while m % 5 == 0:
        m //= 5
    return m


def probe_rsqrt(P: int, m_iter, label: str):
    g_min = -(math.ceil((P + 1) / 2) + (P + 1))
    floor = D(10) ** (2 * g_min) / D("2.01")
    worst = (None, None)
    exact_hits = 0
    for m in m_iter:
        y0 = 1 / D(m).sqrt()
        rel, exact = rel_dist_to_boundaries(y0, P)
        if exact:
            # Exactness criterion cross-check: 1/sqrt(m) terminating
            # forces stripped m == 1 (m = 2^a * 5^b), the
            # terminating-reciprocal criterion.
            assert strip25(m) == 1, f"exact rSqrt hit at m={m} but m has other prime factors"
            exact_hits += 1
            continue
        assert rel >= floor, f"FALSIFIED rSqrt P={P}: m={m} rel={rel:.3E} < floor={floor:.3E}"
        if worst[0] is None or rel < worst[0]:
            worst = (rel, m)
    print(f"rSqrt  {label}: floor={floor:.2E}  observed min rel={worst[0]:.3E} at m={worst[1]}  exact hits={exact_hits}")


def probe_rootn3(P: int, m_iter, label: str):
    g_min = -(math.ceil((P + 1) / 3) + (P + 1))
    floor = D(10) ** (3 * g_min) / D("3.01")
    worst = (None, None)
    exact_hits = 0
    third = 1 / D(3)
    for m in m_iter:
        y0 = D(m) ** -third
        rel, exact = rel_dist_to_boundaries(y0, P)
        if exact:
            exact_hits += 1
            continue
        assert rel >= floor, f"FALSIFIED rootn3 P={P}: m={m} rel={rel:.3E} < floor={floor:.3E}"
        if worst[0] is None or rel < worst[0]:
            worst = (rel, m)
    print(f"rootn3 {label}: floor={floor:.2E}  observed min rel={worst[0]:.3E} at m={worst[1]}  exact hits={exact_hits}")


def probe_pown3(P: int, label: str):
    floor_pos = D(10) ** -(3 * P + 1)
    floor_neg = D(10) ** -(4 * P + 2)
    worst_p = (None, None)
    worst_n = (None, None)
    exact_p = exact_n = 0
    for a in range(2, 10**P):
        y = D(a) ** 3
        rel, exact = rel_dist_to_boundaries(y, P)
        if exact:
            exact_p += 1
        else:
            assert rel >= floor_pos, f"FALSIFIED pown3 P={P}: a={a} rel={rel:.3E}"
            if worst_p[0] is None or rel < worst_p[0]:
                worst_p = (rel, a)
        yn = 1 / y
        rel, exact = rel_dist_to_boundaries(yn, P)
        if exact:
            exact_n += 1
        else:
            assert rel >= floor_neg, f"FALSIFIED pown-3 P={P}: a={a} rel={rel:.3E}"
            if worst_n[0] is None or rel < worst_n[0]:
                worst_n = (rel, a)
    print(f"pown3  {label}: floor={floor_pos:.2E}  observed min rel={worst_p[0]:.3E} at a={worst_p[1]}  exact hits={exact_p}")
    print(f"pown-3 {label}: floor={floor_neg:.2E}  observed min rel={worst_n[0]:.3E} at a={worst_n[1]}  exact hits={exact_n}")


def probe_hypot(P: int, n_random: int, label: str):
    """Random pairs plus the constructed S = k^2 + k midpoint-hugging
    family; per-case floor 1/(8*S*1.01)."""
    rng = random.Random(20260805)
    worst = (None, None)
    checked = 0
    cases = []
    for _ in range(n_random):
        a1 = rng.randrange(1, 10**P)
        a2 = rng.randrange(1, 10**P)
        delta = rng.randrange(0, P + 3)
        cases.append((a1, a2, delta))
    # Constructed family: a1 = j^2, a2 = j*10^(delta/2), S = k^2 + k.
    for j in range(2, 10 ** (P // 2)):
        for delta in (0, 2, 4):
            if j * j < 10**P and j * 10 ** (delta // 2) < 10**P:
                cases.append((j * j, j * 10 ** (delta // 2), delta))
    for a1, a2, delta in cases:
        S = a1 * a1 * 10 ** (2 * delta) + a2 * a2
        y = D(S).sqrt()
        rel, exact = rel_dist_to_boundaries(y, P)
        if exact:
            r = math.isqrt(S)
            assert r * r == S, f"exact hypot hit but S={S} not a perfect square"
            continue
        # Per-case floor, both lattice regimes: boundaries near sqrt(S)
        # in decade t are multiples of 5*10^(t-P); for t >= P they are
        # (half-)integers and |4S - (2M+1)^2| >= 1 gives 1/(8S); for
        # t < P the lattice is fractional and the denominator picks up
        # 10^(2(P-t)). Conservative constant 8.04 covers both.
        t = y.adjusted()
        floor = D(10) ** (2 * min(t - P, 0)) / (8 * D(S) * D("1.005"))
        assert rel >= floor, f"FALSIFIED hypot P={P}: ({a1},{a2},{delta}) rel={rel:.3E} < {floor:.3E}"
        checked += 1
        # Track worst rel/floor ratio: sharpness of the per-case bound.
        ratio = rel / floor
        if worst[0] is None or ratio < worst[0]:
            worst = (ratio, (a1, a2, delta))
    print(f"hypot  {label}: {checked} cases; min observed rel/floor ratio={worst[0]:.3f} at {worst[1]} (1.0 = bound attained)")


if __name__ == "__main__":
    print("== P = 4 (exhaustive) ==")
    probe_rsqrt(4, range(2, 10**5), "P=4 all m<1E5")
    probe_rootn3(4, range(2, 10**5), "P=4 all m<1E5")
    probe_pown3(4, "P=4 all a<1E4")
    probe_hypot(4, 20000, "P=4")
    print("== P = 5 (exhaustive rSqrt; sampled rootn3) ==")
    probe_rsqrt(5, range(2, 10**6), "P=5 all m<1E6")
    rng = random.Random(1)
    sample = [rng.randrange(2, 10**6) for _ in range(200_000)]
    probe_rootn3(5, sample, "P=5 sampled 2E5")
    print("ALL FLOORS HELD")
