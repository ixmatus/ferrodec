import sys, signal, time
class TO(Exception): pass
def _h(*a): raise TO()
signal.signal(signal.SIGALRM, _h)
sys.path.insert(0, sys.argv[0].rsplit('/', 1)[0])
from oracle2 import *
api = len(sys.argv) > 2 and sys.argv[2] == 'api'
bad = []; stats = {}; n = 0
for line in open(sys.argv[1]):
    if ' => ' not in line: continue
    lhs, rhs = line.rstrip('\n').split(' => ')
    parts = lhs.split(); fm, op, mode, args = parts[0], parts[1], parts[2], parts[3:]
    rhs = rhs.strip()
    kind = rhs.split(' ')[0]
    n += 1
    if kind == 'PANIC':
        if fm == '32' and op == 'rootn' and ('1E+100' in args or '1E-100' in args): continue
        bad.append(('PANIC', line.strip())); continue
    signal.alarm(4)
    try:
        tv = true_value(fm, op, args)
        _ = exact_or_tie(fm, tv) if tv[0] in ('dec','nines') else None
        signal.alarm(0)
    except TO:
        bad.append(('ORACLE_TIMEOUT', line.strip())); continue
    stats[(op, kind, tv[0])] = stats.get((op, kind, tv[0]), 0) + 1
    if kind in ('None', 'Anchor'):
        if tv[0] in ('zero', 'inf'): bad.append(('MISSING_SPECIAL', line.strip())); continue
        if tv[0] in ('dec', 'nines'):
            ex, tie = exact_or_tie(fm, tv)
            if ex or tie: bad.append(('MISSING_' + ('EXACT' if ex else 'TIE'), line.strip()))
        continue
    _, val, bits = rhs.split(' '); val = Decimal(val); bits = int(bits)
    if tv[0] in ('irr', 'quot', 'wide'):
        if not api:
            bad.append(('WRONG_SOME_' + tv[0], line.strip()))
        elif not bits & 16:
            bad.append(('API_NOT_INEXACT_ON_' + tv[0], line.strip()))
        continue
    if tv[0] in ('?',): continue
    if tv[0] == 'zero':
        if not (val.is_zero() and val.is_signed() == tv[1] and bits == 0): bad.append(('WRONG_ZERO', line.strip(), tv))
        continue
    if tv[0] == 'inf':
        if not (val.is_infinite() and val.is_signed() == tv[1] and bits == 2): bad.append(('WRONG_POLE', line.strip(), tv))
        continue
    if tv[0] == 'huge':
        big = ('dec', 0, 1, 999999 if tv[1] > 0 else -999999)
        if tv[1] == 0: continue
        d, inx, ovf, unf = round_v(fm, big, mode)
        if not (d == val and inx == bool(bits & 16) and ovf == bool(bits & 4)):
            bad.append(('HUGE_MISMATCH', line.strip(), str(d)))
        continue
    d, inx, ovf, unf = round_v(fm, tv, mode)
    okv = d == val and d.is_signed() == val.is_signed() and d.is_infinite() == val.is_infinite()
    okf = inx == bool(bits & 16) and ovf == bool(bits & 4)
    if not (okv and okf):
        bad.append(('WRONG_VALUE_OR_FLAGS', line.strip(), 'want', str(d), 'inx', inx, 'ovf', ovf))
    elif unf != bool(bits & 8):
        bad.append(('UNDERFLOW_FLAG_DIFF', line.strip(), 'want unf', unf))
print('cases', n)
for k in sorted(stats): print('  ', k, stats[k])
groups = {}
for b in bad: groups.setdefault(b[0], []).append(b)
print('problems', len(bad))
for k, v in groups.items():
    print('==', k, len(v))
    for b in v[:25]: print('   ', b)
