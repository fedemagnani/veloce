# llvm-mca's cost estimates of every probe on the host CPU, beside `cargo codegen stats`
mca:
    cargo codegen stats --mca

# Hardware counters per perfect_hash lookup: builds as you, then asks for sudo since kperf needs root
counters:
    #!/usr/bin/env bash
    set -euo pipefail
    bin=$(cargo bench --bench perfect_hash --features criterion --no-run --message-format=json |
        jq -r 'select(.target.name == "perfect_hash" and .executable != null) | .executable')
    sudo "$bin" --counters
