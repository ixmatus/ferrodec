import json,sys,collections,os
S=sys.argv[1]
crates=["ferrodec","ferrodec-decimal64","ferrodec-decimal32","ferrodec-decimal","ferrodec-multiword","ferrodec-ieee"]
res={}
allcodes=set()
for c in crates:
    seen=set(); cnt=collections.Counter()
    for line in open(f"{S}/clippy-{c}.json"):
        try: m=json.loads(line)
        except: continue
        if m.get("reason")!="compiler-message": continue
        tgt=m["target"]["name"].replace("_","-")
        if tgt!=c: continue
        msg=m["message"]; code=(msg.get("code") or {}).get("code")
        if not code or not code.startswith("clippy::"): continue
        sp=[s for s in msg["spans"] if s["is_primary"]]
        key=(code, sp[0]["file_name"], sp[0]["line_start"], sp[0]["column_start"]) if sp else (code,)
        if key in seen: continue
        seen.add(key); cnt[code]+=1; allcodes.add(code)
    res[c]=cnt
print("lint|"+"|".join(crates)+"|total")
for code in sorted(allcodes, key=lambda k:-sum(res[c][k] for c in crates)):
    print(code+"|"+"|".join(str(res[c][code]) for c in crates)+"|"+str(sum(res[c][code] for c in crates)))
