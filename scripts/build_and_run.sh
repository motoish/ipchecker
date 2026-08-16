#!/usr/bin/env bash
set -euo pipefail

mode="${1:-run}"
app_name="ipchecker"
bundle_id="com.tanishi.ipchecker"
root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
app_bundle="$root_dir/target/release/bundle/ipchecker.app"

pkill -x "$app_name" >/dev/null 2>&1 || true
bash "$root_dir/scripts/bundle-local.sh"

open_app() {
    /usr/bin/open -n "$app_bundle"
}

wait_for_process() {
    for _ in {1..10}; do
        if pgrep -x "$app_name" >/dev/null; then
            return 0
        fi
        sleep 1
    done

    echo "failed to launch $app_name" >&2
    return 1
}

case "$mode" in
    run)
        open_app
        ;;
    --debug|debug)
        open_app
        wait_for_process
        lldb -p "$(pgrep -x "$app_name" | head -n 1)"
        ;;
    --logs|logs)
        open_app
        /usr/bin/log stream --info --style compact --predicate "process == \"$app_name\""
        ;;
    --telemetry|telemetry)
        open_app
        /usr/bin/log stream --info --style compact --predicate "subsystem == \"$bundle_id\""
        ;;
    --verify|verify)
        open_app
        wait_for_process
        ;;
    *)
        echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
        exit 2
        ;;
esac
