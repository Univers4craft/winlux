#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
(cd "$root/testdll" && cargo build --release)
(cd "$root/testexe" && cargo build --release)
(cd "$root/testexe" && cargo rustc --release --bin delay_import -- -C link-arg=/delayload:testdll.dll)
out="$root/target/x86_64-pc-windows-msvc/release"
cp "$out/winlux_testdll.dll" "$out/testdll.dll"
echo "Tests Windows construits dans $out"
