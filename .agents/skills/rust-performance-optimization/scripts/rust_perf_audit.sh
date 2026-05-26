#!/usr/bin/env bash
set -euo pipefail

repo="${1:-.}"

if [[ ! -d "$repo" ]]; then
  echo "error: repo path does not exist: $repo" >&2
  exit 2
fi

cd "$repo"

echo "== Rust performance audit =="
printf 'repo: %s\n' "$(pwd)"

if command -v rustc >/dev/null 2>&1; then
  printf 'rustc: %s\n' "$(rustc --version)"
fi
if command -v cargo >/dev/null 2>&1; then
  printf 'cargo: %s\n' "$(cargo --version)"
fi

echo
echo "== Cargo targets =="
if [[ -f Cargo.toml ]] && command -v cargo >/dev/null 2>&1; then
  cargo metadata --no-deps --format-version 1 2>/dev/null \
    | sed 's/},/},\
/g' \
    | grep -E '"name"|"kind"|"edition"|"manifest_path"' \
    || true
else
  echo "Cargo.toml not found or cargo unavailable"
fi

echo
echo "== Profile settings =="
if command -v rg >/dev/null 2>&1; then
  rg -n '^\[profile\.|opt-level|lto|codegen-units|panic|overflow-checks|debug\s*=' Cargo.toml .cargo/config.toml .cargo/config 2>/dev/null || true
else
  grep -RInE '^\[profile\.|opt-level|lto|codegen-units|panic|overflow-checks|debug\s*=' Cargo.toml .cargo 2>/dev/null || true
fi

echo
echo "== Benchmarks =="
find benches -maxdepth 2 -type f 2>/dev/null | sort || true
if [[ -f Cargo.toml ]]; then
  grep -n '^\[\[bench\]\]' Cargo.toml 2>/dev/null || true
fi

echo
echo "== Likely optimization-sensitive patterns =="
if command -v rg >/dev/null 2>&1; then
  rg -n --glob '*.rs' 'get_unchecked|MaybeUninit|spare_capacity_mut|set_len|Vec::with_capacity|HashMap|HashSet|BufReader|BufWriter|println!|print!|\.clone\(\)|\.collect::<Vec|for .+ in .*\.\.=|wrapping_|checked_|saturating_|overflowing_|unsafe \{|repr\(C\)|NonZero|target_feature|std::arch|std::simd' . 2>/dev/null || true
else
  grep -RInE 'get_unchecked|MaybeUninit|spare_capacity_mut|set_len|Vec::with_capacity|HashMap|HashSet|BufReader|BufWriter|println!|print!|\.clone\(\)|\.collect::<Vec|wrapping_|checked_|saturating_|overflowing_|unsafe \{|repr\(C\)|NonZero|target_feature|std::arch|std::simd' . 2>/dev/null || true
fi
