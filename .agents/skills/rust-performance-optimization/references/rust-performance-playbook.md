# Rust Performance Playbook

Source basis: Yuri Gribov and Zakhar Akimov, "Performance of Rust language", May 2026, `https://github.com/yugr/rust-slides/blob/main/EN.pdf`. Use this as a checklist, not as proof that a given code path is slow.

## Measurement Discipline

- Attribute a slowdown to a specific workload and hot path before editing.
- Prefer standard project benchmarks (`cargo bench`, Criterion) and profiler evidence.
- Expect small mean overheads to hide large local regressions: a 1% geomean can mean a few tests moved by 10% or more.
- Stabilize measurements when possible: fixed inputs, release builds, consistent CPU governor, warmed caches, and repeated runs.
- Verify correctness before and after every optimization.

## Bounds Checks

Symptoms:
- Panic/bounds-check calls in hot loops.
- Missed vectorization around indexed slice or `Vec` access.
- Repeated checks where one precondition would prove a whole loop safe.

Safe tactics:
- Use slices rather than `Vec`/containers in hot functions to simplify alias analysis and make lengths explicit.
- Compute a common safe range before iterating over multiple containers: `let n = x.len().min(y.len());`.
- Prefer `iter_mut().zip(other).for_each(...)`, `fold`, or `find` when it removes indexing. Verify long iterator chains.
- Reslice once before a loop: `let xs = &xs[..n];` then iterate/index `xs`.
- Build offset slices in two steps to avoid overflow-obscured proofs: prefer `&v[i..][..n]` over `&v[i..i + n]`.
- Add manual precondition asserts that replace many checks with one check, such as `assert_eq!(coefficients.len(), 64);`.
- Access the farthest required fixed index first or destructure a known-size prefix with `let [a, b, c] = slice[..3] else { ... };`.
- Avoid complex affine index expressions in hot loops when simple offsets or chunking express the same work.

Riskier tactics:
- Use `get_unchecked`, raw pointers, `unreachable_unchecked`, or `assert_unchecked` only when a tested local invariant proves safety.
- Consider manual SIMD (`std::arch`, nightly `std::simd`, or a well-maintained crate) only for genuinely vectorizable hot loops.
- Replacing checks with `min`, masking, or table padding changes error behavior; do it only when clamping/wrapping is semantically correct.

Notes:
- LLVM can eliminate or hoist many bounds checks, but not all. Do not rely on heroic optimization in non-trivial loops.
- Bounds checks can cost direct instructions and, more importantly, inhibit vectorization or add panic-path pressure.

## Aliasing

Symptoms:
- Loads are repeated after stores to another reference.
- Passing `&mut Vec<T>` and `&Vec<T>` or references to fields prevents obvious no-alias reasoning.
- Raw-pointer rewrites fail to optimize as expected.

Tactics:
- Prefer function signatures with `&mut [T]` and `&[T]` for hot loops.
- Split owner/container management from element processing: prepare lengths/capacity outside, then call a small slice-based worker.
- Create small helper functions specifically to expose noalias function arguments when a large function hides references inside locals or struct fields.
- Avoid raw pointers unless necessary; Rust aliasing benefits apply to references, not raw pointers.
- Keep mutable and shared borrows structurally separate.

Notes:
- Rust references give LLVM noalias information at function boundaries, similar to C `restrict`.
- References created inside functions may not carry the same metadata today, so helper boundaries can matter.

## Initialization and In-Place Construction

Symptoms:
- Repeated zeroing of buffers that are immediately overwritten.
- Large array/struct construction causes stack pressure, `memset`, or `memcpy`.
- `Box::new([value; HUGE])`, `Rc::new`, or vector push patterns create extra copies.
- IO loops allocate or initialize fresh buffers each iteration.

Safe or mostly safe tactics:
- Reuse buffers across iterations.
- Use `Vec::with_capacity`, `reserve`, `spare_capacity_mut`, and then carefully set length after initialization.
- Use `Box::new_uninit`, `Rc::new_uninit`, `MaybeUninit`, or collection APIs designed for uninitialized backing storage.
- Use `Vec::into_boxed_slice()` instead of constructing huge boxed arrays on the stack.
- For IO, use APIs that write into unfilled buffers when available, such as `BorrowedBuf`/`read_buf` style APIs.

Unsafe requirements:
- Never create `&T` or `&mut T` to uninitialized memory.
- Use pointer writes or `MaybeUninit::write`; only call `assume_init` after every field/element has been initialized.
- On panic paths, avoid dropping uninitialized elements.

Notes:
- Redundant scalar initialization is usually optimized away; arrays and large aggregates are the common problem.
- Copy/move elision improves over time but is release-build and compiler-version dependent.

## Symbol Visibility, Inlining, and Codegen

Tactics:
- Keep functions non-`pub` unless public API requires export.
- Prefer `pub(crate)`, `pub(super)`, or private helpers for hot internal paths so the compiler can localize and optimize.
- Use small worker functions to expose constants, noalias slice arguments, or single-call inlining opportunities.
- Use `#[inline]`/`#[inline(always)]` sparingly and verify; over-inlining can increase code size and hurt cache behavior.
- Consider profile settings only with a measured reason: `lto`, `codegen-units = 1`, `panic = "abort"`, `opt-level`, and `target-cpu=native` trade build time, portability, and behavior.

Notes:
- Rust defaults already localize many functions better than typical C/C++ defaults.

## Struct Layout, Enums, and Cache Behavior

Tactics:
- Avoid `#[repr(C)]` on performance-critical internal types unless ABI/layout stability is required.
- Let Rust reorder fields for smaller layout on ordinary `repr(Rust)` structs.
- Check sizes with `std::mem::size_of`, `cargo bloat`, heap profiles, or cache-miss profiles before layout edits.
- Use niche-aware types where they preserve meaning: references, `NonNull<T>`, `NonZero*`, and `Option<T>` can be compact.
- Group hot fields or split cold fields only when profiles show cache pressure.

Notes:
- Field reordering alone may not dominate, but it can enable or disable other compiler or CPU optimizations.
- Unsafe code must not assume field order for `repr(Rust)`.

## Arithmetic Semantics

Symptoms:
- Hot loops compiled with overflow checks do not vectorize.
- Inclusive ranges add extra checks or branches.
- Casts through `as` silently wrap/truncate and hide correctness bugs.

Tactics:
- Use exclusive ranges where possible: prefer `1..n + 1` only if overflow is impossible or checked, otherwise restructure.
- Use `.for_each`, `.fold`, or chunking when it helps avoid inclusive-range control-flow overhead.
- Use explicit arithmetic semantics: `wrapping_add`, `checked_add`, `saturating_add`, `overflowing_add`, or `Wrapping<T>`.
- Use `NonZero*` types to encode nonzero divisors and remove divide-by-zero checks where the invariant is real.
- Use `try_from`/`try_into` for lossy casts when correctness matters; do not replace them with `as` for speed unless truncation is intended.
- Avoid enabling release `-C overflow-checks=on` for hot arithmetic without measuring; it can block vectorization.

Riskier tactics:
- `unchecked_add`, `to_int_unchecked`, `unreachable_unchecked`, and related APIs require airtight invariants and local tests.

## Standard Library Hot Paths

Common slow defaults:
- `HashMap`/`HashSet` use a DoS-resistant default hasher. Faster hashers can help trusted-key workloads.
- Rust IO is often unbuffered unless wrapped. Use `BufReader`, `BufWriter`, or locked stdout handles.
- `print!`/`println!` lock stdout and formatting can dominate tight output loops.
- `String`/`str` APIs enforce UTF-8 validity and boundary checks.
- Containers check capacity overflow and allocation success.

Tactics:
- Switch hashers only when key source and collision risk are understood.
- Buffer file, socket, stdin, and stdout paths.
- Preallocate containers with realistic capacity.
- Use byte slices for byte-oriented work that does not need UTF-8.
- Prefer stdlib algorithms that are already strong before replacing them; Rust `HashMap`, `BTreeMap`, and sorting can be faster than naive alternatives.

## Floating Point and Fast-Math-Like Optimizations

Facts:
- Rust intentionally does not provide a global `-ffast-math` equivalent.
- Global fast-math can remove NaN/Inf checks, reorder operations, change rounding, and break numerically careful algorithms.

Tactics:
- Keep exact IEEE behavior by default.
- For a measured math hotspot, consider local nightly intrinsics such as `f*_fast`, `f*_algebraic`, or `algebraic_*` only when the project accepts nightly and the numerical contract permits it.
- Use explicit SIMD or domain-specific math crates when semantics are acceptable and measured.
- Do not apply fast-math-like rewrites to Kahan summation, NaN-sensitive code, signed-zero-sensitive code, or code with documented rounding requirements.

## Build and Tooling Checklist

Useful commands:
- `cargo test --release`
- `cargo bench`
- `hyperfine --warmup 3 '<command>'`
- `cargo flamegraph --bench <bench>`
- `perf record --call-graph=dwarf <command>` then `perf report`
- `cargo asm <path::to::fn>`
- `cargo llvm-ir <path::to::fn>`
- `cargo bloat --release`
- `cargo metadata --no-deps --format-version 1`

Profile settings to inspect:
- `[profile.release] opt-level`
- `lto`
- `codegen-units`
- `panic`
- `debug`
- `overflow-checks`
- target-specific `RUSTFLAGS`, especially `target-cpu` and `target-feature`

## Reporting Results

Report:
- Baseline and optimized command.
- Hardware/toolchain/profile.
- Before/after timing, memory, allocation count, binary size, or profiler share.
- Noise and confidence.
- Correctness checks run.
- Why the change addresses the measured bottleneck.
