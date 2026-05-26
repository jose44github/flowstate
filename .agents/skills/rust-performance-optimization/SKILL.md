---
name: rust-performance-optimization
description: Diagnose and optimize Rust performance in real codebases. Use when Codex needs to speed up Rust code, investigate regressions, review hot loops, reduce allocations or bounds checks, improve cache/layout behavior, tune Cargo/rustc build settings, interpret benchmark/profiling output, or choose safe versus unsafe optimization tactics.
---

# Rust Performance Optimization

Use this skill to optimize Rust code from evidence, not guesswork. Preserve correctness first; make narrow changes; verify with benchmarks, tests, assembly, or profiler output before claiming wins.

## Workflow

1. Establish the performance target.
   - Identify the user-visible workload, benchmark, regression, or hot function.
   - Record the command, input size, toolchain, profile, feature flags, CPU/OS constraints, and current timing.
   - Prefer existing project benchmarks. If missing, add the smallest benchmark that exercises the suspected path.

2. Collect baseline facts.
   - Run `scripts/rust_perf_audit.sh <repo>` when useful to summarize Cargo metadata, profile settings, benchmark targets, and likely optimization-sensitive code.
   - Use profiler output when available: `perf`, `samply`, `flamegraph`, `heaptrack`, `cargo instruments`, or project-specific tracing.
   - Do not optimize cold code because it looks inefficient.

3. Choose the investigation lane.
   - Read `references/rust-performance-playbook.md` for the slide-derived checklist.
   - Start with sections that match the evidence:
     - Bounds checks and vectorization for hot indexing loops.
     - Aliasing for `Vec`/field/reference interactions and missed store/load elimination.
     - Mandatory initialization and in-place construction for large buffers, IO, arrays, `Box`, `Rc`, and `Vec`.
     - Arithmetic semantics for overflow checks, inclusive ranges, casts, and divide-by-zero checks.
     - Standard library behavior for hashing, IO buffering, UTF-8, allocation, sorting, maps, and stdout.
     - Layout and cache behavior for structs, enums, `repr(C)`, and niche-aware types.
     - Floating point semantics for math-heavy loops where fast-math-like local intrinsics may matter.

4. Inspect generated code only when it answers a concrete question.
   - Use `cargo asm`, `cargo llvm-ir`, `cargo rustc -- --emit=asm`, or Compiler Explorer-style minimal examples.
   - Look for repeated bounds checks, panic paths inside loops, missed vectorization, redundant `memset`/`memcpy`, unnecessary allocation, missed inlining, and calls through slow stdlib paths.

5. Apply the least risky optimization that explains the evidence.
   - Prefer safe, idiomatic rewrites before unsafe code: slices over containers in hot loops, iterator `zip`/internal iteration, reslicing, common safe ranges, `BufReader`/`BufWriter`, preallocation, `MaybeUninit` APIs, layout-conscious types, and better algorithms.
   - Use unsafe only for isolated hot paths where safe code cannot express the needed invariant. Document the invariant locally and test it.
   - Avoid benchmark gaming: do not remove work, reduce input fidelity, or optimize only the benchmark harness.

6. Verify.
   - Run correctness tests first.
   - Rerun the same benchmark enough times to see noise. Use `hyperfine`, Criterion, or project-specific benchmarking. Compare against the baseline command.
   - Report absolute numbers, percent change, confidence/noise caveats, and files/functions changed.

## Rust-Specific Reminders

- Build and benchmark in release mode unless the user explicitly cares about debug performance.
- Check `Cargo.toml` profiles before suggesting `lto`, `codegen-units`, `panic=abort`, or `target-cpu=native`; these can improve speed or size but may change build time, portability, or panic behavior.
- Treat compiler-version effects as real. Bounds-check elimination, vectorization, copy elision, and inlining can change non-monotonically across Rust/LLVM versions.
- Do not assume iterators are always faster. They can remove bounds checks, but long chains, external iteration, debug builds, or aliasing through captured references can regress.
- Do not assume unsafe is faster. It can block alias analysis via raw pointers or introduce UB; verify the generated code and benchmark.

## Bundled Resources

- `references/rust-performance-playbook.md`: slide-derived optimization tactics and decision points.
- `scripts/rust_perf_audit.sh`: non-mutating repository audit for Cargo profiles, benches, and suspicious hot-code patterns.
