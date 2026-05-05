#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd -P)

if [[ -t 1 && -z "${NO_COLOR:-}" && "${TERM:-}" != "dumb" ]]; then
    RESET=$'\033[0m'
    BOLD=$'\033[1m'
    DIM=$'\033[2m'
    RED=$'\033[31m'
    GREEN=$'\033[32m'
    YELLOW=$'\033[33m'
    BLUE=$'\033[34m'
    MAGENTA=$'\033[35m'
    CYAN=$'\033[36m'
else
    RESET=""
    BOLD=""
    DIM=""
    RED=""
    GREEN=""
    YELLOW=""
    BLUE=""
    MAGENTA=""
    CYAN=""
fi

print_help() {
    cat <<'EOF'
Sigma Morpho interactive runner.

What it does:
- walks through the most important launch parameters step by step
- shows short inline hints for every choice
- keeps archived logs in runs/YYYY-MM-DD_HH-MM-SS_HOST_OR_IP_PORT/
- writes command.txt, rerun.sh, metadata.txt, build logs, stdout, stderr, time.txt, and findings.txt

Usage:
  scripts/run_sigma_session.sh
  scripts/run_sigma_session.sh --help

Optional environment defaults:
  SIGMA_BASE_URL=http://127.0.0.1:8080
  SIGMA_DICT_DIR=dict
  SIGMA_WORDLIST=
  SIGMA_WORKERS=16
  SIGMA_ROUNDS=1
  SIGMA_RECURSION_DEPTH=0
  SIGMA_SCAN_MODE=path
  SIGMA_SUBDOMAIN_SEARCH=no
  SIGMA_VHOST_TEMPLATE=
  SIGMA_RATE=
  SIGMA_TIMEOUT_MS=5000
  SIGMA_INITIAL_DELAY_MS=50
  SIGMA_MIN_DELAY_MS=25
  SIGMA_MAX_DELAY_MS=5000
  SIGMA_LATENCY_THRESHOLD_MS=800
  SIGMA_INTERESTING_STATUSES=200,204,301,302,307,308,401,403,405,500
  SIGMA_MIN_BODY_BYTES=0
  SIGMA_MAX_BODY_BYTES=
  SIGMA_FILTER_WORDS=
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

These environment variables only prefill defaults. The script still asks interactively.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    print_help
    exit 0
fi

cd "$REPO_ROOT"

say_header() {
    printf '\n%s%s%s\n' "${BOLD}${CYAN}" "$1" "$RESET"
}

say_info() {
    printf '%s[INFO]%s %s\n' "$BLUE" "$RESET" "$1"
}

say_warn() {
    printf '%s[WARN]%s %s\n' "$YELLOW" "$RESET" "$1"
}

say_error() {
    printf '%s[ERROR]%s %s\n' "$RED" "$RESET" "$1" >&2
}

say_success() {
    printf '%s[OK]%s %s\n' "$GREEN" "$RESET" "$1"
}

show_banner() {
    printf '%s\n' "${BOLD}${MAGENTA}========================================${RESET}"
    printf '%s\n' "${BOLD}${MAGENTA} Sigma Morpho Interactive Runner${RESET}"
    printf '%s\n' "${BOLD}${MAGENTA}========================================${RESET}"
    printf '%s\n' "${DIM}Press Enter to accept the suggested default in brackets.${RESET}"
    printf '%s\n' "${DIM}Every run will be archived automatically under runs/.${RESET}"
}

read_value() {
    local __resultvar=$1
    local input_value

    if ! IFS= read -r input_value; then
        printf '\n'
        say_warn "Input cancelled."
        exit 130
    fi

    printf -v "$__resultvar" '%s' "$input_value"
}

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

is_local_host() {
    local host=$1
    [[ "$host" == "localhost" || "$host" == "127.0.0.1" || "$host" == "::1" || "$host" == *.localhost ]]
}

as_yes_no() {
    local value=${1,,}

    case "$value" in
        y|yes|1|true|on) printf 'yes\n' ;;
        n|no|0|false|off|'') printf 'no\n' ;;
        *)
            say_error "Expected yes/no style value, got: $1"
            exit 1
            ;;
    esac
}

validate_status_list() {
    local raw=$1
    local part trimmed

    for part in ${raw//,/ }; do
        trimmed=${part// /}
        if [[ -z "$trimmed" ]]; then
            continue
        fi
        if [[ ! "$trimmed" =~ ^[0-9]+$ ]]; then
            return 1
        fi
        if (( trimmed < 100 || trimmed > 599 )); then
            return 1
        fi
    done

    return 0
}

validate_uint_list() {
    local raw=$1
    local part trimmed

    if [[ -z "$raw" ]]; then
        return 0
    fi

    for part in ${raw//,/ }; do
        trimmed=${part// /}
        if [[ -z "$trimmed" ]]; then
            continue
        fi
        if [[ ! "$trimmed" =~ ^[0-9]+$ ]]; then
            return 1
        fi
    done

    return 0
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

prompt_text() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local raw_value final_value

    while true; do
        printf '\n%s%s%s\n' "${BOLD}${MAGENTA}" "$label" "$RESET"
        printf '%s%s%s\n' "$DIM" "$hint" "$RESET"
        printf '%s[%s]%s ' "$CYAN" "$default_value" "$RESET"
        read_value raw_value
        final_value=${raw_value:-$default_value}

        if [[ -n "$final_value" ]]; then
            printf -v "$__resultvar" '%s' "$final_value"
            return
        fi

        say_warn "Value cannot be empty."
    done
}

prompt_optional_text() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local raw_value display_value final_value

    if [[ -n "$default_value" ]]; then
        display_value=$default_value
    else
        display_value='leave empty to skip'
    fi

    printf '\n%s%s%s\n' "${BOLD}${MAGENTA}" "$label" "$RESET"
    printf '%s%s%s\n' "$DIM" "$hint" "$RESET"
    printf '%s[%s]%s ' "$CYAN" "$display_value" "$RESET"
    read_value raw_value
    final_value=${raw_value:-$default_value}
    printf -v "$__resultvar" '%s' "$final_value"
}

prompt_uint() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local min_value=$5
    local max_value=$6
    local candidate

    while true; do
        prompt_text candidate "$label" "$hint" "$default_value"
        if [[ ! "$candidate" =~ ^[0-9]+$ ]]; then
            say_warn "Enter a non-negative integer."
            continue
        fi
        if (( candidate < min_value || candidate > max_value )); then
            say_warn "Value must be between $min_value and $max_value."
            continue
        fi
        printf -v "$__resultvar" '%s' "$candidate"
        return
    done
}

prompt_optional_uint() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local min_value=$5
    local max_value=$6
    local candidate

    while true; do
        prompt_optional_text candidate "$label" "$hint" "$default_value"
        if [[ -z "$candidate" ]]; then
            printf -v "$__resultvar" '%s' ""
            return
        fi
        if [[ ! "$candidate" =~ ^[0-9]+$ ]]; then
            say_warn "Enter a non-negative integer or leave it empty."
            continue
        fi
        if (( candidate < min_value || candidate > max_value )); then
            say_warn "Value must be between $min_value and $max_value."
            continue
        fi
        printf -v "$__resultvar" '%s' "$candidate"
        return
    done
}

prompt_choice() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    shift 4
    local options=("$@")
    local raw_value normalized selected_value index

    while true; do
        printf '\n%s%s%s\n' "${BOLD}${MAGENTA}" "$label" "$RESET"
        printf '%s%s%s\n' "$DIM" "$hint" "$RESET"
        index=1
        for selected_value in "${options[@]}"; do
            if [[ "$selected_value" == "$default_value" ]]; then
                printf '  %s%d)%s %s %s(default)%s\n' "$CYAN" "$index" "$RESET" "$selected_value" "$DIM" "$RESET"
            else
                printf '  %s%d)%s %s\n' "$CYAN" "$index" "$RESET" "$selected_value"
            fi
            index=$((index + 1))
        done
        printf '%s[%s]%s ' "$CYAN" "$default_value" "$RESET"
        read_value raw_value
        normalized=${raw_value:-$default_value}

        if [[ "$normalized" =~ ^[0-9]+$ ]]; then
            if (( normalized >= 1 && normalized <= ${#options[@]} )); then
                printf -v "$__resultvar" '%s' "${options[$((normalized - 1))]}"
                return
            fi
        else
            for selected_value in "${options[@]}"; do
                if [[ "$selected_value" == "$normalized" ]]; then
                    printf -v "$__resultvar" '%s' "$selected_value"
                    return
                fi
            done
        fi

        say_warn "Choose one of the listed values or numbers."
    done
}

prompt_yes_no() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local raw_value normalized

    while true; do
        printf '\n%s%s%s\n' "${BOLD}${MAGENTA}" "$label" "$RESET"
        printf '%s%s%s\n' "$DIM" "$hint" "$RESET"
        printf '%s[%s]%s ' "$CYAN" "$default_value" "$RESET"
        read_value raw_value
        normalized=$(as_yes_no "${raw_value:-$default_value}")
        printf -v "$__resultvar" '%s' "$normalized"
        return
    done
}

prompt_base_url() {
    local __resultvar=$1
    local default_value=$2
    local candidate host port

    while true; do
        prompt_text candidate "Base URL" "Full target URL. Example: http://127.0.0.1:8080 or https://example.org." "$default_value"

        if [[ "$candidate" != *://* ]]; then
            say_warn "The URL must include a scheme such as http:// or https://."
            continue
        fi

        host=$(extract_host "$candidate")
        port=$(extract_port "$candidate")
        if [[ -z "$host" || -z "$port" ]]; then
            say_warn "Could not parse host and port from the URL."
            continue
        fi

        say_info "Resolved target host: $host"
        say_info "Resolved target port: $port"
        printf -v "$__resultvar" '%s' "$candidate"
        return
    done
}

prompt_existing_path() {
    local __resultvar=$1
    local label=$2
    local hint=$3
    local default_value=$4
    local allow_empty=$5
    local raw_value resolved_value display_default

    while true; do
        if [[ -n "$default_value" ]]; then
            display_default=$default_value
        else
            display_default='leave empty to skip'
        fi

        printf '\n%s%s%s\n' "${BOLD}${MAGENTA}" "$label" "$RESET"
        printf '%s%s%s\n' "$DIM" "$hint" "$RESET"
        printf '%s[%s]%s ' "$CYAN" "$display_default" "$RESET"
        read_value raw_value
        raw_value=${raw_value:-$default_value}

        if [[ -z "$raw_value" && "$allow_empty" == "yes" ]]; then
            printf -v "$__resultvar" '%s' ""
            return
        fi

        if [[ -z "$raw_value" ]]; then
            say_warn "Value cannot be empty."
            continue
        fi

        if ! resolved_value=$(canonical_path "$raw_value"); then
            say_warn "Could not resolve path: $raw_value"
            continue
        fi

        if [[ ! -e "$resolved_value" ]]; then
            say_warn "Path does not exist: $resolved_value"
            continue
        fi

        printf -v "$__resultvar" '%s' "$resolved_value"
        return
    done
}

prompt_wordlist_choice() {
    local __resultvar=$1
    local dict_dir=$2
    local default_value=$3
    local dict_abs default_abs custom_default raw_value selected_index custom_index index
    local file rel_name line_count display_default
    local -a wordlist_files=()

    if ! dict_abs=$(canonical_path "$dict_dir"); then
        say_warn "Could not resolve dictionary directory: $dict_dir"
        prompt_existing_path "$__resultvar" "Wordlist path" "File or directory. Relative paths are resolved from the repository root." "$default_value" no
        return
    fi

    if [[ ! -d "$dict_abs" ]]; then
        say_warn "Dictionary directory does not exist: $dict_abs"
        prompt_existing_path "$__resultvar" "Wordlist path" "File or directory. Relative paths are resolved from the repository root." "$default_value" no
        return
    fi

    while IFS= read -r -d '' file; do
        wordlist_files+=("$file")
    done < <(find "$dict_abs" -maxdepth 1 -type f -print0 | sort -z)

    if [[ ${#wordlist_files[@]} -eq 0 ]]; then
        say_warn "No wordlist files found in: $dict_abs"
        prompt_existing_path "$__resultvar" "Wordlist path" "File or directory. Relative paths are resolved from the repository root." "$default_value" no
        return
    fi

    default_abs=""
    if [[ -n "$default_value" ]]; then
        default_abs=$(canonical_path "$default_value" 2>/dev/null || true)
    fi

    selected_index=1
    custom_default=${default_value:-}
    for index in "${!wordlist_files[@]}"; do
        if [[ -n "$default_abs" && "${wordlist_files[$index]}" == "$default_abs" ]]; then
            selected_index=$((index + 1))
            custom_default=""
            break
        fi
    done
    custom_index=$((${#wordlist_files[@]} + 1))
    display_default=$selected_index
    if [[ -n "$custom_default" ]]; then
        display_default=$custom_index
    fi

    while true; do
        printf '\n%s%s%s\n' "${BOLD}${MAGENTA}" "Wordlist" "$RESET"
        printf '%s%s%s\n' "$DIM" "Choose a wordlist discovered in $dict_abs, or choose custom path." "$RESET"

        for index in "${!wordlist_files[@]}"; do
            file=${wordlist_files[$index]}
            rel_name=${file#"$REPO_ROOT/"}
            line_count=$(wc -l < "$file" 2>/dev/null | tr -d '[:space:]')
            if [[ -z "$line_count" ]]; then
                line_count="?"
            fi

            if (( index + 1 == selected_index )) && [[ -z "$custom_default" ]]; then
                printf '  %s%d)%s %s %s(%s lines, default)%s\n' "$CYAN" "$((index + 1))" "$RESET" "$rel_name" "$DIM" "$line_count" "$RESET"
            else
                printf '  %s%d)%s %s %s(%s lines)%s\n' "$CYAN" "$((index + 1))" "$RESET" "$rel_name" "$DIM" "$line_count" "$RESET"
            fi
        done

        if [[ -n "$custom_default" ]]; then
            printf '  %s%d)%s custom path %s(default: %s)%s\n' "$CYAN" "$custom_index" "$RESET" "$DIM" "$custom_default" "$RESET"
        else
            printf '  %s%d)%s custom path\n' "$CYAN" "$custom_index" "$RESET"
        fi

        printf '%s[%s]%s ' "$CYAN" "$display_default" "$RESET"
        read_value raw_value
        raw_value=${raw_value:-$display_default}

        if [[ ! "$raw_value" =~ ^[0-9]+$ ]]; then
            say_warn "Choose a number from the list."
            continue
        fi

        if (( raw_value >= 1 && raw_value <= ${#wordlist_files[@]} )); then
            printf -v "$__resultvar" '%s' "${wordlist_files[$((raw_value - 1))]}"
            return
        fi

        if (( raw_value == custom_index )); then
            prompt_existing_path "$__resultvar" "Custom wordlist path" "File or directory. Relative paths are resolved from the repository root." "$custom_default" no
            return
        fi

        say_warn "Choose a number between 1 and $custom_index."
    done
}

if ! command -v cargo >/dev/null 2>&1; then
    say_error "cargo is required but was not found in PATH."
    exit 1
fi

show_banner

base_url_default=${SIGMA_BASE_URL:-http://127.0.0.1:8080}
dict_dir_default=${SIGMA_DICT_DIR:-dict}
wordlist_path_default=${SIGMA_WORDLIST:-}
workers_default=${SIGMA_WORKERS:-16}
rounds_default=${SIGMA_ROUNDS:-1}
recursion_depth_default=${SIGMA_RECURSION_DEPTH:-0}
scan_mode_default=${SIGMA_SCAN_MODE:-path}
subdomain_search_default=$(as_yes_no "${SIGMA_SUBDOMAIN_SEARCH:-no}")
if [[ "$subdomain_search_default" == "yes" ]]; then
    scan_mode_default=vhost
fi
if [[ -z "$wordlist_path_default" && "$scan_mode_default" == "vhost" && -f "$REPO_ROOT/$dict_dir_default/bitquark-subdomains-top100000.txt" ]]; then
    wordlist_path_default="$dict_dir_default/bitquark-subdomains-top100000.txt"
fi
vhost_template_default=${SIGMA_VHOST_TEMPLATE:-}
rate_default=${SIGMA_RATE:-}
timeout_ms_default=${SIGMA_TIMEOUT_MS:-5000}
initial_delay_ms_default=${SIGMA_INITIAL_DELAY_MS:-50}
min_delay_ms_default=${SIGMA_MIN_DELAY_MS:-25}
max_delay_ms_default=${SIGMA_MAX_DELAY_MS:-5000}
latency_threshold_ms_default=${SIGMA_LATENCY_THRESHOLD_MS:-800}
interesting_statuses_default=${SIGMA_INTERESTING_STATUSES:-200,204,301,302,307,308,401,403,405,500}
min_body_bytes_default=${SIGMA_MIN_BODY_BYTES:-0}
max_body_bytes_default=${SIGMA_MAX_BODY_BYTES:-}
filter_words_default=${SIGMA_FILTER_WORDS:-}
speed_mode_default=${SIGMA_SPEED_MODE:-balanced}
client_profile_default=${SIGMA_CLIENT_PROFILE:-research-default}
build_profile_default=${SIGMA_BUILD_PROFILE:-release}
simulation_mode_default=$(as_yes_no "${SIGMA_SIMULATION_MODE:-no}")
compare_profiles_live_default=$(as_yes_no "${SIGMA_COMPARE_PROFILES_LIVE:-no}")
rebuild_client_on_advisory_default=$(as_yes_no "${SIGMA_REBUILD_CLIENT_ON_ADVISORY:-no}")
disable_soft_404_filter_default=$(as_yes_no "${SIGMA_DISABLE_SOFT_404_FILTER:-no}")
authorized_target_default=$(as_yes_no "${SIGMA_AUTHORIZED_TARGET:-no}")
snn_state_file_default=${SIGMA_SNN_STATE_FILE:-}
proxies_file_default=${SIGMA_PROXIES_FILE:-}
tor_proxy_default=${SIGMA_TOR_PROXY:-socks5h://127.0.0.1:9050}
tor_control_default=${SIGMA_TOR_CONTROL:-127.0.0.1:9051}
tor_password_default=${SIGMA_TOR_PASSWORD:-}

say_header "1. Target"
prompt_base_url base_url "$base_url_default"
host=$(extract_host "$base_url")
port=$(extract_port "$base_url")

authorized_target=no
if ! is_local_host "$host"; then
    say_warn "The selected target is not local."
    prompt_yes_no authorized_target "Remote target authorization" "Confirm that you own the target or have explicit permission to test it." "$authorized_target_default"
    if [[ "$authorized_target" != "yes" ]]; then
        say_error "Run cancelled because authorization was not confirmed."
        exit 1
    fi
fi

prompt_wordlist_choice wordlist_abs "$dict_dir_default" "$wordlist_path_default"

prompt_choice build_profile "Build profile" "release is the normal choice. debug is faster to rebuild but slower to run." "$build_profile_default" release debug

say_header "2. Workload"
prompt_choice scan_mode "Scan mode" "path fuzzes URL paths. vhost/subdomain sends candidates through the Host header, like ffuf -H 'Host: FUZZ.domain'." "$scan_mode_default" path vhost
vhost_template=""
if [[ "$scan_mode" == "vhost" ]]; then
    if [[ -z "$vhost_template_default" ]]; then
        vhost_template_default="FUZZ.$host"
    fi
    while true; do
        prompt_text vhost_template "VHost template" "Host header template. It must include FUZZ, for example FUZZ.example.htb." "$vhost_template_default"
        if [[ "$vhost_template" == *FUZZ* ]]; then
            break
        fi
        say_warn "VHost template must contain FUZZ."
    done
fi

prompt_uint workers "Workers" "Parallel request workers. Higher values increase pressure and can trigger rate limits sooner." "$workers_default" 1 100000
prompt_uint rounds "Rounds" "How many times to replay the wordlist." "$rounds_default" 1 100000
if [[ "$scan_mode" == "vhost" ]]; then
    recursion_depth=0
    say_info "Recursion depth is fixed to 0 for vhost/subdomain scans."
else
    prompt_uint recursion_depth "Recursion depth" "0 disables recursion. Use higher values only when you want directory expansion." "$recursion_depth_default" 0 255
    if (( recursion_depth > 0 )); then
        say_warn "Recursion is not ffuf-style single-pass scanning. Every discovered directory can add the whole wordlist again."
        prompt_yes_no confirm_recursion "Confirm recursive expansion" "Choose no for the same behavior as ffuf -u http://target/FUZZ." no
        if [[ "$confirm_recursion" != "yes" ]]; then
            recursion_depth=0
            say_info "Recursion depth reset to 0 for ffuf-style path fuzzing."
        fi
    fi
fi

say_header "3. Timing"
prompt_optional_uint rate_limit "Rate limit req/s" "Optional global requests-per-second cap. Use 1200 for your ffuf-style target speed, or leave empty for adaptive delay mode." "$rate_default" 1 10000000
prompt_uint timeout_ms "Timeout ms" "Per-request timeout in milliseconds." "$timeout_ms_default" 1 86400000
prompt_uint initial_delay_ms "Initial delay ms" "Starting adaptive delay before the RSNN changes pacing." "$initial_delay_ms_default" 0 86400000

while true; do
    prompt_uint min_delay_ms "Min delay ms" "Lower bound for adaptive pacing." "$min_delay_ms_default" 0 86400000
    prompt_uint max_delay_ms "Max delay ms" "Upper bound for adaptive pacing." "$max_delay_ms_default" 0 86400000
    if (( min_delay_ms <= max_delay_ms )); then
        break
    fi
    say_warn "Min delay cannot be greater than max delay. Please enter both values again."
done

prompt_uint latency_threshold_ms "Latency threshold ms" "Response times above this value are treated as pressure by the adaptive logic." "$latency_threshold_ms_default" 1 86400000

say_header "4. Findings"
while true; do
    prompt_text interesting_statuses "Interesting statuses" "Comma-separated HTTP status codes that should be written into findings.txt." "$interesting_statuses_default"
    if validate_status_list "$interesting_statuses"; then
        break
    fi
    say_warn "Enter a comma-separated list of valid HTTP status codes, for example 200,403,429."
done

while true; do
    prompt_uint min_body_bytes "Min body bytes" "Ignore findings smaller than this size." "$min_body_bytes_default" 0 1000000000
    prompt_optional_uint max_body_bytes "Max body bytes" "Optional upper body-size filter. Leave empty to disable it." "$max_body_bytes_default" 0 1000000000
    if [[ -z "$max_body_bytes" || $max_body_bytes -ge $min_body_bytes ]]; then
        break
    fi
    say_warn "Max body bytes cannot be smaller than min body bytes. Please enter both again."
done

while true; do
    prompt_optional_text filter_words "Filter words" "Optional comma-separated word counts to suppress, like ffuf -fw 4. Leave empty to disable." "$filter_words_default"
    if validate_uint_list "$filter_words"; then
        break
    fi
    say_warn "Enter comma-separated non-negative integers, for example 4 or 4,8."
done

prompt_yes_no disable_soft_404_filter "Disable soft-404 filter" "Choose yes only if you want every 404-like body reported without baseline suppression." "$disable_soft_404_filter_default"

say_header "5. Runtime Profile"
prompt_choice speed_mode "Speed mode" "safe is slowest, balanced is default, fast and aggressive reduce delay caps." "$speed_mode_default" safe balanced fast aggressive
prompt_choice client_profile "Client profile" "Choose the request identity style used by the HTTP client." "$client_profile_default" research-default browser-desktop mobile-safari api-diagnostic
prompt_yes_no simulation_mode "Simulation mode" "Use the safe simulator instead of live network effects where supported." "$simulation_mode_default"
prompt_yes_no compare_profiles_live "Compare profiles live" "Run the same workload across all fixed client profiles and compare the results." "$compare_profiles_live_default"
prompt_yes_no rebuild_client_on_advisory "Rebuild client on advisory" "Hot-swap the reqwest client when the neuro layer asks for a refresh." "$rebuild_client_on_advisory_default"

say_header "6. Advanced Options"
prompt_yes_no configure_advanced "Configure advanced files and proxy options" "Choose yes if you want to set SNN state, proxy files, or Tor settings." no

snn_state_file=""
proxies_file=""
tor_proxy=""
tor_control=""
tor_password=""

if [[ "$configure_advanced" == "yes" ]]; then
    prompt_existing_path snn_state_file "SNN state file" "Optional JSON state file. Leave empty to skip persistence." "$snn_state_file_default" yes
    prompt_existing_path proxies_file "Proxies file" "Optional file with one proxy URL per line. Leave empty to skip." "$proxies_file_default" yes
    prompt_yes_no use_tor "Configure Tor options" "Choose yes if you want to pass Tor proxy and Tor control settings." no
    if [[ "$use_tor" == "yes" ]]; then
        prompt_text tor_proxy "Tor proxy URL" "Typical value: socks5h://127.0.0.1:9050" "$tor_proxy_default"
        prompt_text tor_control "Tor control address" "Typical value: 127.0.0.1:9051" "$tor_control_default"
        prompt_optional_text tor_password "Tor control password" "Leave empty if your Tor control port does not require a password." "$tor_password_default"
    fi
fi

say_header "7. Review"
printf '  %sBase URL:%s %s\n' "$BOLD" "$RESET" "$base_url"
printf '  %sHost:%s %s\n' "$BOLD" "$RESET" "$host"
printf '  %sPort:%s %s\n' "$BOLD" "$RESET" "$port"
printf '  %sWordlist:%s %s\n' "$BOLD" "$RESET" "$wordlist_abs"
printf '  %sBuild profile:%s %s\n' "$BOLD" "$RESET" "$build_profile"
printf '  %sScan mode:%s %s\n' "$BOLD" "$RESET" "$scan_mode"
printf '  %sVHost template:%s %s\n' "$BOLD" "$RESET" "${vhost_template:-none}"
printf '  %sWorkers / rounds:%s %s / %s\n' "$BOLD" "$RESET" "$workers" "$rounds"
printf '  %sRecursion depth:%s %s\n' "$BOLD" "$RESET" "$recursion_depth"
printf '  %sRate limit:%s %s\n' "$BOLD" "$RESET" "${rate_limit:-adaptive}"
printf '  %sTimeout ms:%s %s\n' "$BOLD" "$RESET" "$timeout_ms"
printf '  %sDelay ms:%s initial=%s min=%s max=%s\n' "$BOLD" "$RESET" "$initial_delay_ms" "$min_delay_ms" "$max_delay_ms"
printf '  %sLatency threshold ms:%s %s\n' "$BOLD" "$RESET" "$latency_threshold_ms"
printf '  %sInteresting statuses:%s %s\n' "$BOLD" "$RESET" "$interesting_statuses"
printf '  %sBody size filter:%s min=%s max=%s\n' "$BOLD" "$RESET" "$min_body_bytes" "${max_body_bytes:-none}"
printf '  %sWord count filter:%s %s\n' "$BOLD" "$RESET" "${filter_words:-none}"
printf '  %sSpeed mode:%s %s\n' "$BOLD" "$RESET" "$speed_mode"
printf '  %sClient profile:%s %s\n' "$BOLD" "$RESET" "$client_profile"
printf '  %sSimulation mode:%s %s\n' "$BOLD" "$RESET" "$simulation_mode"
printf '  %sCompare profiles live:%s %s\n' "$BOLD" "$RESET" "$compare_profiles_live"
printf '  %sRebuild client on advisory:%s %s\n' "$BOLD" "$RESET" "$rebuild_client_on_advisory"
printf '  %sDisable soft-404 filter:%s %s\n' "$BOLD" "$RESET" "$disable_soft_404_filter"
printf '  %sSNN state file:%s %s\n' "$BOLD" "$RESET" "${snn_state_file:-none}"
printf '  %sProxies file:%s %s\n' "$BOLD" "$RESET" "${proxies_file:-none}"
printf '  %sTor proxy:%s %s\n' "$BOLD" "$RESET" "${tor_proxy:-none}"
printf '  %sTor control:%s %s\n' "$BOLD" "$RESET" "${tor_control:-none}"
printf '  %sAuthorized target:%s %s\n' "$BOLD" "$RESET" "$authorized_target"
printf '  %sRun directory pattern:%s runs/<timestamp>_%s_%s/\n' "$BOLD" "$RESET" "$(sanitize_segment "$host")" "$(sanitize_segment "$port")"

prompt_yes_no proceed_run "Start run now" "Choose yes to build the binary and execute Sigma Morpho with the selected settings." yes

if [[ "$proceed_run" != "yes" ]]; then
    say_warn "Run cancelled by user before execution."
    exit 0
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
    --scan-mode "$scan_mode"
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

append_optional_arg --vhost-template "$vhost_template"
append_optional_arg --rate "$rate_limit"
append_optional_arg --max-body-bytes "$max_body_bytes"
append_optional_arg --filter-words "$filter_words"
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
    printf 'scan_mode=%s\n' "$scan_mode"
    printf 'vhost_template=%s\n' "${vhost_template:-none}"
    printf 'rate_limit=%s\n' "${rate_limit:-adaptive}"
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
    printf 'filter_words=%s\n' "${filter_words:-none}"
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

say_info "Run directory: $run_dir_rel"
say_info "Target: $base_url"
say_info "Building Sigma Morpho. Build logs will be stored in the run directory."

set +e
cargo "${build_args[@]}" > "$run_dir_abs/build.stdout.log" 2> "$run_dir_abs/build.stderr.log"
build_exit_code=$?
set -e

printf 'build_exit_code=%s\n' "$build_exit_code" >> "$run_dir_abs/metadata.txt"

if [[ "$build_exit_code" -ne 0 ]]; then
    printf 'ended_at=%s\n' "$(date --iso-8601=seconds)" >> "$run_dir_abs/metadata.txt"
    printf 'run_exit_code=%s\n' "$build_exit_code" >> "$run_dir_abs/metadata.txt"
    say_error "Build failed. See $run_dir_rel/build.stderr.log"
    exit "$build_exit_code"
fi

say_info "Executing Sigma Morpho. Stdout, stderr, timing, and findings will be archived automatically."

set +e
if [[ -x /usr/bin/time ]]; then
    /usr/bin/time -p -o "$run_dir_abs/time.txt" "${CMD[@]}" \
        > >(tee "$run_dir_abs/sigma.stdout.log") \
        2> >(tee "$run_dir_abs/sigma.stderr.log" >&2)
else
    TIMEFORMAT=$'real=%3R\nuser=%3U\nsys=%3S'
    { time "${CMD[@]}" > "$run_dir_abs/sigma.stdout.log" 2> "$run_dir_abs/sigma.stderr.log"; } 2> "$run_dir_abs/time.txt"
    cat "$run_dir_abs/sigma.stdout.log"
    cat "$run_dir_abs/sigma.stderr.log" >&2
fi
run_exit_code=$?
set -e

printf 'ended_at=%s\n' "$(date --iso-8601=seconds)" >> "$run_dir_abs/metadata.txt"
printf 'run_exit_code=%s\n' "$run_exit_code" >> "$run_dir_abs/metadata.txt"

if [[ "$run_exit_code" -eq 0 ]]; then
    say_success "Run finished successfully."
else
    say_warn "Run finished with a non-zero exit code: $run_exit_code"
fi

printf '%sArtifacts:%s\n' "$BOLD" "$RESET"
printf '  %s/command.txt\n' "$run_dir_rel"
printf '  %s/rerun.sh\n' "$run_dir_rel"
printf '  %s/metadata.txt\n' "$run_dir_rel"
printf '  %s/build.stdout.log\n' "$run_dir_rel"
printf '  %s/build.stderr.log\n' "$run_dir_rel"
printf '  %s/sigma.stdout.log\n' "$run_dir_rel"
printf '  %s/sigma.stderr.log\n' "$run_dir_rel"
printf '  %s/time.txt\n' "$run_dir_rel"
printf '  %s/findings.txt\n' "$run_dir_rel"

exit "$run_exit_code"
