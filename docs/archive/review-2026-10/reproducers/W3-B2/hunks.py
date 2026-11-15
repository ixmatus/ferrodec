import re, sys, os, glob
def norm(s):
    s = re.sub(r'\b[ui](8|16|32|64|128)\b', 'INT', s)
    s = re.sub(r'\b(U128|U64|U32|u128|u64|u32)\b', 'INT', s)
    s = re.sub(r'POW10_\w+', 'POW10', s)
    s = re.sub(r'\b0x[0-9A-Fa-f_]+\b', 'N', s)
    s = re.sub(r'\b\d[\d_]*(\.\d+)?(e-?\d+)?\b', 'N', s)
    s = re.sub(r'_into_INT', '_into', s)
    s = re.sub(r'\s+', '', s)
    return s
def is_comment(l):
    t = l[1:].strip()
    return t.startswith('//') or t == ''
def hunks(path):
    cur=None; out=[]; lnum=0
    for line in open(path, errors='replace'):
        if line.startswith('@@'):
            m = re.match(r'@@ -(\d+)', line); cur={'start':int(m.group(1)),'lines':[]}; out.append(cur)
        elif cur is not None and line[:1] in '+- ' and not line.startswith('---') and not line.startswith('+++'):
            cur['lines'].append(line.rstrip('\n'))
    return out
def test_start(path):
    try:
        for i,l in enumerate(open(path),1):
            if l.strip()=='#[cfg(test)]': return i
    except: pass
    return 10**9
res={}
for d in sys.argv[2:]:
    for f in sorted(glob.glob(os.path.join(sys.argv[1], d, '*.diff'))):
        if 'verify' in f or 'convert' in f: continue
        hs = hunks(f)
        if not hs: continue
        # left source file
        base=os.path.basename(f)[:-5]
        left = base.replace('ferrodec-decimal64_src_','ferrodec-decimal64/src/').replace('ferrodec-decimal32_src_','ferrodec-decimal32/src/')
        if left.startswith('src_'): left = 'src/'+left[4:]
        left = left.replace('_ops_','/ops/') if '/ops/' not in left else left
        left = left.replace('src/ops_','src/ops/')
        ts = test_start('/Users/parnell/Development/ferrodec/'+left)
        cnt={'doc':0,'test':0,'width':0,'residual':0}; resid=[]
        for h in hs:
            if h['start'] >= ts: cnt['test']+=1; continue
            code=[l for l in h['lines'] if l[:1] in '+-' and not is_comment(l)]
            if not code: cnt['doc']+=1; continue
            minus=[norm(l[1:]) for l in code if l[0]=='-']; plus=[norm(l[1:]) for l in code if l[0]=='+']
            if ''.join(minus)==''.join(plus): cnt['width']+=1
            else: cnt['residual']+=1; resid.append(h['start'])
        print(f"{d}\t{left}\thunks={len(hs)}\tdoc={cnt['doc']}\ttest={cnt['test']}\twidth={cnt['width']}\tresidual={cnt['residual']}\tat={resid[:30]}")
