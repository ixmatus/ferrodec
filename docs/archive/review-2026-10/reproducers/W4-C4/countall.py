import json,sys,collections
S=sys.argv[1]
crates=["ferrodec","ferrodec-decimal64","ferrodec-decimal32","ferrodec-decimal","ferrodec-multiword","ferrodec-ieee"]
dirs={"ferrodec":"","ferrodec-decimal64":"ferrodec-decimal64/","ferrodec-decimal32":"ferrodec-decimal32/","ferrodec-decimal":"ferrodec-decimal/","ferrodec-multiword":"ferrodec-multiword/","ferrodec-ieee":"ferrodec-ieee/"}
res={};allc=set()
for c in crates:
    seen=set();cnt=collections.Counter()
    for line in open(f"{S}/clippyall-{c}.json"):
        try:m=json.loads(line)
        except:continue
        if m.get("reason")!="compiler-message":continue
        msg=m["message"];code=(msg.get("code") or {}).get("code")
        if not code or not code.startswith("clippy::"):continue
        sp=[s for s in msg["spans"] if s["is_primary"]]
        if not sp: continue
        fn=sp[0]["file_name"]
        # attribute to the crate whose directory holds the file
        if c=="ferrodec":
            if any(fn.startswith(d) for d in dirs.values() if d): continue
        elif not fn.startswith(dirs[c]): continue
        key=(code,fn,sp[0]["line_start"],sp[0]["column_start"])
        if key in seen:continue
        seen.add(key);cnt[code]+=1;allc.add(code)
    res[c]=cnt
print("lint|"+"|".join(crates)+"|total")
for code in sorted(allc,key=lambda k:-sum(res[c][k] for c in crates)):
    print(code+"|"+"|".join(str(res[c][code]) for c in crates)+"|"+str(sum(res[c][code] for c in crates)))
