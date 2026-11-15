import re,glob,os
base='/Users/parnell/Development/ferrodec/ferrodec-transcend/src/'
rows=[]
for p in sorted(glob.glob(base+'*.rs')):
    L=open(p).read().split('\n')
    stop=len(L)
    for i,l in enumerate(L):
        if re.match(r'^mod tests',l): stop=i;break
    i=0
    while i<stop:
        m=re.match(r'^\s*(pub(\([a-z]+\))? )?(const )?fn (\w+)',L[i])
        if m:
            depth=0;seen=False;j=i
            while j<len(L):
                s=re.sub(r'//.*','',L[j]); s=re.sub(r"'.'","",s); s=re.sub(r'"([^"\\]|\\.)*"','',s)
                depth+=s.count('{')-s.count('}')
                if '{' in s: seen=True
                if seen and depth<=0: break
                if not seen and s.rstrip().endswith(';'): break
                j+=1
            n=j-i+1
            if seen and n>80:
                body=L[i:j+1]
                code=sum(1 for b in body if b.strip() and not b.strip().startswith('//'))
                rows.append((n,code,os.path.basename(p),i+1,m.group(4)))
            i=j+1 if seen else i+1
            continue
        i+=1
for r in sorted(rows,reverse=True): print(r)
print(len(rows))
