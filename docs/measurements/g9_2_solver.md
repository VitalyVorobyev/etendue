# G9.2, G9.3: the Scheimpflug solver without argmin

- **Gate G9.2** (`docs/pivot/PLAN.md` P9): the in-tree Nelder–Mead reproduces the optimum that
  argmin's Nelder–Mead found on the solver's tests, recorded before the port, to 1e-6 rad and
  1e-6 m.
- **Gate G9.3:** the wasm graph has no `getrandom`.
- Measured 2026-10-19 on branch `p9c/nelder-mead`, Apple M4 Pro.

## Procedure

1. With argmin 0.11 (restarted, as P9a left the solver), each solver test printed
   `(tau_x, tau_y, s_o, max CoC px)`.
2. `solver::nelder_mead` replaced argmin. It uses argmin's coefficients and the same
   standard-deviation stopping rule at 1e-6, with at most 500 iterations a run and the same
   restarts.
3. The same tests ran again. `assert_reproduces_argmin` in `solver/scheimpflug.rs` pins the
   recorded optima.

```bash
cargo test -p etendue-core --lib solver
cargo tree -p etendue-wasm --target wasm32-unknown-unknown -i getrandom   # G9.3: nothing to print
```

## G9.2 result: PASS

| Test | | argmin (recorded) | in-tree | max \|Δ\| |
|---|---|---|---|---|
| fronto-parallel fan | τx, τy (rad) | 2.011e-7, 3.943e-8 | −1.186e-7, −3.127e-8 | 3.2e-7 rad |
| | s_o (m) | 0.59999997 | 0.60000003 | 5.6e-8 m |
| | worst CoC (px) | 3.98e-6 | 3.36e-6 | |
| solving lowers the worst-case CoC | τx, τy (rad) | −2.2354e-8, 0.1197571375 | −2.2354e-8, 0.1197571375 | 2.9e-12 rad |
| | s_o (m) | 0.4924444769 | 0.4924444769 | 3.3e-11 m |
| | worst CoC (px) | 1.939e-6 | 1.946e-6 | |
| default scene | τx, τy (rad) | −8.33e-9, 0.0522313716 | 2.69e-9, 0.0522313755 | 1.1e-8 rad |
| | s_o (m) | 0.6171709483 | 0.6171709355 | 1.3e-8 m |
| | worst CoC (px) | 2.82e-6 | 2.60e-6 | |

## G9.3 result: PASS

`cargo tree -p etendue-wasm --target wasm32-unknown-unknown -i getrandom` prints nothing. The
solver is now always built into `etendue-core`, and therefore into the wasm kernel. `argmin` and
`argmin-math` have left `Cargo.toml` and `Cargo.lock`.
