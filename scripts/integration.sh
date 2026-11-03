#!/usr/bin/env bash
# scripts/integration.sh — real ffmpeg, synthetic lavfi inputs, the exact one-segment argv the
# app builds (mirrors src-tauri/crates/core/src/command.rs: screen = input 0, camera = input 1,
# mic = input 2; camera PiP 22 % of height = 236 px, bottom-right, margin 24, rounded, mirrored).
#
#   bash scripts/integration.sh        # from the repo root; ~15 s wall-clock (inputs run at -re)
#
# Writes under tests/out/ (git-ignored):
#   integration.mp4            screen + camera PiP + 1 mic track
#   integration-2audio.mp4     the same with a second audio input mapped as its own track
#   *.json                     ffprobe -show_format -show_streams of each file (for the record)
#   *.progress / *.stderr      ffmpeg -progress output and warnings
# Prints one PASS/FAIL line per assertion with the measured value, then a final PASS or FAIL line,
# and exits 1 on any FAIL. Override SECONDS_TO_RECORD (default 6) or FFMPEG/FFPROBE if needed.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT_DIR="tests/out"
mkdir -p "$OUT_DIR"

FFMPEG="${FFMPEG:-$(command -v ffmpeg || true)}"
FFPROBE="${FFPROBE:-$(command -v ffprobe || true)}"
if [ -z "$FFMPEG" ] || [ ! -x "$FFMPEG" ]; then FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"; fi
if [ -z "$FFPROBE" ] || [ ! -x "$FFPROBE" ]; then FFPROBE="src-tauri/binaries/ffprobe-aarch64-apple-darwin"; fi
command -v xxd >/dev/null 2>&1 || { echo "FAIL: xxd not found on PATH"; exit 1; }
echo "ffmpeg : $FFMPEG ($("$FFMPEG" -version 2>/dev/null | head -1))"
echo "ffprobe: $FFPROBE"

SECONDS_TO_RECORD="${SECONDS_TO_RECORD:-6}"
SCREEN_GRAPH="color=c=blue:s=1920x1080:r=30"
CAMERA_GRAPH="color=c=red:s=640x480:r=30"
MIC_GRAPH="sine=frequency=440:sample_rate=48000"
MIC2_GRAPH="sine=frequency=880:sample_rate=48000"

# Filter graph exactly as command.rs emits it for 1920x1080 (PiP 236 px at 1660:820, radius 29).
FILTER="[0:v]scale=1920:1080:flags=bicubic[scr];[1:v]crop=ih:ih,hflip,scale=236:236:flags=bicubic,format=rgba[pip_c];nullsrc=s=236x236:r=30,format=gray,geq=lum='clip((29-hypot(max(abs(X-118+0.5)-89,0),max(abs(Y-118+0.5)-89,0)))*255+127.5,0,255)',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)[pip_m];[pip_c][pip_m]alphamerge[pip];[scr][pip]overlay=1660:820:format=auto[vout];[2:a]aresample=48000:async=1[aout]"

# Encoder, muxer and progress options exactly as the app passes them (budget 400 MB/h = 889 kbps).
ENCODE=(-c:v hevc_videotoolbox -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main
        -b:v 793k -maxrate 1190k -bufsize 1586k -g 60 -pix_fmt yuv420p -r 30 -fps_mode cfr
        -c:a aac -b:a 96k -ar 48000 -ac 2
        -movflags +frag_keyframe+empty_moov+default_base_moof -frag_duration 2000000
        -progress pipe:1 -nostats)

OUT1="$OUT_DIR/integration.mp4"
OUT2="$OUT_DIR/integration-2audio.mp4"

# ---------------------------------------------------------------- helpers
pass=0
fail=0
report() { # status(0=ok) name detail
  if [ "$1" = 0 ]; then echo "PASS  $2: $3"; pass=$((pass + 1)); else echo "FAIL  $2: $3"; fail=$((fail + 1)); fi
}
check_range() { # name value lo hi
  local ok=1
  if awk -v v="$2" -v lo="$3" -v hi="$4" 'BEGIN { exit !(v + 0 >= lo + 0 && v + 0 <= hi + 0) }'; then ok=0; fi
  report "$ok" "$1" "measured $2, expected $3..$4"
}
check_eq() { # name value expected
  local ok=1
  if [ "$2" = "$3" ]; then ok=0; fi
  report "$ok" "$1" "measured $2, expected $3"
}
probe() { "$FFPROBE" -v error "$@" 2>/dev/null; }
fmt_entry() { probe -show_entries "format=$2" -of default=nw=1:nk=1 "$1"; }
stream_entry() { probe -select_streams "$2" -show_entries "stream=$3" -of default=nw=1:nk=1 "$1"; }
count_streams() { probe -show_entries stream=codec_type -of csv=p=0 "$1" | grep -c "^$2" || true; }
kbps() { awk -v b="$1" 'BEGIN { printf "%.1f", b / 1000 }'; }
pixel() { # file x y -> "r g b" of frame 60 at (x,y); -1 -1 -1 when nothing decoded
  local hex
  hex=$("$FFMPEG" -v error -i "$1" -vf "select=eq(n\,60),format=rgb24,crop=1:1:$2:$3" \
        -fps_mode passthrough -frames:v 1 -f rawvideo -pix_fmt rgb24 - | xxd -p)
  if [ "${#hex}" -lt 6 ]; then echo "-1 -1 -1"; return; fi
  echo "$((16#${hex:0:2})) $((16#${hex:2:2})) $((16#${hex:4:2}))"
}
check_pixel() { # name file x y expectation(awk expression over r g b)
  local rgb r g b ok=1
  rgb=$(pixel "$2" "$3" "$4")
  read -r r g b <<<"$rgb"
  if awk -v r="$r" -v g="$g" -v b="$b" "BEGIN { exit !($5) }"; then ok=0; fi
  report "$ok" "$1" "rgb($r,$g,$b) at ($3,$4) frame 60, expected $5"
}
common_checks() { # file label
  local f="$1" label="$2" dur nv na w h vc ac br
  probe -print_format json -show_format -show_streams "$f" >"${f%.mp4}.json"
  dur=$(fmt_entry "$f" duration)
  nv=$(count_streams "$f" video)
  na=$(count_streams "$f" audio)
  w=$(stream_entry "$f" v:0 width)
  h=$(stream_entry "$f" v:0 height)
  vc=$(stream_entry "$f" v:0 codec_name)
  ac=$(stream_entry "$f" a:0 codec_name)
  br=$(fmt_entry "$f" bit_rate)
  echo "  size: $(wc -c <"$f" | tr -d ' ') bytes, format bit_rate: $br b/s = $(kbps "$br") kbps"
  check_range "$label duration s" "$dur" 5.0 7.5
  check_eq "$label video streams" "$nv" 1
  check_eq "$label resolution" "${w}x${h}" "1920x1080"
  check_eq "$label video codec" "$vc" hevc
  check_eq "$label audio codec" "$ac" aac
  check_range "$label format bit_rate kbps" "$(kbps "$br")" 600 1200
  echo "$na"
}

# ---------------------------------------------------------------- run 1: the app argv verbatim
echo "== run 1: screen + camera PiP + mic, ${SECONDS_TO_RECORD}s at -re → $OUT1"
t0=$SECONDS
"$FFMPEG" -hide_banner -loglevel warning \
  -thread_queue_size 1024 -re -f lavfi -i "$SCREEN_GRAPH" \
  -thread_queue_size 1024 -re -f lavfi -i "$CAMERA_GRAPH" \
  -thread_queue_size 1024 -re -f lavfi -i "$MIC_GRAPH" \
  -filter_complex "$FILTER" \
  -map "[vout]" -map "[aout]" \
  "${ENCODE[@]}" -t "$SECONDS_TO_RECORD" -y "$OUT1" \
  >"$OUT_DIR/integration.progress" 2>"$OUT_DIR/integration.stderr"
echo "  ffmpeg exit 0 after $((SECONDS - t0)) s wall-clock; stderr: $(wc -l <"$OUT_DIR/integration.stderr" | tr -d ' ') line(s)"
na1=$(common_checks "$OUT1" "run1" | tee /dev/stderr | tail -1)
check_eq "run1 audio streams" "$na1" 1
check_pixel "run1 screen centre is blue"      "$OUT1" 960  540 "b > 200 && r < 60"
check_pixel "run1 PiP centre is red (camera)" "$OUT1" 1778 938 "r > 200 && b < 60"
check_pixel "run1 PiP outer corner is blue (rounded corner shows screen)" "$OUT1" 1661 821 "b > 200 && r < 60"

# ---------------------------------------------------------------- run 2: two audio inputs, two tracks
echo "== run 2: + 4th input (second mic) mapped as a separate track → $OUT2"
t0=$SECONDS
"$FFMPEG" -hide_banner -loglevel warning \
  -thread_queue_size 1024 -re -f lavfi -i "$SCREEN_GRAPH" \
  -thread_queue_size 1024 -re -f lavfi -i "$CAMERA_GRAPH" \
  -thread_queue_size 1024 -re -f lavfi -i "$MIC_GRAPH" \
  -thread_queue_size 1024 -re -f lavfi -i "$MIC2_GRAPH" \
  -filter_complex "$FILTER;[3:a]aresample=48000:async=1[a1]" \
  -map "[vout]" -map "[aout]" -map "[a1]" \
  "${ENCODE[@]}" -t "$SECONDS_TO_RECORD" -y "$OUT2" \
  >"$OUT_DIR/integration-2audio.progress" 2>"$OUT_DIR/integration-2audio.stderr"
echo "  ffmpeg exit 0 after $((SECONDS - t0)) s wall-clock; stderr: $(wc -l <"$OUT_DIR/integration-2audio.stderr" | tr -d ' ') line(s)"
na2=$(common_checks "$OUT2" "run2" | tee /dev/stderr | tail -1)
check_eq "run2 audio streams" "$na2" 2
check_eq "run2 second audio codec" "$(stream_entry "$OUT2" a:1 codec_name)" aac

# ---------------------------------------------------------------- verdict
for f in "$OUT_DIR/integration.stderr" "$OUT_DIR/integration-2audio.stderr"; do
  if [ -s "$f" ]; then echo "-- ffmpeg warnings in $f:"; cat "$f"; fi
done
echo "---"
if [ "$fail" = 0 ]; then
  echo "PASS: $pass checks passed, 0 failed ($OUT1, $OUT2)"
else
  echo "FAIL: $fail of $((pass + fail)) checks failed ($OUT1, $OUT2)"
  exit 1
fi
