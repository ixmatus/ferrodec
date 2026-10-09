import random, subprocess, sys
from harness import DRV
rng=random.Random(int(sys.argv[1]))
ops2=["add","sub","mul","div","remnear","remtrunc","divint","quantize","min","max","minmag","maxmag","nexttoward","totcmp","totmag","samequantum","cmp","cmpsig","rotate","shift","and","or","xor"]
ops1=["sqrt","rti","rtie","reduce","logb","nextup","nextdown","abs","neg","floor","ceil","trunc","round","roundeven","canon","invert"]
def rb():
    r=rng.random()
    if r<0.3:
        top=rng.choice([0x78,0x79,0x7A,0x7B,0x7C,0x7D,0x7E,0x7F,0x60,0x6F,0x70,0x77,0x5F,0x00,0x3F])
        return (top<<120 | rng.getrandbits(120)) | (rng.getrandbits(1)<<127)
    if r<0.5:
        return rng.getrandbits(128)
    # canonical-ish finite with random exponent
    b=rng.randrange(0,12288); c=rng.choice([0,1,10**33,10**34-1,rng.randrange(10**34)])
    s=rng.getrandbits(1)
    return (s<<127)|((b>>12)<<125)|((c>>110)<<122)|((b&0xFFF)<<110)|(c&((1<<110)-1))
lines=[]
for i in range(int(sys.argv[2])):
    m=rng.choice(["ne","na","tz","tp","tn"])
    if rng.random()<0.6:
        op=rng.choice(ops2); lines.append(f"{op} {m} B,{rb():032X} B,{rb():032X}")
    elif rng.random()<0.5:
        lines.append(f"fma {m} B,{rb():032X} B,{rb():032X} B,{rb():032X}")
    elif rng.random()<0.5:
        lines.append(f"scaleb {m} B,{rb():032X} {rng.randrange(-13000,13000)}")
    else:
        op=rng.choice(ops1); lines.append(f"{op} {m} B,{rb():032X}")
# run in chunks to localize panics
CH=2000
for k in range(0,len(lines),CH):
    chunk=lines[k:k+CH]
    p=subprocess.run([DRV],input="\n".join(chunk)+"\n",capture_output=True,text=True)
    if p.returncode!=0:
        # bisect to find line
        for l in chunk:
            q=subprocess.run([DRV],input=l+"\n",capture_output=True,text=True)
            if q.returncode!=0:
                print("PANIC", l, q.stderr.strip().splitlines()[1] if len(q.stderr.strip().splitlines())>1 else q.stderr.strip()); break
print("done", len(lines))
