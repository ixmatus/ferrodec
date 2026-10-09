import subprocess, sys
probe = sys.argv[1]
def dec(d):
    b = [(d >> i) & 1 for i in range(10)]
    p,q,r,s,t,u,v,w,x,y = b[9],b[8],b[7],b[6],b[5],b[4],b[3],b[2],b[1],b[0]
    if v == 0: return (4*p+2*q+r, 4*s+2*t+u, 4*w+2*x+y)
    wx = 2*w + x
    if wx == 0: return (4*p+2*q+r, 4*s+2*t+u, 8+y)
    if wx == 1: return (4*p+2*q+r, 8+u, 4*s+2*t+y)
    if wx == 2: return (8+r, 4*s+2*t+u, 4*p+2*q+y)
    st = 2*s + t
    if st == 0: return (8+r, 8+u, 4*p+2*q+y)
    if st == 1: return (8+r, 4*p+2*q+u, 8+y)
    if st == 2: return (4*p+2*q+r, 8+u, 8+y)
    return (8+r, 8+u, 8+y)
def enc_canon(n):
    # canonical declet: search the unique d with dec(d)==digits and canonical (pq=00 when st=11,wx=11,v=1)
    for d in range(1024):
        if dec(d) == ((n//100), (n//10)%10, n%10):
            b = d
            if (d>>3)&1 and (d>>1)&3 == 3 and (d>>5)&3 == 3 and (d>>8)&3 != 0: continue
            return d
lines = []; exp = []
for fmt, width, base in (("32", 8, 0), ("64", 16, 0), ("128", 32, 0)):
    for d in range(1024):
        a, b_, c = dec(d); val = 100*a + 10*b_ + c
        for pos in (0, 1):
            bits = base | (d << (10*pos))
            lines.append("FROMDPD %s NE %0*x" % (fmt, width, bits))
            canon = enc_canon(val) << (10*pos)
            exp.append((fmt, d, pos, val * (1000**pos), canon))
out = subprocess.run([probe], input="".join(l+"\n" for l in lines).encode(), capture_output=True).stdout.decode().splitlines()
bad = 0
for (fmt, d, pos, val, canon), o in zip(exp, out):
    bidbits, disp, dpd = o.split(" ")
    bid = int(bidbits, 16)
    coef = bid & {"32": (1<<23)-1, "64": (1<<53)-1, "128": (1<<113)-1}[fmt]
    dpdv = int(dpd.split("=")[1], 16)
    if coef != val or dpdv != canon:
        bad += 1
        if bad < 10: print("BAD", fmt, hex(d), pos, val, o)
print("checked", len(exp), "bad", bad)
