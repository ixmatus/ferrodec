import sys
sys.path.insert(0, '.')
from oracle2 import *
out = []
def add(fm, op, args, modes=MODES):
    for m in modes: out.append(f"{fm} {op} {m} {' '.join(map(str, args))}")
cfg = {'128': (10**17 + 5, 49), '64': (10**8 + 5, 23), '32': (5005, 11)}
for fm, (c, k) in cfg.items():
    p, emax = FMT[fm]
    for sh in (p + 6, p + 10, p + 30, emax // 2 - 30):
        add(fm, 'compound', [f"{c}E+{sh}", 2])
        add(fm, 'compound', [f"{c + 10}E+{sh}", 2])
        add(fm, 'compound', [f"{2**k}E+{sh}", -1])
        add(fm, 'compound', [f"{2**(k-1)}E+{sh}", -2]) if False else None
        add(fm, 'compound', [f"{2**k}E+{sh}", 1])
        add(fm, 'compound', [f"1E+{sh}", 1]); add(fm, 'compound', [f"1E+{sh}", -1]); add(fm, 'compound', [f"1E+{sh}", 3])
    # exact family x^n on huge x where (1+x)^n is still exactly near anchors: classifier runs first
open(sys.argv[1], 'w').write('\n'.join(out) + '\n'); print(len(out))
