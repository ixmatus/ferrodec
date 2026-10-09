import subprocess, sys, time
probe = sys.argv[1]
for line in open(sys.argv[2]):
    line = line.rstrip("\n")
    if not line: continue
    t = time.time(); o = subprocess.run([probe], input=(line+"\n").encode(), capture_output=True).stdout.decode().strip(); dt = time.time() - t
    print("%7.3fs  %s  -> %s" % (dt, line[:90], o[:80]))
