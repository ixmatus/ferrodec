import os,sys,re,json
root='/Users/parnell/Development/ferrodec'
d=root+'/docs/references'
out={}
for f in sorted(os.listdir(d)):
    if not f.endswith('.md') or f in('SCHEMA.md','INDEX.md'): continue
    t=open(os.path.join(d,f)).read()
    fm,body=t[4:].split('\n---\n',1)
    sc={};ls={};cur=None
    for line in fm.splitlines():
        if line.startswith('  - '):
            ls[cur].append(line[4:].strip('"'));continue
        k,v=line.split(':',1);k=k.strip();v=v.strip()
        if not v: ls[k]=[];cur=k
        else: sc[k]=v.strip('"') if v.startswith('"') and v.endswith('"') else v
    out[f[:-3]]=dict(sc=sc,ls=ls,body=body)
json.dump(out,open(sys.argv[1],'w'),indent=1)
