#!/usr/bin/env bash
set -euo pipefail

if [[ "${XDG_SESSION_TYPE:-x11}" != "x11" ]]; then
    echo "linux-render-quality currently requires an X11 session" >&2
    exit 2
fi
if [[ -z "${DISPLAY:-}" ]]; then
    echo "DISPLAY must point at the desktop X11 session" >&2
    exit 2
fi

for command in cargo xwininfo xwd; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "missing required command: $command" >&2
        exit 2
    fi
done

quality_output_dir="${1:-/tmp/zero-ui-render-quality}"
quality_target_dir="${CARGO_TARGET_DIR:-target}"
widget_binary="$quality_target_dir/debug/widgets"
mkdir -p "$quality_output_dir"
cargo build -p widgets

if xwininfo -name "zero-ui widgets" >/dev/null 2>&1; then
    echo "close the existing 'zero-ui widgets' window before running this capture" >&2
    exit 2
fi

widget_pid=""
cleanup_widget() {
    if [[ -n "$widget_pid" ]] && kill -0 "$widget_pid" 2>/dev/null; then
        kill "$widget_pid"
        wait "$widget_pid" 2>/dev/null || true
    fi
}
trap cleanup_widget EXIT INT TERM

for scale in 1.0 1.25 1.35 1.5 2.0; do
    log="$quality_output_dir/widgets-$scale.log"
    shot="$quality_output_dir/widgets-$scale.xwd"
    geometry="$quality_output_dir/widgets-$scale.geometry"

    WINIT_X11_SCALE_FACTOR="$scale" "$widget_binary" >"$log" 2>&1 &
    widget_pid=$!
    for _attempt in {1..50}; do
        if xwininfo -name "zero-ui widgets" -stats >"$geometry" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    if ! xwininfo -name "zero-ui widgets" -stats >"$geometry" 2>/dev/null; then
        echo "widgets window did not appear at scale $scale; see $log" >&2
        exit 1
    fi
    # Mapping precedes the first GPU present. Capturing immediately can read a
    # compositor pixmap that still contains pixels from its previous owner.
    sleep 1
    xwininfo -name "zero-ui widgets" -stats >"$geometry"
    xwd -silent -name "zero-ui widgets" -out "$shot"
    cleanup_widget
    widget_pid=""

    if [[ -s "$log" ]]; then
        echo "widgets emitted output at scale $scale; see $log" >&2
        exit 1
    fi
    width=$(awk '/Width:/ {print $2}' "$geometry")
    height=$(awk '/Height:/ {print $2}' "$geometry")
    printf 'scale=%s window=%sx%s screenshot=%s\n' "$scale" "$width" "$height" "$shot"
done

echo "Captured native-pixel XWD files in $quality_output_dir"
