import re,sys,difflib
base='/Users/parnell/Development/ferrodec/ferrodec-transcend/src/'
files=['extended.rs','extended2.rs','extended_dyn.rs']
def funcs(path):
    lines=open(base+path).read().split('\n')
    out={}
    # stop at "impl ExtNum for" to collect inherent + free fns, and stop at mod tests
    region='inh'
    i=0
    while i<len(lines):
        l=lines[i]
        if re.match(r'^impl ExtNum for',l): region='ext'
        if re.match(r'^mod tests',l) or re.match(r'^#\[cfg\(test\)\]',l): break
        m=re.match(r'^\s*(pub(\([a-z]+\))? )?(const )?fn (\w+)',l)
        if m and region=='inh':
            name=m.group(4)
            # find body by brace counting
            depth=0;start=i;j=i;seen=False
            while j<len(lines):
                depth+=lines[j].count('{')-lines[j].count('}')
                if '{' in lines[j]: seen=True
                if seen and depth==0: break
                if not seen and lines[j].rstrip().endswith(';'): break
                j+=1
            if seen: out.setdefault(name,(start+1,j-start+1,'\n'.join(lines[start:j+1])))
            i=j+1; continue
        i+=1
    return out
def norm(s):
    s=re.sub(r'ExtendedDyn(<\'_>)?|Extended2','Extended',s)
    s=re.sub(r'\bU(256|384|768)\b|DecBig','UW',s)
    s=re.sub(r'//.*','',s)
    s=re.sub(r'\bpub(\(crate\))? ','',s)
    s=re.sub(r'\s+',' ',s)
    return s
F=[funcs(f) for f in files]
names=sorted(set(F[0])|set(F[1])|set(F[2]), key=lambda n: F[0].get(n,F[1].get(n,F[2].get(n)))[0])
print('| fn | ext L(line) | ext2 | dyn | sim e/e2 | sim e2/dyn |')
for n in names:
    c=[f.get(n) for f in F]
    def cell(x): return f'{x[1]} (:{x[0]})' if x else '-'
    def sim(a,b):
        if not a or not b: return '-'
        return f'{difflib.SequenceMatcher(None,norm(a[2]),norm(b[2])).ratio():.2f}'
    print(f'| {n} | {cell(c[0])} | {cell(c[1])} | {cell(c[2])} | {sim(c[0],c[1])} | {sim(c[1],c[2])} |')
