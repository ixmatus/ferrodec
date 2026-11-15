import json,os,re,subprocess
root='/Users/parnell/Development/ferrodec'
o=json.load(open('fm.json'))
tok={'arb-flint':'arb|flint','astro-float':'astro','brent-zimmermann-mca':'brent|zimmermann|modern computer arith|mca','brisebarre-muller-2007':'brisebarre','c23-draft':'c23|annex f|n3220|n3096|n3054','core-math':'core-math|core_math|coremath','cowlishaw-algorism-2003':'cowlishaw|algorism','cowlishaw-dectest':'dectest','cowlishaw-gda-arith':'gda|general decimal|decarith|speleotrove','crlibm':'crlibm','decimal-thread':'rexx|decifaq|lineage|gda','decnumber':'decnumber','glossary':'glossary','goldberg-1991':'goldberg','haible-papanikolaou-1998':'haible|binary split|papanikolaou','ieee-754-2008':'754-2008|754:2008|2008','ieee-754-2019':'754-2019|754:2019|2019','ieee-854-1987':'854','iordache-matula-1999':'iordache|matula','karatsuba-1962':'karatsuba','knuth-taocp-v2':'knuth|taocp|algorithm d','lang-muller-2001':'lang','lauter-lefevre-pow-boundary':'lauter','lefevre-2000':'lef[eè]vre|lefevre','lefevre-stehle-zimmermann-d64-exp':'lsz|stehl|lef[eè]vre','matveev-2000':'matveev','mpdecimal':'mpdec|libmpdec|cpython decimal|decimal module','mpfr':'mpfr','mpmath':'mpmath','muller-elementary-functions':'muller|elementary functions','muller-handbook-2018':'handbook|muller','niven-irrational-numbers':'niven','num-bigint':'num-bigint|num_bigint|biguint','paterson-stockmeyer-1973':'paterson|stockmeyer','payne-hanek-1983':'payne|hanek','registry-ieee-classes':'ieeeclass|ieee_class','registry-rounding-modes':'roundingmode','registry-status-flags':'status','rlibm':'rlibm','salikhov-2008':'salikhov','salikhov-zudilin-pi-irrationality':'salikhov|zudilin|irrationality','shidlovskii-e-functions':'shidlovsk','shidlovskii-transcendence':'shidlovsk','slz-worst-cases':'slz|stehl|lattice','verification-map':'verification-map|verification map','ziv-1991':'ziv','zeilberger-zudilin-2020':'zeilberger|zudilin','failure-anchor-band-cancellation':'anchor','failure-comparator-masking':'comparator|nan','failure-d64-quantize-pow10':'quantize','failure-directed-rounding-seam':'residual|seam','failure-min-positive-normal':'min_positive_normal','failure-self-referential-metamorphic':'metamorphic|identit'}
for k,v in o.items():
    t=tok.get(k,k)
    for key in ('consumers','verification'):
        for p in v['ls'].get(key,[]):
            fp=os.path.join(root,p)
            if os.path.isdir(fp):
                r=subprocess.run(['grep','-rliE',t,fp],capture_output=True,text=True); hit=bool(r.stdout.strip())
            else:
                try: txt=open(fp,errors='replace').read()
                except Exception as e: print('ERR',k,p,e); continue
                hit=bool(re.search(t,txt,re.I)) or (k in txt)
            if not hit: print('MISS',k,key,p)
