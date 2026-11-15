# Exhaustive: for every 7-digit-or-shorter coefficient c (not divisible by 10)
# and q in 3..6, the true relative distance of c^-q (scaled) to the nearest
# Decimal32 rounding boundary (grid point or midpoint): min(r, D-r)/D half-ulps
# where 2*10^K/c^q = T + r/D with T in [2e6, 2e7).
import sys
q=int(sys.argv[1])
best=[]
import math
for c in range(1,10**7):
    if c%10==0: continue
    D=c**q
    # choose K so that 2*10^K // D in [2*10^6, 2*10^7)
    K=len(str(D))+6
    T,r=divmod(2*10**K, D)
    if T>=2*10**7: K-=1; T,r=divmod(2*10**K, D)
    elif T<2*10**6: K+=1; T,r=divmod(2*10**K, D)
    if r==0: continue  # exact / tie: classified input side
    d=min(r,D-r)
    # distance in half-ulps = d/D ; relative distance = (d/D)*(half-ulp)/(value) = (d/D)/T
    rel=d/(D*T)
    if len(best)<5 or rel<best[-1][0]:
        best.append((rel,c,T)); best.sort(); best=best[:5]
print("q=%d five smallest true relative distances:"%q, [("%.3e"%b[0],b[1]) for b in best])
