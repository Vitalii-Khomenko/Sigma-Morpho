#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd -P)

print_help() {
    cat <<'EOF'
Interactive Sigma Morpho runner.

What it does:
- asks for the most common run parameters with short explanations
- creates a timestamped run directory inside runs/
- names the directory using date, time, label, and target host or IP
- saves build logs, command, metadata, stdout, stderr, timing, and findings automatically
- writes a rerun.sh file with the exact resolved command

Usage:
  scripts/run_sigma_session.sh
  scripts/run_sigma_session.sh --help

Notes:
- local targets do not require --authorized-target
- remote targets require explicit confirmation and the wrapper adds --authorized-target
- the wrapper builds the Rust binary before every run so logs stay reproducible
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    print_help
    exit 0
fi

cd "$REPO_ROOT"

prompt_with_default() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local raw_value

    printf '\n%s\n' "$label"
    printf 'Hint: %s\n' "$hint"
    printf 'Value [%s]: ' "$default_value"
    read -r raw_value

    if [[ -z "$raw_value" ]]; then
        raw_value=$default_value
    fi

    printf -v "$__resultvar" '%s' "$raw_value"
}

prompt_optional() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local raw_value

    printf '\n%s\n' "$label"
    printf 'Hint: %s\n' "$hint"
    printf 'Value [leave empty to skip]: '
    read -r raw_value
    printf -v "$__resultvar" '%s' "$raw_value"
}

prompt_yes_no() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local raw_value normalized

    printf '\n%s\n' "$label"
    printf 'Hint: %s\n' "$hint"
    printf 'Value [%s]: ' "$default_value"
    read -r raw_value

    normalized=${raw_value:-$default_value}
    normalized=${normalized,,}

    case "$normalized" in
        y|yes|1|true)
            printf -v "$__resultvar" '%s' "yes"
            ;;
        n|no|0|false)
            printf -v "$__resultvar" '%s' "no"
            ;;
        *)
            echo "Expected yes or no, got: $normalized" >&2
            exit 1
            ;;
    esac
}

canonical_path() {
    local input_path=$1
    local dir_part file_part

    if [[ -d "$input_path" ]]; then
        (
            cd -- "$input_path"
            pwd -P
        )
        return
    fi

    dir_part=$(dirname -- "$input_path")
    file_part=$(basename -- "$input_path")
    (
        cd -- "$dir_part"
        printf '%s/%s\n' "$(pwd -P)" "$file_part"
    )
}

extract_host() {
    local url=$1
    local remainder host_port host_only

    remainder=${url#*://}
    host_port=${remainder%%/*}
    host_only=${host_port%%:*}
    host_only=${host_only#[}
    host_only=${host_only%]}
    printf '%s\n' "$host_only"
}

sanitize_segment() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9._-' '_'
}

is_local_host() {
    local host=$1
    [[ "$host" == "localhost" || "$host" == "127.0.0.1" || "$host" == "::1" || "$host" == *.localhost ]]
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

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required but was not found in PATH" >&2
    exit 1
fi

echo "Sigma Morpho interactive runner"
echo "Press Enter to accept the default value shown in brackets."
echo "All run artifacts will be saved automatically under runs/."

prompt_with_default run_label "Run label" "Short suffix for the run directory name." "interactive"
prompt_with_default build_profile "Build profile" "Use release for normal runs or debug for faster rebuilds." "release"
prompt_with_default base_url "Base URL" "Full target URL including scheme, for example http://127.0.0.1:8080." "http://127.0.0.1:8080"
prompt_with_default wordlist_path "Wordlist path" "File or directory relative to repo root or absolute path." "wordlist.txt"
prompt_with_default workers "Workers" "Parallel request workers. Higher values increase pressure." "16"
prompt_with_default rounds "Rounds" "How many times the seed wordlist should be replayed." "1"
prompt_with_default recursion_depth "Recursion depth" "0 disables recursion. Use 1 or more to recurse into found directories." "0"
prompt_with_default timeout_ms "Timeout ms" "Per-request timeout in milliseconds." "5000"
prompt_with_default initial_delay_ms "Initial delay ms" "Starting adaptive delay before the RSNN reacts." "50"
prompt_with_default min_delay_ms "Min delay ms" "Lower bound for adaptive pacing." "25"
prompt_with_default max_delay_ms "Max delay ms" "Upper bound for adaptive pacing." "5000"
prompt_with_default latency_threshold_ms "Latency threshold ms" "Latency above this value counts as pressure." "800"
prompt_with_default interesting_statuses "Interesting statuses" "Comma-separated HTTP status codes to write into findings.txt." "200,204,301,302,307,308,401,403,405,500"
prompt_with_default min_body_bytes "Min body bytes" "Ignore hits smaller than this size." "0"
prompt_optional max_body_bytes "Max body bytes" "Optional upper body-size limit for findings."
prompt_with_default speed_mode "Speed mode" "One of: safe, balanced, fast, aggressive." "balanced"
prompt_with_default client_profile "Client profile" "One of: research-default, browser-desktop, mobile-safari, api-diagnostic." "research-default"
prompt_yes_no simulation_mode "Simulation mode" "Use simulator only and avoid real network activity." "no"
prompt_yes_no compare_profiles_live "Compare profiles live" "Run the same workload with every fixed client profile." "no"
prompt_yes_no rebuild_client_on_advisory "Rebuild client on advisory" "Hot-swap the reqwest client when the neuro layer asks for it." "no"
prompt_yes_no disable_soft_404_filter "Disable soft-404 filter" "Turn off baseline probing for soft-404 suppression." "no"
prompt_optional snn_state_file "SNN state file" "Optional path to persistent SNN state JSON."
prompt_optional proxies_file "Proxies file" "Optional file with one proxy URL per line."
prompt_yes_no use_tor "Use Tor settings" "If yes, the wrapper will ask for Tor proxy and control settings." "no"

case "$build_profile" in
    release|debug)
        ;;
    *)
        echo "Build profile must be either release or debug, got: $build_profile" >&2
        exit 1
        ;;
esac

tor_proxy=""
tor_control=""
tor_password=""

if [[ "$use_tor" == "yes" ]]; then
    prompt_with_default tor_proxy "Tor proxy URL" "Usually socks5h://127.0.0.1:9050." "socks5h://127.0.0.1:9050"
    prompt_with_default tor_control "Tor control address" "Usually 127.0.0.1:9051." "127.0.0.1:9051"
    prompt_optional tor_password "Tor control password" "Optional unless your torrc requires authentication."
fi

host=$(extract_host "$base_url")

if [[ -z "$host" ]]; then
    echo "Could not extract host from base URL: $base_url" >&2
    exit 1
fi

wordlist_abs=$(canonical_path "$wordlist_path")

if [[ ! -e "$wordlist_abs" ]]; then
    echo "Wordlist path does not exist: $wordlist_abs" >&2
    exit 1
fi

if [[ -n "$snn_state_file" ]]; then
    snn_state_file=$(canonical_path "$snn_state_file")
    if [[ ! -e "$snn_state_file" ]]; then
        echo "SNN state file does not exist: $snn_state_file" >&2
        exit 1
    fi
fi

if [[ -n "$proxies_file" ]]; then
    proxies_file=$(canonical_path "$proxies_file")
    if [[ ! -e "$proxies_file" ]]; then
        echo "Proxies file does not exist: $proxies_file" >&2
        exit 1
    fi
fi

remote_authorized="no"
if ! is_local_host "$host"; then
    prompt_yes_no remote_authorized "Remote target confirmation" "The target is not local. Confirm you are authorized to test it." "no"
    if [[ "$remote_authorized" != "yes" ]]; then
        echo "Aborted because remote target authorization was not confirmed." >&2
        exit 1
    fi
fi

build_profile_slug=$(sanitize_segment "$build_profile")
label_slug=$(sanitize_segment "$run_label")
host_slug=$(sanitize_segment "$host")
timestamp=$(date +%F_%H-%M-%S)
run_dir_rel="runs/${timestamp}_${label_slug}_${host_slug}"
run_dir_abs="$REPO_ROOT/$run_dir_rel"
mkdir -p "$run_dir_abs"

findings_file="$run_dir_abs/findings.txt"

if [[ "$build_profile_slug" == "release" ]]; then
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

if [[ "$remote_authorized" == "yes" ]]; then
    CMD+=(--authorized-target)
fi

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
    printf 'label=%s\n' "$run_label"
    printf 'target=%s\n' "$base_url"
    printf 'host=%s\n' "$host"
    printf 'build_profile=%s\n' "$build_profile_slug"
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
    printf 'authorized_target=%s\n' "$remote_authorized"
} > "$run_dir_abs/metadata.txt"

if [[ -f "$wordlist_abs" ]] && command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$wordlist_abs" > "$run_dir_abs/wordlist.sha256"
fi

echo
echo "Run directory: $run_dir_rel"
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

echo
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