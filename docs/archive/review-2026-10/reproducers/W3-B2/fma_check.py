import decimal
from decimal import Decimal as D
from fractions import Fraction as F
RM = {'NE':decimal.ROUND_HALF_EVEN,'NA':decimal.ROUND_HALF_UP,'TZ':decimal.ROUND_DOWN,'TP':decimal.ROUND_CEILING,'TN':decimal.ROUND_FLOOR}
cases = [
 ('TZ','-100000000000001E1','1000000001E-382','1000000000000001E-396'),
 ('TN','-100000000000001E1','1000000001E-382','1000000000000001E-396'),
 ('NE','1000000000000001E-16','5000000000000001E-383','-100000000000000E-366'),
 ('TZ','-5000000000000001E15','-5000000000000001E0','-1500000000000000E0'),
 ('NE','1000000000000E1','5000000000000001E-32','-10000000000000E0'),
]
for m,a,b,c in cases:
    ctx = decimal.Context(prec=16, Emax=384, Emin=-383, clamp=1, rounding=RM[m], traps=[])
    r = ctx.fma(D(a),D(b),D(c))
    exact = F(D(a))*F(D(b))+F(D(c))
    print(m, a, b, c, '->', r, [k.__name__ for k,v in ctx.flags.items() if v], 'exact=', D(exact.numerator)/D(exact.denominator) if False else '%.25e' % float(0) )
    print('   exact numerator/denominator digits:', D(a)*D(b) if False else decimal.Context(prec=200).fma(D(a),D(b),D(c)))
