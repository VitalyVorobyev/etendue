# Scheimpflug Derivation

This chapter is the derivation of the Scheimpflug-tilted plane of best focus
and the geometric circle of confusion that underpin
`etendue-core::optics`. It is reproduced verbatim from
`docs/derivations/scheimpflug_pobf.md` — the single source of truth for the
physics, also embedded in the `optics::coc` module doc-comment in the source
tree. The M4 implementation rests on this derivation being correct (the
"kill-gate" tests in `optics/coc.rs` and `optics/thick_lens.rs` check the
worked numbers below).

{{#include ../../docs/derivations/scheimpflug_pobf.md}}
