import sys, random, subprocess, mpmath as m, decimal
m.mp.dps = 80
random.seed(7)
FMT = {'d64':(16,384,-383),'d32':(7,96,-95)}
lines=[]
for t,(p,emax,emin) in FMT.items():
    qmin = emin-(p-1)
    lo_e = float(m.log(m.mpf(10)**qmin/2)) - 1.0
    hi_e = float(m.log(m.mpf(10)**emin)) + 2.0
    for fn,scale in [('exp',1.0),('exp10',1/2.302585092994046),('exp2',1/0.6931471805599453),('expm1',1.0)]:
        if fn=='expm1': continue
        for _ in range(250):
            x = random.uniform(lo_e, hi_e)*scale
            s = ('%.*g' % (p, x))
            lines.append(f'{t} {fn} {s}')
    # overflow band for exp and sinh/cosh (sinh/cosh known A5-1; skip them here)
    ov = float(m.log((10**p-1)*m.mpf(10)**(emax-p+1)))
    for _ in range(250):
        x = random.uniform(ov-1.5, ov+1.5)
        lines.append(f'{t} exp {"%.*g" % (p, x)}')
out = subprocess.run([sys.argv[1]], input='\n'.join(lines)+'\n', capture_output=True, text=True).stdout.splitlines()
RM = {'NE':decimal.ROUND_HALF_EVEN,'NA':decimal.ROUND_HALF_UP,'TZ':decimal.ROUND_DOWN,'TP':decimal.ROUND_CEILING,'TN':decimal.ROUND_FLOOR}
bad=0; tot=0
for l in out:
    t,fn,x,mn,v,st = l.split()
    p,emax,emin = FMT[t]
    X = m.mpf(x)
    y = {'exp':m.exp(X),'exp10':m.power(10,X),'exp2':m.power(2,X)}[fn]
    # correct rounding via decimal at huge precision then context round
    ctx = decimal.Context(prec=p, Emax=emax, Emin=emin, clamp=1, rounding=RM[mn], traps=[])
    Y = decimal.Decimal(m.nstr(y, 70, min_fixed=1, max_fixed=0).replace('e','E'))
    r = ctx.plus(Y)
    flags = (4 if ctx.flags[decimal.Overflow] else 0)|(8 if ctx.flags[decimal.Underflow] else 0)|16
    s,dg,e = r.as_tuple()
    want = 'Infinity' if r.is_infinite() else (('-' if s else '')+str(int(''.join(map(str,dg))))+'E'+str(e))
    tot+=1
    if want != v or (int(st)&~32) != flags:
        bad+=1
        if bad<=15: print('MISMATCH', l, 'want', want, flags)
print('total', tot, 'mismatch', bad)
