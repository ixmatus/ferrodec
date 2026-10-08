#!/usr/bin/env bash
# ADR-0059 lane gate battery. Every lane the CR-d128 lane treats as a
# merge gate, in one script, so a green run means "the battery passed"
# with no lane silently skipped.
#
# `set -o pipefail` is load-bearing (the 9f30a98 lesson): a
# `cargo test | grep`-shaped pipeline exits green on test failure
# without it, and that masked a real kernel defect for a full battery
# round. Do not remove it; do not pipe cargo output through filters
# inside a lane.
#
# Usage:
#   tools/gate_battery.sh            # run every lane
#   tools/gate_battery.sh fmt tests  # run the named lanes only
#
# Lanes: fmt clippy clippy_members rustdoc tests members transcend telemetry
#        force_escalate ladder_audit force_adjudicate force_rung3 thumbv6m kani
#
# Selection parity with .github/workflows/ci.yml is a gate property
# (the 2026-07-26 to 2026-10-08 lesson: push CI on main was red for
# ten weeks while this battery was green, because each instrument
# selected feature sets and packages the other never built). When CI
# gains a selection, the matching lane lands here in the same commit,
# and vice versa. The toolchain floats on `stable` on both sides, so
# `rustup update stable` before a battery run is part of the gate;
# the preflight below prints the version into every log.
#
# The RUSTFLAGS lanes use per-lane CARGO_TARGET_DIR subdirectories so
# repeated battery runs reuse each configuration's cache instead of
# thrashing one directory through recompiles.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== preflight: $(rustc --version) / $(cargo --version)"
echo "=== preflight: $(rustup check 2>/dev/null | grep '^stable' || echo 'rustup check unavailable (offline?)')"

FAILED=()

lane() {
    local name="$1"
    shift
    echo
    echo "==================================================================="
    echo "=== lane: ${name}"
    echo "==================================================================="
    if "$@"; then
        echo "=== lane ${name}: PASS"
    else
        echo "=== lane ${name}: FAIL"
        FAILED+=("${name}")
    fi
}

lane_fmt() {
    cargo fmt --all -- --check
}

lane_clippy() {
    # Workspace-wide, then the direct-package sweeps for the crates
    # whose feature surfaces the workspace resolver can mask.
    cargo clippy --workspace --all-targets --all-features -- -D warnings &&
        cargo clippy -p ferrodec-transcend --all-features --all-targets -- -D warnings &&
        cargo clippy -p ferrodec-multiword --all-features --all-targets -- -D warnings
}

lane_clippy_members() {
    # CI's per-package clippy jobs: `cargo clippy -p X --all-features`
    # resolves X's dependencies with ONLY X's feature requests, which
    # the workspace-wide sweep above never sees (ferrodec-decimal pulls
    # ferrodec-transcend with no function cluster, which is how the
    # ladder module went dead-code red on CI for ten weeks).
    # ferrodec-ieee and ferrodec-test-support run without
    # --all-features as in CI: test-support's `mpfr-gate` needs the
    # MPFR C library.
    cargo clippy -p ferrodec --all-features --all-targets -- -D warnings &&
        cargo clippy -p ferrodec-decimal64 --all-features --all-targets -- -D warnings &&
        cargo clippy -p ferrodec-decimal32 --all-features --all-targets -- -D warnings &&
        cargo clippy -p ferrodec-decimal --all-features --all-targets -- -D warnings &&
        cargo clippy -p ferrodec-ieee --all-targets -- -D warnings &&
        cargo clippy -p ferrodec-test-support --all-targets -- -D warnings
}

lane_rustdoc() {
    # ferrodec-multiword needs --all-features (DecBig / u768 docs).
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ferrodec --features transcendentals &&
        RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ferrodec-decimal64 --features transcendentals &&
        RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ferrodec-decimal32 --features transcendentals &&
        RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ferrodec-transcend --all-features &&
        RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ferrodec-multiword --all-features
}

lane_tests() {
    # Full root and sibling suites, feature-off AND feature-on.
    cargo test -p ferrodec &&
        cargo test -p ferrodec --features transcendentals &&
        cargo test -p ferrodec-decimal64 &&
        cargo test -p ferrodec-decimal64 --features transcendentals &&
        cargo test -p ferrodec-decimal32 &&
        cargo test -p ferrodec-decimal32 --features transcendentals
}

lane_members() {
    # The suites CI runs that the `tests` lane does not: the root and
    # d64 `--all-features` rows of the CI matrix (which include the
    # Python differential and the dpd / serde / num-traits surfaces),
    # the arbitrary-precision crate's 27k-vector conformance suite, the
    # multiword and ieee substrates, and the test-support crate's own
    # suite (the ADR-0052 registry guard lives there and was red for
    # eight weeks while no lane ran it).
    cargo test -p ferrodec --all-features &&
        cargo test -p ferrodec-decimal64 --all-features &&
        cargo test -p ferrodec-decimal --all-features &&
        cargo test -p ferrodec-multiword --all-features &&
        cargo test -p ferrodec-ieee &&
        cargo test -p ferrodec-test-support
}

lane_transcend() {
    # The kernel crate's own tests (budget audit, predicate, seams),
    # with and without the unbounded rung.
    cargo test -p ferrodec-transcend --features transcendentals &&
        cargo test -p ferrodec-transcend --features transcendentals,unbounded-ladder &&
        cargo test -p ferrodec-transcend --all-features
}

lane_telemetry() {
    # S3 pinned escalation counts (fd-4zo.21): the planted corpus's
    # 20-per-file rung-2 entries, the sampled corpus's measured pins,
    # and the siblings' structural zero. Default lane only (the cfg
    # lanes bypass the natural predicate).
    local td="target/battery/telemetry"
    CARGO_TARGET_DIR="$td" \
        cargo test -p ferrodec --features transcendentals,telemetry --test ladder_telemetry &&
        CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal64 --features transcendentals,telemetry --test ladder_telemetry &&
        CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal32 --features transcendentals,telemetry --test ladder_telemetry
}

lane_force_escalate() {
    # Anti-rot byte-identity differential: every guarded delivery
    # routed through rung 2; the full pinned corpus is the reference.
    local td="target/battery/force_escalate"
    RUSTFLAGS="--cfg force_escalate" CARGO_TARGET_DIR="$td" \
        cargo test -p ferrodec --features transcendentals &&
        RUSTFLAGS="--cfg force_escalate" CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal64 --features transcendentals &&
        RUSTFLAGS="--cfg force_escalate" CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal32 --features transcendentals
}

lane_ladder_audit() {
    # Panics on top-rung residual ambiguity. ALL THREE formats: the
    # audit lane that only ran on Decimal128 masked the sinh/cosh
    # saturation defect (9f30a98) because d128's saturation region is
    # sampler-rare while d64/d32's are not.
    local td="target/battery/ladder_audit"
    RUSTFLAGS="--cfg ladder_audit" CARGO_TARGET_DIR="$td" \
        cargo test -p ferrodec --features transcendentals &&
        RUSTFLAGS="--cfg ladder_audit" CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal64 --features transcendentals &&
        RUSTFLAGS="--cfg ladder_audit" CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal32 --features transcendentals
}

lane_force_adjudicate() {
    # ADR-0060 anti-rot differential: with force_escalate routing every
    # guarded delivery to rung 2 and force_adjudicate replacing rung 2's
    # budgeted verdict with the unbudgeted nearest-boundary locate,
    # every corpus row of the five algebraic operations (rsqrt, hypot,
    # powi's powering arm, rootn, compound) delivers THROUGH the exact
    # integer adjudicator wherever its range gates accept; the full
    # pinned corpus is the byte-identity reference. Default build: the
    # lane's meaning is rung-2-as-top.
    local td="target/battery/force_adjudicate"
    RUSTFLAGS="--cfg force_escalate --cfg force_adjudicate" CARGO_TARGET_DIR="$td" \
        cargo test -p ferrodec --features transcendentals &&
        RUSTFLAGS="--cfg force_escalate --cfg force_adjudicate" CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal64 --features transcendentals &&
        RUSTFLAGS="--cfg force_escalate --cfg force_adjudicate" CARGO_TARGET_DIR="$td" \
            cargo test -p ferrodec-decimal32 --features transcendentals
}

lane_force_rung3() {
    # Release lane: both fixed rungs route to the dynamic rung; the
    # full corpus and the S1 replay are the byte-identity references.
    # Release profile because the dynamic rung at 220+ digits is slow
    # enough to matter under the full corpus.
    local td="target/battery/force_rung3"
    RUSTFLAGS="--cfg force_rung3" CARGO_TARGET_DIR="$td" \
        cargo test --release -p ferrodec --features transcendentals,unbounded-ladder &&
        RUSTFLAGS="--cfg force_rung3" CARGO_TARGET_DIR="$td" \
            cargo test --release -p ferrodec-decimal64 --features transcendentals,unbounded-ladder &&
        RUSTFLAGS="--cfg force_rung3" CARGO_TARGET_DIR="$td" \
            cargo test --release -p ferrodec-decimal32 --features transcendentals,unbounded-ladder
}

lane_thumbv6m() {
    # The Cortex-M0+ floor build (no_std, no allocator by default),
    # then CI's `--all-features` cross builds (every opt-in feature,
    # including the test-only `telemetry` counters, must at least
    # compile for the floor target: the AtomicU64 lesson) and the
    # substrate crates CI cross-builds.
    cargo build --target thumbv6m-none-eabi --no-default-features \
        --features transcendentals,binary-float -p ferrodec &&
        cargo build --target thumbv6m-none-eabi --no-default-features \
            --features transcendentals,binary-float -p ferrodec-decimal64 &&
        cargo build --target thumbv6m-none-eabi --no-default-features \
            --features transcendentals,binary-float -p ferrodec-decimal32 &&
        cargo build --target thumbv6m-none-eabi --all-features -p ferrodec &&
        cargo build --target thumbv6m-none-eabi --all-features -p ferrodec-decimal64 &&
        cargo build --target thumbv6m-none-eabi --all-features -p ferrodec-decimal32 &&
        cargo build --target thumbv6m-none-eabi --no-default-features -p ferrodec-decimal &&
        cargo build --target thumbv6m-none-eabi -p ferrodec-ieee &&
        cargo build --target thumbv6m-none-eabi -p ferrodec-multiword
}

lane_kani() {
    # CI's four Kani invocations (about three minutes wall for the
    # whole suite). The dpd feature gates the DPD codec harnesses.
    cargo kani --package ferrodec-ieee &&
        cargo kani --features=transcendentals,dpd &&
        cargo kani --package ferrodec-decimal32 --features=fmt,dpd &&
        cargo kani --package ferrodec-decimal64 --features=transcendentals,dpd
}

ALL_LANES=(fmt clippy clippy_members rustdoc tests members transcend telemetry force_escalate ladder_audit force_adjudicate force_rung3 thumbv6m kani)
LANES=("$@")
if [ ${#LANES[@]} -eq 0 ]; then
    LANES=("${ALL_LANES[@]}")
fi

for l in "${LANES[@]}"; do
    case "$l" in
    fmt) lane fmt lane_fmt ;;
    clippy) lane clippy lane_clippy ;;
    clippy_members) lane clippy_members lane_clippy_members ;;
    rustdoc) lane rustdoc lane_rustdoc ;;
    tests) lane tests lane_tests ;;
    members) lane members lane_members ;;
    transcend) lane transcend lane_transcend ;;
    telemetry) lane telemetry lane_telemetry ;;
    force_escalate) lane force_escalate lane_force_escalate ;;
    ladder_audit) lane ladder_audit lane_ladder_audit ;;
    force_adjudicate) lane force_adjudicate lane_force_adjudicate ;;
    force_rung3) lane force_rung3 lane_force_rung3 ;;
    thumbv6m) lane thumbv6m lane_thumbv6m ;;
    kani) lane kani lane_kani ;;
    *)
        echo "unknown lane: $l (known: ${ALL_LANES[*]})" >&2
        exit 2
        ;;
    esac
done

echo
echo "==================================================================="
if [ ${#FAILED[@]} -eq 0 ]; then
    echo "=== battery: ALL LANES PASS (${LANES[*]})"
else
    echo "=== battery: FAILED lanes: ${FAILED[*]}"
    exit 1
fi
