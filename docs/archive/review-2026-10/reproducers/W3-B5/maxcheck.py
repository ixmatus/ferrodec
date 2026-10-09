import mpmath, sys, time
gens = {"pi": (lambda: mpmath.pi, 0), "two_over_pi": (lambda: 2/mpmath.pi, -1), "ln2": (lambda: mpmath.log(2), -1),
        "ln10": (lambda: mpmath.log(10), 0), "e": (lambda: mpmath.e, 0), "tan_pi_8": (lambda: mpmath.sqrt(2)-1, -1),
        "inv_ln2": (lambda: 1/mpmath.log(2), 0), "inv_ln10": (lambda: 1/mpmath.log(10), -1), "inv_pi": (lambda: 1/mpmath.pi, -1)}
for name in sys.argv[1:]:
    n = 99976 if name == "inv_pi" else 100000
    got = int(open(f"maxdepth/{name}.txt").read().strip())
    f, E = gens[name]
    t = time.time()
    mpmath.mp.dps = n + 40
    c = f()
    want = int(mpmath.floor(c * mpmath.mpf(10)**(n - 1 - E)))
    mpmath.mp.dps = n + 90
    want2 = int(mpmath.floor(f() * mpmath.mpf(10)**(n - 1 - E)))
    print(f"{name} n={n} digits={len(str(got))} diff={got-want} (oracle self-agree {want==want2}) {time.time()-t:.1f}s", flush=True)
