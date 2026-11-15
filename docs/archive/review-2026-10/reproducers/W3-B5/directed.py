H = hex
lines = []
for w, maxd in [(256,78),(384,116),(512,155),(768,232),(1024,309)]:
    M = 1 << w
    for k in range(0, maxd+1):
        for v in (10**k - 1, 10**k, 10**k + 1):
            if 0 <= v < M:
                lines.append(f"{w}dig {H(v)}"); lines.append(f"{w}dr10 {H(v)}")
    lines.append(f"{w}dig {H(M-1)}")
    # carries across every limb boundary
    for i in range(w//128):
        a = (1 << (128*(i+1))) - 1
        if a < M:
            lines.append(f"{w}add {H(a)} {H(1)}"); lines.append(f"{w}sub {H(a+1 if a+1 < M else a)} {H(1)}")
    lines.append(f"{w}add {H(M-1)} {H(1)}")  # wraps to 0
    lines.append(f"{w}add {H(M-1)} {H(M-1)}")
    lines.append(f"{w}sub {H(0)} {H(1)}")    # precondition violated: documents wrap
    lines.append(f"{w}cmp {H(M-1)} {H(M-2)}")
for p in [(1<<512, 1<<512), ((1<<512)-1, (1<<512)+1), ((1<<1024)-1, 1), (1, (1<<1024)-1), (1<<1023, 2), (1<<1023, 1), ((1<<128), (1<<(1024-128))), ((1<<128)-1, (1<<(1024-128))), (3, ((1<<1024)-1)//3), (3, ((1<<1024)-1)//3 + 1)]:
    lines.append(f"1024cm {H(p[0])} {H(p[1])}")
for m in [(1<<128)-1, 10**38, 2]:
    lines.append(f"1024x {H((1<<768)-1)} {H(m)}")
    lines.append(f"768x {H((1<<384)-1)} {H((1<<384)-1)}")
# isqrt at the documented edge (bit_len 254) and powers
for v in [(1<<254)-1, 1<<253, 10**70-1, 10**76, (1<<252)]:
    lines.append(f"256isqrt {H(v)}")
for k in range(0, 77):
    lines.append(f"256mulp {H(1)} {k}")
    lines.append(f"256mulp {H((1<<256)//10**k - 1 if k else (1<<256)-1)} {k}")
for k in range(0, 116):
    lines.append(f"384mulp {H(1)} {k}")
lines.append(f"256divu {H((1<<256)-1)} {H((1<<128)-1)}")
lines.append(f"256divu {H((1<<256)-1)} {H(1)}")
lines.append(f"384divu {H((1<<384)-1)} {H((1<<128)-1)}")
lines.append(f"384divu {H((1<<384)-1)} {H(1)}")
lines.append(f"384divu {H((1<<384)-1)} {H(1<<127)}")
print("\n".join(lines))
