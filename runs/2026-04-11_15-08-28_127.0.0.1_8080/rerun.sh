#!/usr/bin/env bash
set -euo pipefail
cd /home/warmond/AI-Research/Sigma-Morpho
/home/warmond/AI-Research/Sigma-Morpho/target/release/sigma_morpho --base-url http://127.0.0.1:8080 --wordlist /home/warmond/AI-Research/Sigma-Morpho/dict/local-validation-control.txt --workers 3 --rounds 1 --recursion-depth 0 --timeout-ms 4000 --initial-delay-ms 150 --min-delay-ms 100 --max-delay-ms 300 --latency-threshold-ms 200 --findings-file /home/warmond/AI-Research/Sigma-Morpho/runs/2026-04-11_15-08-28_127.0.0.1_8080/findings.txt --interesting-statuses 200\,403\,429 --min-body-bytes 0 --speed-mode safe --client-profile api-diagnostic
