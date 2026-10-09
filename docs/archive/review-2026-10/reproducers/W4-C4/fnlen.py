import re,sys,os
root="/Users/parnell/Development/ferrodec"
dirs=["src","ferrodec-decimal64/src","ferrodec-decimal32/src","ferrodec-decimal/src","ferrodec-multiword/src","ferrodec-ieee/src"]
excl=["src/math/","src/verify/"]
fnre=re.compile(r'^\s*(pub(\([^)]*\))?\s+)?(const\s+)?(unsafe\s+)?fn\s+(\w+)')
out=[]
for d in dirs:
  for dp,_,fs in os.walk(os.path.join(root,d)):
    for f in fs:
      if not f.endswith(".rs"): continue
      p=os.path.join(dp,f); rel=os.path.relpath(p,root)
      if d=="src" and any(rel.startswith(e) for e in excl): continue
      if "/verify/" in rel: continue
      lines=open(p).read().split("\n")
      # find test module start
      test_start=None
      for i,l in enumerate(lines):
        if re.match(r'\s*#\[cfg\(test\)\]',l) and i+1<len(lines) and re.match(r'\s*(pub\s+)?mod\s+\w+',lines[i+1]):
          test_start=i; break
      i=0
      while i<len(lines):
        m=fnre.match(lines[i])
        if m:
          name=m.group(5); start=i
          depth=0; seen=False; j=i
          while j<len(lines):
            s=re.sub(r'//.*','',lines[j])
            s=re.sub(r'"(\\.|[^"\\])*"','""',s)
            s=re.sub(r"'(\\.|[^'\\])'","''",s)
            for ch in s:
              if ch=='{': depth+=1; seen=True
              elif ch=='}': depth-=1
            if seen and depth<=0: break
            if not seen and s.rstrip().endswith(";"): break
            j+=1
          n=j-start+1
          intest = test_start is not None and start>test_start
          if n>=120 and not intest:
            # count non blank non comment lines
            code=sum(1 for l in lines[start:j+1] if l.strip() and not l.strip().startswith("//"))
            out.append((n,code,rel,start+1,name))
          i=start+1
        else: i+=1
for n,c,rel,ln,name in sorted(out,reverse=True):
  print(f"{n}\t{c}\t{rel}:{ln}\t{name}")
