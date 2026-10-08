#!/usr/bin/env python3
"""Phase 1 counts table for the 2026-10 deep review (read only).

Every number the review's documents are checked against is produced
here once, from the tree, so the coherence reader (D2) and the report
never count twice. Run from the workspace root:

    python3 -I docs/archive/review-2026-10/counts.py > counts.md

Test totals as cargo reports them (passed / ignored per suite) come
from the battery log, not from here; this script counts what the
tree declares.
"""
import os, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CRATES = {
    "ferrodec": ROOT,
    "ferrodec-decimal64": ROOT / "ferrodec-decimal64",
    "ferrodec-decimal32": ROOT / "ferrodec-decimal32",
    "ferrodec-transcend": ROOT / "ferrodec-transcend",
    "ferrodec-decimal": ROOT / "ferrodec-decimal",
    "ferrodec-multiword": ROOT / "ferrodec-multiword",
    "ferrodec-ieee": ROOT / "ferrodec-ieee",
    "ferrodec-test-support": ROOT / "ferrodec-test-support",
    "ferrodec-campaign": ROOT / "ferrodec-campaign",
}
CAST = re.compile(r"\bas (?:u|i)(?:8|16|32|64|128|size)\b")
PANICS = {
    "unwrap": re.compile(r"\.unwrap\(\)"),
    "expect": re.compile(r"\.expect\("),
    "panic!": re.compile(r"\bpanic!\("),
    "unreachable!": re.compile(r"\bunreachable!\("),
}

def rs_files(d):
    return sorted(p for p in d.rglob("*.rs")) if d.exists() else []

def production_text(path):
    """Source before the first column-0 `#[cfg(test)]` (the inline test
    module convention in this workspace), with comment lines dropped."""
    out = []
    for line in path.read_text(errors="replace").split("\n"):
        if line.startswith("#[cfg(test)]"):
            break
        if line.lstrip().startswith("//"):
            continue
        out.append(line)
    return "\n".join(out)

def count(pattern, text):
    return len(pattern.findall(text))

rows = []
for name, d in CRATES.items():
    src = [p for p in rs_files(d / "src")]
    tests = [p for p in rs_files(d / "tests")]
    src_lines = sum(len(p.read_text(errors="replace").split("\n")) for p in src)
    test_lines = sum(len(p.read_text(errors="replace").split("\n")) for p in tests)
    all_text = "\n".join(p.read_text(errors="replace") for p in src + tests)
    n_test = len(re.findall(r"#\[test\]", all_text))
    n_kani = len(re.findall(r"#\[kani::proof\]", all_text))
    n_prop = len(re.findall(r"\bproptest!\s*\{", all_text))
    n_ignore = len(re.findall(r"#\[ignore", all_text))
    prod = "\n".join(production_text(p) for p in src if "verify" not in p.parts and p.name != "mock_format.rs")
    rows.append({
        "crate": name, "src_files": len(src), "src_lines": src_lines,
        "test_files": len(tests), "test_lines": test_lines,
        "#[test]": n_test, "#[kani::proof]": n_kani, "proptest!": n_prop, "#[ignore]": n_ignore,
        "as casts (src)": count(CAST, "\n".join(p.read_text(errors="replace") for p in src)),
        **{f"{k} (prod)": count(v, prod) for k, v in PANICS.items()},
    })

print("## Crate declarations\n")
keys = list(rows[0].keys())
print("| " + " | ".join(keys) + " |")
print("|" + "---|" * len(keys))
for r in rows:
    print("| " + " | ".join(str(r[k]) for k in keys) + " |")

print("\n## Corpus rows per vector directory (non-comment, non-blank lines)\n")
print("| directory | files | rows |\n|---|---|---|")
for vd in sorted(set(p.parent for p in ROOT.rglob("tests/vectors/**/*") if p.is_file() and p.suffix in {".txt", ".decTest", ".tsv", ".prov"} and ".claude" not in p.parts and "target" not in p.parts)):
    files = [p for p in vd.iterdir() if p.is_file() and p.suffix in {".txt", ".decTest", ".tsv", ".prov"}]
    n = 0
    for p in files:
        for line in p.read_text(errors="replace").split("\n"):
            s = line.strip()
            if s and not s.startswith(("#", "--")):
                n += 1
    print(f"| {vd.relative_to(ROOT)} | {len(files)} | {n} |")

reg = [p for p in (ROOT / "docs/references").glob("*.md") if p.name not in ("INDEX.md", "SCHEMA.md")]
idx = [l for l in (ROOT / "docs/references/INDEX.md").read_text().split("\n") if l.startswith("- [")]
print(f"\n## Registry\n\nentries: {len(reg)}; INDEX lines: {len(idx)}")
adrs = sorted((ROOT / "docs/decisions").glob("[0-9][0-9][0-9][0-9]-*.md"))
print(f"\n## ADRs\n\nfiles: {len(adrs)}; highest: {adrs[-1].name}")
