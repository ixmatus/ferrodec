import json,subprocess
o=json.load(open('fm.json'))
kw={'payne-hanek-1983':r'Payne','lauter-lefevre-pow-boundary':r'Lauter','knuth-taocp-v2':r'Knuth','muller-elementary-functions':r'Muller, \*Elementary|Elementary Functions','ziv-1991':r'\bZiv\b','decnumber':r'decNumber','mpdecimal':r'libmpdec|mpdecimal','paterson-stockmeyer-1973':r'Paterson','haible-papanikolaou-1998':r'Haible','c23-draft':r'C23','cowlishaw-dectest':r'decTest','astro-float':r'astro.float','mpmath':r'mpmath','arb-flint':r'python-flint|\bArb\b'}
for slug,k in kw.items():
    r=subprocess.run(['bash','-c','cd /Users/parnell/Development/ferrodec && grep -rlE "'+k.replace('"','')+'" src ferrodec-*/src tools tests/*.rs ferrodec-*/tests/*.rs ferrodec-*/tests/common 2>/dev/null'],capture_output=True,text=True)
    files=set(r.stdout.split())
    listed=set(o[slug]['ls']['consumers'])|set(o[slug]['ls']['verification'])
    miss=sorted(files-listed)
    print(f"{slug}: cites={len(files)} listed_of_those={len(files&listed)} unlisted={len(miss)} e.g. {miss[:4]}")
