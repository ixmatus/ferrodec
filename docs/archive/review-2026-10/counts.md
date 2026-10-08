## Crate declarations

| crate | src_files | src_lines | test_files | test_lines | #[test] | #[kani::proof] | proptest! | #[ignore] | as casts (src) | unwrap (prod) | expect (prod) | panic! (prod) | unreachable! (prod) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ferrodec | 71 | 20678 | 77 | 22596 | 1140 | 87 | 45 | 2 | 372 | 1 | 14 | 3 | 11 |
| ferrodec-decimal64 | 61 | 18622 | 52 | 15846 | 891 | 104 | 17 | 0 | 311 | 13 | 49 | 5 | 30 |
| ferrodec-decimal32 | 61 | 19666 | 53 | 15991 | 930 | 105 | 22 | 2 | 329 | 21 | 48 | 3 | 30 |
| ferrodec-transcend | 27 | 23036 | 0 | 0 | 172 | 0 | 1 | 0 | 241 | 0 | 28 | 9 | 3 |
| ferrodec-decimal | 29 | 7037 | 10 | 2807 | 168 | 0 | 2 | 0 | 94 | 0 | 60 | 0 | 1 |
| ferrodec-multiword | 8 | 4218 | 1 | 123 | 127 | 0 | 1 | 0 | 110 | 0 | 2 | 0 | 0 |
| ferrodec-ieee | 6 | 920 | 0 | 0 | 20 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| ferrodec-test-support | 9 | 4068 | 3 | 820 | 36 | 0 | 0 | 0 | 8 | 8 | 35 | 15 | 0 |
| ferrodec-campaign | 6 | 1434 | 1 | 91 | 14 | 0 | 0 | 0 | 4 | 12 | 5 | 0 | 2 |

## Corpus rows per vector directory (non-comment, non-blank lines)

| directory | files | rows |
|---|---|---|
| ferrodec-campaign/tests/vectors | 1 | 66 |
| ferrodec-decimal/tests/vectors | 50 | 29854 |
| ferrodec-decimal32/tests/vectors | 2 | 1214 |
| ferrodec-decimal64/tests/vectors | 42 | 14854 |
| tests/vectors | 42 | 15214 |
| tests/vectors/gda-transcend | 4 | 2920 |
| tests/vectors/round_half_even | 1 | 28 |
| tests/vectors/transcend | 97 | 38419 |
| tests/vectors/transcend/anchor_bands | 17 | 1885 |
| tests/vectors/transcend/campaign | 18 | 6000 |
| tests/vectors/transcend/campaign/s1 | 3 | 1819 |
| tests/vectors/transcend/exhaustive | 19 | 19 |
| tests/vectors/transcend/external | 1 | 170 |
| tests/vectors/transcend/planted | 72 | 2160 |

## Registry

entries: 53; INDEX lines: 53

## ADRs

files: 61; highest: 0061-pi-scaled-family-design.md
