# W2-A4 (reader A4): the dynamic rung's cancellation depth premise

`argred_probe_modules.diff` is the reader's three probe test modules as
a diff against `ferrodec-transcend/src/argred.rs` (apply to a copy of
the crate's `src/`; `tc_Cargo.toml` is the copy crate's manifest with
relative path dependencies). Run:

    cargo test --offline --features trig,unbounded-ladder --lib a4_sincos_dump -- --nocapture | grep '^SC' > scdump.txt
    /usr/bin/python3 check_sc.py scdump.txt      # mpmath at 8,300 dps

`deep_inputs.py` and `base.py` produce the deep cancellation inputs
(the P1B-1 continued fraction search per exponent).

Result (A4-1): at the Ziv entry width p = 220 the working `sin` error on
`±9308532438209917461067659354862169E4639` is 15,130 ulp against
`sin_budget_dyn(220)` = 14,000 (`sc_result.txt`, reader;
`sc_result_coordinator_rerun.txt`, identical). The constant 33 in
`frac_digits_dyn` is the same false premise as P1B-1.
