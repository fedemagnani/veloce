#!/usr/bin/env bash
# Emits the assembly of the swmr hot paths in examples/swmr_codegen.rs, one file per symbol and target.
# Usage: scripts/swmr-codegen.sh [target...]   (default: the host; e.g. add x86_64-apple-ios on Apple silicon)
set -euo pipefail

toolchain="${TOOLCHAIN:-nightly}"
host="$(rustc +"$toolchain" -vV | sed -n 's/^host: //p')"
targets=("$@")
if [ ${#targets[@]} -eq 0 ]; then
    targets=("$host")
fi

symbols=(swmr_publish_p8 swmr_publish_p4k swmr_latest_p8 swmr_publish_async_p8 swmr_latest_async_p8)
root="$(cd "$(dirname "$0")/.." && pwd)"
out_root="$root/target/swmr-codegen"

for target in "${targets[@]}"; do
    # assembly only, so targets whose linker is missing still work
    cargo +"$toolchain" rustc --quiet --release --example swmr_codegen --target "$target" -- --emit asm
    # newest listing, as its location depends on the cargo version's build directory layout
    asm="$(find "$root/target/$target/release" -name 'swmr_codegen*.s' -print0 | xargs -0 ls -t | head -1)"
    out="$out_root/$target"
    mkdir -p "$out"

    echo "== $target"
    for symbol in "${symbols[@]}"; do
        # identical functions get merged, leaving `symbol = other` behind: follow it to the body
        alias="$(sed -nE "s/^_?$symbol = _?([A-Za-z0-9_]+)$/\1/p" "$asm")"
        source="${alias:-$symbol}"
        # Mach-O prefixes symbols with an underscore, ELF doesn't
        body="$(awk -v a="$source:" -v b="_$source:" '$1 == a || $1 == b {on = 1} on {print} on && /\.cfi_endproc/ {exit}' "$asm")"
        echo "$body" > "$out/$symbol.s"
        instructions="$(echo "$body" | grep -cE '^\s+[a-z]' || true)"
        barriers="$(echo "$body" | grep -oE '\b(ldar[a-z]*|ldapr[a-z]*|ldapur[a-z]*|stlr[a-z]*|stlur[a-z]*|dmb|dsb|isb|xchg[a-z]*|mfence|lock)\b' | sort | uniq -c | tr -s ' ' | paste -sd, - || true)"
        calls="$(echo "$body" | grep -oE '\b(bl|b|call|jmp)\s+_?[A-Za-z_][A-Za-z0-9_$.]*' | awk '{print $2}' | grep -vE '^L|^\.L' | sort -u | paste -sd' ' - || true)"
        merged="${alias:+ (merged into $alias)}"
        printf '%-24s %4s instr | barriers: %s | calls: %s%s\n' "$symbol" "$instructions" "${barriers:-none}" "${calls:-none}" "$merged"
    done
    echo "   full listings in $out"
done
