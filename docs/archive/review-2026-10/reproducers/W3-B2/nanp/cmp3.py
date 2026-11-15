import re,sys
def load(f):
    return [l.rstrip('\n') for l in open(f)]
A,B,C = load('out128.txt'), load('out64.txt'), load('out32.txt')
def parse(l):
    left, right = l.split(' -> ')
    val, st = right.rsplit(' ',1)
    return left, val, int(st)
special = re.compile(r'^(-?s?NaN\d*|-?Infinity|-?0(E[+-]?\d+)?|-?0\.0*|1|-1|Some\(\w+\)|None|true|false|Less|Equal|Greater|-?0E[-+]\d+)$')
def cls(v, st):
    # normalise: special/exact values compared verbatim; other finite values compared as 'FIN'
    if special.match(v) or v.endswith('NaN') or 'NaN' in v or 'Inf' in v:
        return v
    return 'FIN'
from collections import defaultdict
diffs = defaultdict(list)
for a,b,c in zip(A,B,C):
    la,va,sa = parse(a); lb,vb,sb = parse(b); lc,vc,sc = parse(c)
    assert la==lb==lc
    ka,kb,kc = (cls(va,sa),sa&~0x30), (cls(vb,sb),sb&~0x30), (cls(vc,sc),sc&~0x30)
    # mask INEXACT(16) and CLAMPED(32) for FIN results
    if ka!=kb or ka!=kc:
        op = la.split()[1]
        diffs[op].append(f'{la[2:]} | d128 {va}#{sa} | d64 {vb}#{sb} | d32 {vc}#{sc}')
for op in diffs:
    print('==',op,len(diffs[op]))
    for l in diffs[op][:int(sys.argv[1]) if len(sys.argv)>1 else 12]: print('  ',l)
