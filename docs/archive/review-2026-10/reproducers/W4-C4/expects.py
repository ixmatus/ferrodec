import re,os,collections,sys
root="/Users/parnell/Development/ferrodec"
crates={"root":"src","d64":"ferrodec-decimal64/src","d32":"ferrodec-decimal32/src","decimal":"ferrodec-decimal/src","multiword":"ferrodec-multiword/src"}
for name,d in crates.items():
  rows=[]
  for dp,_,fs in os.walk(os.path.join(root,d)):
    for f in fs:
      if not f.endswith(".rs"): continue
      p=os.path.join(dp,f); rel=os.path.relpath(p,root)
      txt=open(p).read().split("\n")
      for i,l in enumerate(txt):
        if l.startswith("#[cfg(test)]"): break
        # join following lines for multi-line expect
        if ".expect(" in l or ".unwrap()" in l:
          seg=" ".join(x.strip() for x in txt[i:i+3])
          m=re.search(r'\.expect\(\s*"((?:[^"\\]|\\.)*)"',seg)
          kind="expect" if ".expect(" in l else "unwrap"
          if l.strip().startswith("//"): continue
          rows.append((rel,i+1,kind,m.group(1) if m else l.strip()[:80]))
  print(f"== {name}: {len(rows)} (expect {sum(r[2]=='expect' for r in rows)}, unwrap {sum(r[2]=='unwrap' for r in rows)})")
  for r in rows: print(f"  {r[0]}:{r[1]} [{r[2]}] {r[3]}")
