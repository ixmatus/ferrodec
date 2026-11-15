"""Worst-case cancellation depth of the Payne-Hanek reduction over a
decimal format's whole input set.

For x = c * 10^q (1 <= c < 10^P) the reduced argument is
r = (pi/2) * y with y = x*(2/pi) - round(x*(2/pi)), so the number of
leading zeros of |y| is the cancellation depth. Over all c < 10^P at a
fixed q, min_c ||c*theta_q|| with theta_q = frac(10^q * 2/pi) is attained
at the last continued-fraction convergent denominator q_k < 10^P
(best-approximation theorem). We take theta_q from the decimal
expansion of 2/pi (mpmath, 6400 digits) and evaluate ||q_k*theta|| with
a 160-digit rational, so the only approximation is 10^-160 * q_k, far
below anything reported.

Only inputs with |x| >= 0.1 take the windowed path (decade >= -1), so
q ranges over [-(P), q_max] where q_max is the largest unbiased
exponent for a normalized coefficient.
"""
import sys, math
from fractions import Fraction
import mpmath

mpmath.mp.dps = 6500
two_over_pi = mpmath.mpf(2) / mpmath.pi
digits = mpmath.nstr(two_over_pi, 6450, strip_zeros=False)  # "0.6366..."
assert digits.startswith("0.")
D = digits[2:]  # d_1 d_2 ...

def theta(q, width=160):
    # frac(10^q * 2/pi): the digits from position q+1 on (q >= 0);
    # for q < 0 the value is 2/pi * 10^q itself, whose fraction is all of it.
    if q >= 0:
        s = D[q:q + width]
    else:
        s = ("0" * (-q) + D)[:width]
    return Fraction(int(s), 10 ** width)

def min_dist_over_c(th, cmax):
    """min over 1 <= c < cmax of ||c*th||, via convergents."""
    # continued fraction of th
    p0, q0, p1, q1 = 0, 1, 1, 0
    x = th
    best = None
    while True:
        a = math.floor(x)
        p2, q2 = a * p1 + p0, a * q1 + q0
        if q2 >= cmax:
            break
        if q2 >= 1:
            d = abs(q2 * th - p2)
            best = (d, q2, p2) if best is None or d < best[0] else best
        p0, q0, p1, q1 = p1, q1, p2, q2
        frac = x - a
        if frac == 0:
            break
        x = 1 / frac
    return best

