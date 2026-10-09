keys = ["hypot Infinity sNaN3","hypot Infinity NaN12","pow 1 NaN12","pow 1 sNaN3","pow NaN12 0","pow -1 Infinity","pow 0 -Infinity","pow -0 -1","powr 0 0","powr Infinity 0","powr 1 Infinity","powr 0 -Infinity","powr -1 NaN12","atan2 0 -0","atan2 sNaN3 NaN12","atan2_pi sNaN3 NaN12","compound NaN12 0","compound sNaN3 0","compound -2.5 2","rootn -0 -2","rootn NaN12 0","rootn 2 0","powi NaN12 0","powi sNaN3 0","powi 0 -1","quantize NaN12 sNaN3","rem_near NaN12 sNaN3","divide_integer Infinity Infinity","next_toward NaN12 sNaN3","compare_signaling NaN12 1","partial_cmp sNaN3 1","fma NaN12 sNaN3,1","fma 0 Infinity,NaN12"]
def load(f):
    d={}
    for l in open(f):
        k,v=l.rstrip('\n')[2:].split(' -> '); d.setdefault(k,v)
    return d
A,B,C=load('out128.txt'),load('out64.txt'),load('out32.txt')
for k in keys: print(f'{k} | {A.get(k)} | {B.get(k)} | {C.get(k)}')
