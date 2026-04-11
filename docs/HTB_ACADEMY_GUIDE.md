# HTB Academy Guide

## Intended use

Sigma Morpho is suitable for:

- authorized lab targets
- performance/reliability observation during directory and endpoint testing
- studying how response sequences affect adaptive pacing
- documenting safe experimentation methodology

## Recommended workflow

1. Confirm written authorization for the target.
2. Start with a small wordlist and low worker count.
3. Choose a fixed client profile for the run.
4. Capture the final summary after each run.
5. Compare profile transitions (`baseline`, `cautious`, `defensive`) against observed server behavior.
6. Use scenario mode and `--compare-profiles` for offline experimentation before touching a live lab target.
7. Use the generated telemetry in your report to justify tuning decisions.

## Example

```bash
cargo run -- \
  --base-url https://lab.example \
  --wordlist wordlist.txt \
  --workers 4 \
  --rounds 1 \
  --initial-delay-ms 75 \
  --min-delay-ms 50 \
  --max-delay-ms 1500 \
  --latency-threshold-ms 700 \
  --client-profile browser-desktop \
  --simulation-mode \
  --authorized-target
```

## Offline research example

```bash
cargo run -- \
  --scenario mixed-defense \
  --simulation-mode \
  --compare-profiles
```

## Reporting suggestions

Include:

- scope of authorization
- run configuration
- latency threshold used
- number of requests generated
- observed 403/429/503 events
- final adaptive profile
- whether the RSNN reduced or recovered delay during the run

## Important note

If the lab requires higher safety margins, reduce `--workers`, increase `--initial-delay-ms`, and keep the wordlist minimal. The tool is designed to slow down when protection signals appear, not to push past them.
