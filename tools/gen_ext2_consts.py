# Generate the nine Extended2 (rung 2) constants at 115 significant
# digits, mirroring tools/gen_argred.py's mpmath usage for the 55-digit
# rung 1 set. Prints the 55-digit rendering first as a self-check
# against the committed *_EXT_STR literals in
# ferrodec-transcend/src/consts.rs.
from mpmath import mp, mpf, pi, e, log, tan, nstr

NAMES = [
    ("PI", lambda: pi + 0),
    ("E", lambda: e + 0),
    ("LN2", lambda: log(2)),
    ("LN10", lambda: log(10)),
    ("INV_LN10", lambda: 1 / log(10)),
    ("INV_LN2", lambda: 1 / log(2)),
    ("PI_OVER_TWO", lambda: pi / 2),
    ("PI_OVER_FOUR", lambda: pi / 4),
    ("TAN_PI_OVER_EIGHT", lambda: tan(pi / 8)),
]


def render(digits: int) -> None:
    # Work with generous guard precision, render at `digits`.
    mp.dps = digits + 30
    print(f"# --- {digits} significant digits ---")
    for name, f in NAMES:
        v = f()
        s = nstr(v, digits, strip_zeros=False)
        print(f"{name} = \"{s}\"")


render(55)
render(115)
