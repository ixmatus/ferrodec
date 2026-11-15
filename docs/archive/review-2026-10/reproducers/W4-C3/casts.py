import re,glob,os
base='/Users/parnell/Development/ferrodec/ferrodec-transcend/src/'
out=[]
pat=re.compile(r'\bas (u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f64)\b')
for p in sorted(glob.glob(base+'*.rs')):
    f=os.path.basename(p)
    L=open(p).read().split('\n')
    stop=len(L)
    for i,l in enumerate(L):
        if re.match(r'^mod tests',l) or (f=='mock_format.rs'): stop=i if f!='mock_format.rs' else 0;break
    for i in range(stop):
        code=re.sub(r'//.*','',L[i])
        for m in pat.finditer(code):
            out.append((f,i+1,L[i].strip()))
print(len(out))
from collections import Counter
print(Counter(o[0] for o in out).most_common())
samp=[o for i,o in enumerate(out) if i%16==0 and (i-2)%5!=0][:12]
for s in samp: print(f'{s[0]}:{s[1]}: {s[2][:140]}')
