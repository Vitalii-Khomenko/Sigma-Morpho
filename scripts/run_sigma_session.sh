#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd -P)

print_help() {
    cat <<'EOF'
Automatic Sigma Morpho runner.

What it does:
- starts without interactive prompts
- always creates a fresh run directory inside runs/
- names the directory using date, time, target host or IP, and target port
- always saves command.txt, rerun.sh, metadata.txt, build logs, stdout, stderr, timing, and findings.txt

Usage:
  scripts/run_sigma_session.sh
  scripts/run_sigma_session.sh --help

Optional environment overrides:
  SIGMA_BASE_URL=http://127.0.0.1:8080
  SIGMA_WORDLIST=wordlist.txt
  SIGMA_WORKERS=16
  SIGMA_ROUNDS=1
  SIGMA_RECURSION_DEPTH=0
  SIGMA_TIMEOUT_MS=5000
  SIGMA_INITIAL_DELAY_MS=50
  SIGMA_MIN_DELAY_MS=25
  SIGMA_MAX_DELAY_MS=5000
  SIGMA_LATENCY_THRESHOLD_MS=800
  SIGMA_INTERESTING_STATUSES=200,204,301,302,307,308,401,403,405,500
  SIGMA_MIN_BODY_BYTES=0
  SIGMA_MAX_BODY_BYTES=
  SIGMA_SPEED_MODE=balanced
  SIGMA_CLIENT_PROFILE=research-default
  SIGMA_BUILD_PROFILE=release
  SIGMA_SIMULATION_MODE=no
  SIGMA_COMPARE_PROFILES_LIVE=no
  SIGMA_REBUILD_CLIENT_ON_ADVISORY=no
  SIGMA_DISABLE_SOFT_404_FILTER=no
  SIGMA_SNN_STATE_FILE=
  SIGMA_PROXIES_FILE=
  SIGMA_TOR_PROXY=
  SIGMA_TOR_CONTROL=
  SIGMA_TOR_PASSWORD=
  SIGMA_AUTHORIZED_TARGET=no
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    print_help
    exit 0
fi

cd "$REPO_ROOT"

canonical_path() {
    local input_path=$1
    local candidate_path dir_part file_part

    if [[ "$input_path" == /* ]]; then
        candidate_path=$input_path
    else
        candidate_path="$REPO_ROOT/$input_path"
    fi

    if [[ -d "$candidate_path" ]]; then
        (
            cd -- "$candidate_path" || exit 1
            pwd -P
        )
        return
    fi

    dir_part=$(dirname -- "$candidate_path")
    file_part=$(basename -- "$candidate_path")
    (
        cd -- "$dir_part" || exit 1
        printf '%s/%s\n' "$(pwd -P)" "$file_part"
    )
}

extract_scheme() {
    local url=$1
    printf '%s\n' "${url%%://*}"
}

extract_host_port() {
    local url=$1
    local remainder host_port

    remainder=${url#*://}
    host_port=${remainder%%/*}
    printf '%s\n' "$host_port"
}

extract_host() {
    local url=$1
    local host_port host_only

    host_port=$(extract_host_port "$url")
    host_only=${host_port%%:*}
    host_only=${host_only#[}
    host_only=${host_only%]}
    printf '%s\n' "$host_only"
}

extract_port() {
    local url=$1
    local scheme host_port port

    scheme=$(extract_scheme "$url")
    host_port=$(extract_host_port "$url")

    if [[ "$host_port" == \[*\]:* ]]; then
        port=${host_port##*]:}
        printf '%s\n' "$port"
        return
    fi

    if [[ "$host_port" == \[*\] ]]; then
        case "$scheme" in
            https) printf '443\n' ;;
            http) printf '80\n' ;;
            *) printf 'unknown-port\n' ;;
        esac
        return
    fi

    if [[ "$host_port" == *:* ]]; then
        port=${host_port##*:}
        printf '%s\n' "$port"
        return
    fi

    case "$scheme" in
        https) printf '443\n' ;;
        http) printf '80\n' ;;
        *) printf 'unknown-port\n' ;;
    esac
}

sanitize_segment() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9._-' '_'
}

as_yes_no() {
    local value=${1,,}

    case "$value" in
        y|yes|1|true|on) printf 'yes\n' ;;
        n|no|0|false|off|'') printf 'no\n' ;;
        *)
            echo "Expected yes/no style value, got: $1" >&2
            exit 1
            ;;
    esac
}

append_flag_if_yes() {
    local answer=$1
    local flag=$2

    if [[ "$answer" == "yes" ]]; then
        CMD+=("$flag")
    fi
}

append_optional_arg() {
    local flag=$1
    local value=$2

    if [[ -n "$value" ]]; then
        CMD+=("$flag" "$value")
    fi
}

render_command() {
    local rendered=""
    local part

    for part in "$@"; do
        if [[ -n "$rendered" ]]; then
            rendered+=" "
        fi
        rendered+=$(printf '%q' "$part")
    done

    printf '%s\n' "$rendered"
}

next_run_dir() {
    local base_rel=$1
    local candidate_rel=$base_rel
    local counter=1

    while [[ -e "$REPO_ROOT/$candidate_rel" ]]; do
        candidate_rel="${base_rel}_$counter"
        counter=$((counter + 1))
    done

    printf '%s\n' "$candidate_rel"
}

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required but was not found in PATH" >&2
    exit 1
fi

base_url=${SIGMA_BASE_URL:-http://127.0.0.1:8080}
wordlist_path=${SIGMA_WORDLIST:-wordlist.txt}
workers=${SIGMA_WORKERS:-16}
rounds=${SIGMA_ROUNDS:-1}
recursion_depth=${SIGMA_RECURSION_DEPTH:-0}
timeout_ms=${SIGMA_TIMEOUT_MS:-5000}
initial_delay_ms=${SIGMA_INITIAL_DELAY_MS:-50}
min_delay_ms=${SIGMA_MIN_DELAY_MS:-25}
max_delay_ms=${SIGMA_MAX_DELAY_MS:-5000}
latency_threshold_ms=${SIGMA_LATENCY_THRESHOLD_MS:-800}
interesting_statuses=${SIGMA_INTERESTING_STATUSES:-200,204,301,302,307,308,401,403,405,500}
min_body_bytes=${SIGMA_MIN_BODY_BYTES:-0}
max_body_bytes=${SIGMA_MAX_BODY_BYTES:-}
speed_mode=${SIGMA_SPEED_MODE:-balanced}
client_profile=${SIGMA_CLIENT_PROFILE:-research-default}
build_profile=${SIGMA_BUILD_PROFILE:-release}
simulation_mode=$(as_yes_no "${SIGMA_SIMULATION_MODE:-no}")
compare_profiles_live=$(as_yes_no "${SIGMA_COMPARE_PROFILES_LIVE:-no}")
rebuild_client_on_advisory=$(as_yes_no "${SIGMA_REBUILD_CLIENT_ON_ADVISORY:-no}")
disable_soft_404_filter=$(as_yes_no "${SIGMA_DISABLE_SOFT_404_FILTER:-no}")
authorized_target=$(as_yes_no "${SIGMA_AUTHORIZED_TARGET:-no}")
snn_state_file=${SIGMA_SNN_STATE_FILE:-}
proxies_file=${SIGMA_PROXIES_FILE:-}
tor_proxy=${SIGMA_TOR_PROXY:-}
tor_control=${SIGMA_TOR_CONTROL:-}
tor_password=${SIGMA_TOR_PASSWORD:-}

case "$build_profile" in
    release|debug)
        ;;
    *)
        echo "Build profile must be either release or debug, got: $build_profile" >&2
        exit 1
        ;;
esac

host=$(extract_host "$base_url")
port=$(extract_port "$base_url")

if [[ -z "$host" ]]; then
    echo "Could not extract host from base URL: $base_url" >&2
    exit 1
fi

if [[ -z "$port" ]]; then
    echo "Could not extract port from base URL: $base_url" >&2
    exit 1
fi

if ! wordlist_abs=$(canonical_path "$wordlist_path"); then
    echo "Failed to resolve wordlist path: $wordlist_path" >&2
    exit 1
fi

if [[ ! -e "$wordlist_abs" ]]; then
    echo "Wordlist path does not exist: $wordlist_abs" >&2
    exit 1
fi

if [[ -n "$snn_state_file" ]]; then
    if ! snn_state_file=$(canonical_path "$snn_state_file"); then
        echo "Failed to resolve SNN state file path: $snn_state_file" >&2
        exit 1
    fi
    if [[ ! -e "$snn_state_file" ]]; then
        echo "SNN state file does not exist: $snn_state_file" >&2
        exit 1
    fi
fi

if [[ -n "$proxies_file" ]]; then
    if ! proxies_file=$(canonical_path "$proxies_file"); then
        echo "Failed to resolve proxies file path: $proxies_file" >&2
        exit 1
    fi
    if [[ ! -e "$proxies_file" ]]; then
        echo "Proxies file does not exist: $proxies_file" >&2
        exit 1
    fi
fi

timestamp=$(date +%F_%H-%M-%S)
host_slug=$(sanitize_segment "$host")
port_slug=$(sanitize_segment "$port")
run_dir_rel=$(next_run_dir "runs/${timestamp}_${host_slug}_${port_slug}")
run_dir_abs="$REPO_ROOT/$run_dir_rel"
mkdir -p "$run_dir_abs"

findings_file="$run_dir_abs/findings.txt"

if [[ "$build_profile" == "release" ]]; then
    build_args=(build --release)
    binary_path="$REPO_ROOT/target/release/sigma_morpho"
else
    build_args=(build)
    binary_path="$REPO_ROOT/target/debug/sigma_morpho"
fi

CMD=(
    "$binary_path"
    --base-url "$base_url"
    --wordlist "$wordlist_abs"
    --workers "$workers"
    --rounds "$rounds"
    --recursion-depth "$recursion_depth"
    --timeout-ms "$timeout_ms"
    --initial-delay-ms "$initial_delay_ms"
    --min-delay-ms "$min_delay_ms"
    --max-delay-ms "$max_delay_ms"
    --latency-threshold-ms "$latency_threshold_ms"
    --findings-file "$findings_file"
    --interesting-statuses "$interesting_statuses"
    --min-body-bytes "$min_body_bytes"
    --speed-mode "$speed_mode"
    --client-profile "$client_profile"
)

append_optional_arg --max-body-bytes "$max_body_bytes"
append_flag_if_yes "$simulation_mode" --simulation-mode
append_flag_if_yes "$compare_profiles_live" --compare-profiles-live
append_flag_if_yes "$rebuild_client_on_advisory" --rebuild-client-on-advisory
append_flag_if_yes "$disable_soft_404_filter" --disable-soft-404-filter
append_optional_arg --snn-state-file "$snn_state_file"
append_optional_arg --proxies-file "$proxies_file"
append_optional_arg --tor-proxy "$tor_proxy"
append_optional_arg --tor-control "$tor_control"
append_optional_arg --tor-password "$tor_password"
append_flag_if_yes "$authorized_target" --authorized-target

resolved_command=$(render_command "${CMD[@]}")

printf '%s\n' "$resolved_command" > "$run_dir_abs/command.txt"

cat > "$run_dir_abs/rerun.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
cd $(printf '%q' "$REPO_ROOT")
$resolved_command
EOF

chmod +x "$run_dir_abs/rerun.sh"

{
    printf 'started_at=%s\n' "$(date --iso-8601=seconds)"
    printf 'run_dir=%s\n' "$run_dir_rel"
    printf 'target=%s\n' "$base_url"
    printf 'host=%s\n' "$host"
    printf 'port=%s\n' "$port"
    printf 'build_profile=%s\n' "$build_profile"
    printf 'wordlist=%s\n' "$wordlist_abs"
    printf 'workers=%s\n' "$workers"
    printf 'rounds=%s\n' "$rounds"
    printf 'recursion_depth=%s\n' "$recursion_depth"
    printf 'timeout_ms=%s\n' "$timeout_ms"
    printf 'initial_delay_ms=%s\n' "$initial_delay_ms"
    printf 'min_delay_ms=%s\n' "$min_delay_ms"
    printf 'max_delay_ms=%s\n' "$max_delay_ms"
    printf 'latency_threshold_ms=%s\n' "$latency_threshold_ms"
    printf 'interesting_statuses=%s\n' "$interesting_statuses"
    printf 'min_body_bytes=%s\n' "$min_body_bytes"
    printf 'max_body_bytes=%s\n' "${max_body_bytes:-none}"
    printf 'speed_mode=%s\n' "$speed_mode"
    printf 'client_profile=%s\n' "$client_profile"
    printf 'simulation_mode=%s\n' "$simulation_mode"
    printf 'compare_profiles_live=%s\n' "$compare_profiles_live"
    printf 'rebuild_client_on_advisory=%s\n' "$rebuild_client_on_advisory"
    printf 'disable_soft_404_filter=%s\n' "$disable_soft_404_filter"
    printf 'snn_state_file=%s\n' "${snn_state_file:-none}"
    printf 'proxies_file=%s\n' "${proxies_file:-none}"
    printf 'tor_proxy=%s\n' "${tor_proxy:-none}"
    printf 'tor_control=%s\n' "${tor_control:-none}"
    printf 'findings_file=%s\n' "$findings_file"
    printf 'authorized_target=%s\n' "$authorized_target"
} > "$run_dir_abs/metadata.txt"

if [[ -f "$wordlist_abs" ]] && command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$wordlist_abs" > "$run_dir_abs/wordlist.sha256"
fi

echo "Run directory: $run_dir_rel"
echo "Target: $base_url"
echo "Port: $port"
echo "Building Sigma Morpho first. Build logs will be stored in the run directory."

set +e
cargo "${build_args[@]}" > "$run_dir_abs/build.stdout.log" 2> "$run_dir_abs/build.stderr.log"
build_exit_code=$?
set -e

printf 'build_exit_code=%s\n' "$build_exit_code" >> "$run_dir_abs/metadata.txt"

if [[ "$build_exit_code" -ne 0 ]]; then
    printf 'ended_at=%s\n' "$(date --iso-8601=seconds)" >> "$run_dir_abs/metadata.txt"
    printf 'run_exit_code=%s\n' "$build_exit_code" >> "$run_dir_abs/metadata.txt"
    echo "Build failed. See $run_dir_rel/build.stderr.log" >&2
    exit "$build_exit_code"
fi

echo "Executing Sigma Morpho. Stdout, stderr, timing, and findings will be saved automatically."

TIMEFORMAT=$'real=%3R\nuser=%3U\nsys=%3S'
set +e
{
    time "${CMD[@]}" > "$run_dir_abs/sigma.stdout.log" 2> "$run_dir_abs/sigma.stderr.log"
} 2> "$run_dir_abs/time.txt"
run_exit_code=$?
set -e

printf 'ended_at=%s\n' "$(date --iso-8601=seconds)" >> "$run_dir_abs/metadata.txt"
printf 'run_exit_code=%s\n' "$run_exit_code" >> "$run_dir_abs/metadata.txt"

echo "Run finished with exit code: $run_exit_code"
echo "Artifacts:"
echo "  $run_dir_rel/command.txt"
echo "  $run_dir_rel/rerun.sh"
echo "  $run_dir_rel/metadata.txt"
echo "  $run_dir_rel/build.stdout.log"
echo "  $run_dir_rel/build.stderr.log"
echo "  $run_dir_rel/sigma.stdout.log"
echo "  $run_dir_rel/sigma.stderr.log"
echo "  $run_dir_rel/time.txt"
echo "  $run_dir_rel/findings.txt"

exit "$run_exit_code"