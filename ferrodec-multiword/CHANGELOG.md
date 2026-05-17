# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-17

Initial release. The fixed-width wide-integer primitives (`U256` /
`U384` / `U512` and their base-10 and bit operations) were extracted
from `ferrodec`'s private `multiword` module into this standalone
`no_std` foundation crate (fd-r0l P0a.1, commit `82a7fe1`) so the
frozen, Kani-proven arithmetic and transcendental cores depend on a
stable base rather than on `ferrodec-transcend`. Behaviour-neutral:
the moved code is byte-identical to the pre-move `ferrodec` module and
its callers' tests stay green unchanged.
