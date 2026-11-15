from harness import *
cs=[("fma",m,F(0,1,0),F(0,1,40),F(1,1,-100)) for m in MODES]+[("fma",m,F(0,1,6100),F(0,1,45),F(1,1,6000)) for m in MODES]+[("fma",m,F(0,1,6100),F(0,1,44),F(1,1,6000)) for m in MODES]+[("fma",m,F(0,1,6100),F(0,1,45),F(1,1,6110)) for m in MODES]+[("fma",m,F(1,1,6100),F(0,1,45),F(0,1,6000)) for m in MODES]
n,bad=compare(cs); print(n,len(bad)); [print(b) for b in bad]
