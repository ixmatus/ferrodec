import sys, glob
from fractions import Fraction
from flint import arb, ctx, fmpq
sys.path.insert(0,".")
from true_margins_lib import margins, tofrac, q
FN={"sin":arb.sin,"cos":arb.cos,"tan":arb.tan}
import collections
for fn in ("sin","cos","tan"):
    hist=collections.Counter(); n=0; below=0; mins=[]
    for path in glob.glob("/Users/parnell/.local/share/ferrodec-campaign/s2/s2_%s_d128_decades_s*.tsv"%fn):
        for line in open(path):
            if not line.startswith("S"): continue
            p=line.split("\t"); x=p[3]
            xv=q(x); e=int(x.split("E")[1])
            ctx.prec=int(abs(e)*3.33)+300
            y=FN[fn](arb(xv.p)/arb(xv.q))
            g,t=margins(tofrac(y),34); m=min(g,t)
            n+=1; mins.append(m)
            if m<1e-6: below+=1
    mins.sort()
    print(fn,"S rows=",n,"true<1e-6:",below,"true min=%.3e"%mins[0],"median=%.3e"%mins[len(mins)//2])
