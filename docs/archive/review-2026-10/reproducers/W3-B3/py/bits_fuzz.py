import random, subprocess, sys, collections
probe = sys.argv[1]; rnd = random.Random(5)
W = {"128": 128, "64": 64, "32": 32}
lines = []
for f, w in W.items():
    for _ in range(30000):
        k = rnd.randrange(5)
        b = rnd.getrandbits(w)
        if k == 1: b = (b & ~(0b11111 << (w-6))) | (rnd.choice([0b11000,0b11001,0b11010,0b11011,0b11100,0b11101,0b11110,0b11111]) << (w-6))
        if k == 2: b = b & ((1 << (w-6)) - 1) | (rnd.randrange(0, 0b11000) << (w-6))   # form A
        if k == 3: b = (b & ((1 << (w//2)) - 1)) | (rnd.getrandbits(6) << (w-6))
        for cmd in ("RT", "B"):
            lines.append("%s %s NE %0*x" % (cmd, f, w//4, b))
out = subprocess.run([probe], input="".join(l + "\n" for l in lines).encode(), capture_output=True).stdout.decode().splitlines()
cnt = collections.Counter(); ex = {}
for l, o in zip(lines, out):
    cmd, f = l.split()[0], l.split()[1]
    if o.startswith("PANIC"): key = (f, cmd, "PANIC " + o[:80])
    elif cmd == "RT" and o != "ok":
        # classify
        kinds = sorted(set(tok.split(":")[0] for tok in o[4:].split(" ") if ":" in tok))
        nan = "nan" if "NaN" in o else "num"
        key = (f, cmd, "BAD " + ",".join(kinds) + " " + nan)
    else: key = (f, cmd, "ok")
    cnt[key] += 1
    if key not in ex: ex[key] = (l, o[:300])
for k in sorted(cnt): print(k, cnt[k])
for k, v in ex.items():
    if "ok" not in k[2]: print("EX", k, v)
