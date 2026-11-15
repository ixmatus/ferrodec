import sys, re
sys.path.insert(0,'.')
exec(open('hunks.py').read().split('res={}')[0])
path=sys.argv[1]; starts=set(int(x) for x in sys.argv[2].split(','))
for h in hunks(path):
    if h['start'] in starts:
        print('@@', h['start'])
        for l in h['lines']:
            if l[:1] in '+-' and not is_comment(l): print('  ', l[:160])
