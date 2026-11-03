#!/usr/bin/env bash
# scripts/crash-test.sh — proves why invariant 3 (every segment is a fragmented MP4) exists:
# a recording killed with SIGKILL mid-way must stay readable; a classic MP4 does not.
#
#   bash scripts/crash-test.sh         # from the repo root; ~25 s wall-clock
#
# Two recordings with the app's exact argv (synthetic lavfi inputs at -re, -t 30), each killed with
# `kill -9` after KILL_AFTER seconds (default 10):
#   tests/out/crash-frag.mp4      -movflags +frag_keyframe+empty_moov+default_base_moof (the app)
#   tests/out/crash-control.mp4   -movflags +faststart (moov written at the end = control)
# PASS when ffprobe reads >= 8 s from the fragmented file AND the control file does not probe
# (or reports < 1 s). Prints both measurements and a final PASS or FAIL line; exits 1 on FAIL.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT_DIR="tests/out"
mkdir -p "$OUT_DIR"

FFMPEG="${FFMPEG:-$(command -v ffmpeg || true)}"
FFPROBE="${FFPROBE:-$(command -v ffprobe || true)}"
if [ -z "$FFMPEG" ] || [ ! -x "$FFMPEG" ]; then FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"; fi
if [ -z "$FFPROBE" ] || [ ! -x "$FFPROBE" ]; then FFPROBE="src-tauri/binaries/ffprobe-aarch64-apple-darwin"; fi
echo "ffmpeg : $FFMPEG ($("$FFMPEG" -version 2>/dev/null | head -1))"

RUN_SECONDS="${RUN_SECONDS:-30}"
KILL_AFTER="${KILL_AFTER:-10}"
MIN_SURVIVING_SECONDS=8

SCREEN_GRAPH="color=c=blue:s=1920x1080:r=30"
CAMERA_GRAPH="color=c=red:s=640x480:r=30"
MIC_GRAPH="sine=frequency=440:sample_rate=48000"
FILTER="[0:v]scale=1920:1080:flags=bicubic[scr];[1:v]crop=ih:ih,hflip,scale=236:236:flags=bicubic,format=rgba[pip_c];nullsrc=s=236x236:r=30,format=gray,geq=lum='clip((29-hypot(max(abs(X-118+0.5)-89,0),max(abs(Y-118+0.5)-89,0)))*255+127.5,0,255)',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)[pip_m];[pip_c][pip_m]alphamerge[pip];[scr][pip]overlay=1660:820:format=auto[vout];[2:a]aresample=48000:async=1[aout]"
ENCODE=(-c:v hevc_videotoolbox -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main
        -b:v 793k -maxrate 1190k -bufsize 1586k -g 60 -pix_fmt yuv420p -r 30 -fps_mode cfr
        -c:a aac -b:a 96k -ar 48000 -ac 2)
FRAG_MOVFLAGS=(-movflags +frag_keyframe+empty_moov+default_base_moof -frag_duration 2000000)
CONTROL_MOVFLAGS=(-movflags +faststart)

CHILD=""
trap '[ -n "$CHILD" ] && kill -9 "$CHILD" 2>/dev/null || true' EXIT

record_and_kill() { # out movflags...
  local out="$1" t0 pid
  shift
  rm -f "$out"
  t0=$SECONDS
  "$FFMPEG" -hide_banner -loglevel warning \
    -thread_queue_size 1024 -re -f lavfi -i "$SCREEN_GRAPH" \
    -thread_queue_size 1024 -re -f lavfi -i "$CAMERA_GRAPH" \
    -thread_queue_size 1024 -re -f lavfi -i "$MIC_GRAPH" \
    -filter_complex "$FILTER" -map "[vout]" -map "[aout]" \
    "${ENCODE[@]}" "$@" \
    -progress pipe:1 -nostats -t "$RUN_SECONDS" -y "$out" \
    >"$out.progress" 2>"$out.stderr" &
  pid=$!
  CHILD="$pid"
  sleep "$KILL_AFTER"
  kill -9 "$pid"
  wait "$pid" 2>/dev/null || true
  CHILD=""
  echo "  pid $pid SIGKILLed after $((SECONDS - t0)) s wall-clock; last progress out_time=$(grep '^out_time=' "$out.progress" | tail -1 | cut -d= -f2); file $(wc -c <"$out" | tr -d ' ') bytes"
}

probe_duration() { # file -> seconds, or "unreadable" (ffprobe failed / no duration)
  local d
  if d=$("$FFPROBE" -v error -show_entries format=duration -of default=nw=1:nk=1 "$1" 2>"$1.probe.stderr") \
     && [ -n "$d" ] && [ "$d" != "N/A" ]; then
    echo "$d"
  else
    echo "unreadable"
  fi
}

FRAG="$OUT_DIR/crash-frag.mp4"
CONTROL="$OUT_DIR/crash-control.mp4"

echo "== fragmented (app): ${FRAG_MOVFLAGS[*]}, -t $RUN_SECONDS, kill -9 after ${KILL_AFTER}s → $FRAG"
record_and_kill "$FRAG" "${FRAG_MOVFLAGS[@]}"
frag_dur=$(probe_duration "$FRAG")
frag_streams=$("$FFPROBE" -v error -show_entries stream=codec_type -of csv=p=0 "$FRAG" 2>/dev/null | tr '\n' ' ' || true)
echo "  ffprobe duration: $frag_dur s; streams: ${frag_streams:-none}; probe stderr: $(tr '\n' '|' <"$FRAG.probe.stderr")"

echo "== control (classic MP4): ${CONTROL_MOVFLAGS[*]}, -t $RUN_SECONDS, kill -9 after ${KILL_AFTER}s → $CONTROL"
record_and_kill "$CONTROL" "${CONTROL_MOVFLAGS[@]}"
control_dur=$(probe_duration "$CONTROL")
echo "  ffprobe duration: $control_dur; probe stderr: $(tr '\n' '|' <"$CONTROL.probe.stderr")"

frag_ok=0
if [ "$frag_dur" != unreadable ] && awk -v d="$frag_dur" -v m="$MIN_SURVIVING_SECONDS" 'BEGIN { exit !(d + 0 >= m + 0) }'; then
  frag_ok=1
fi
control_ok=0
if [ "$control_dur" = unreadable ]; then
  control_ok=1
elif awk -v d="$control_dur" 'BEGIN { exit !(d + 0 < 1) }'; then
  control_ok=1
fi

echo "---"
if [ "$frag_ok" = 1 ]; then
  echo "PASS  fragmented survives SIGKILL: duration ${frag_dur}s >= ${MIN_SURVIVING_SECONDS}s"
else
  echo "FAIL  fragmented survives SIGKILL: duration ${frag_dur} (need >= ${MIN_SURVIVING_SECONDS}s)"
fi
if [ "$control_ok" = 1 ]; then
  echo "PASS  control does not survive SIGKILL: ffprobe duration ${control_dur} (expected unreadable or < 1 s)"
else
  echo "FAIL  control unexpectedly readable: ffprobe duration ${control_dur}s"
fi
if [ "$frag_ok" = 1 ] && [ "$control_ok" = 1 ]; then
  echo "PASS: fragmented MP4 kept ${frag_dur}s after kill -9, classic MP4 kept ${control_dur}"
else
  echo "FAIL: fragmented=${frag_dur} control=${control_dur}"
  exit 1
fi
