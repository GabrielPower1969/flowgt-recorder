#!/usr/bin/env bash
# bench-startup.sh — time from spawning the app's ffmpeg argv (-t 3, synthetic lavfi inputs)
# to the first "frame=" line on the -progress stdout. Prints milliseconds; PASS < 1000 ms.
# Note: -re + lavfi start is instant; real AVFoundation devices add their own open latency.
# Outputs go to bench-out/ (git-ignored). Exits non-zero on FAIL.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
OUT_DIR="bench-out"
mkdir -p "$OUT_DIR"

FFMPEG="$(command -v ffmpeg || true)"
[[ -n "$FFMPEG" ]] || FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"

LIMIT_MS=1000
SECONDS_ARG=3
OUT="$OUT_DIR/startup-3s.mp4"
ERR="$OUT_DIR/startup-stderr.txt"
PROG="$OUT_DIR/startup-progress.txt"
FIFO="$OUT_DIR/startup-progress.fifo"

SCREEN_GRAPH="testsrc2=size=1920x1080:rate=30"
CAMERA_GRAPH="testsrc=size=1280x720:rate=30"
MIC_GRAPH="sine=frequency=440:sample_rate=48000"
FILTER="[0:v]scale=1920:1080:flags=bicubic[scr];[1:v]crop=ih:ih,hflip,scale=236:236:flags=bicubic,format=rgba[pip_c];nullsrc=s=236x236:r=30,format=gray,geq=lum='clip((29-hypot(max(abs(X-118+0.5)-89,0),max(abs(Y-118+0.5)-89,0)))*255+127.5,0,255)',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)[pip_m];[pip_c][pip_m]alphamerge[pip];[scr][pip]overlay=1660:820:format=auto[vout];[2:a]aresample=48000:async=1[aout]"

ARGV=( "$FFMPEG" -hide_banner -loglevel warning
  -thread_queue_size 1024 -re -f lavfi -i "$SCREEN_GRAPH"
  -thread_queue_size 1024 -re -f lavfi -i "$CAMERA_GRAPH"
  -thread_queue_size 1024 -re -f lavfi -i "$MIC_GRAPH"
  -filter_complex "$FILTER"
  -map "[vout]" -map "[aout]"
  -c:v hevc_videotoolbox -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main
  -b:v 793k -maxrate 1190k -bufsize 1586k -g 60 -pix_fmt yuv420p -r 30 -fps_mode cfr
  -c:a aac -b:a 96k -ar 48000 -ac 2
  -movflags +frag_keyframe+empty_moov+default_base_moof -frag_duration 2000000
  -progress pipe:1 -nostats -t "$SECONDS_ARG" -y "$OUT" )

# millisecond clock (BSD date has no %N; perl Time::HiRes is on every macOS)
now_ms() { perl -MTime::HiRes=time -e 'printf("%d\n", time()*1000)'; }

rm -f "$FIFO"; mkfifo "$FIFO"
: >"$PROG"
# The child blocks on opening the FIFO for writing until we open it for reading below,
# so the clock starts right before that open, i.e. right before ffmpeg actually execs.
"${ARGV[@]}" >"$FIFO" 2>"$ERR" &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; rm -f "$FIFO"' EXIT

t0="$(now_ms)"
first_ms=""
while IFS= read -r line; do
  printf '%s\n' "$line" >>"$PROG"
  if [[ -z "$first_ms" && "$line" == frame=* ]]; then
    first_ms="$(now_ms)"
  fi
done <"$FIFO"
rc=0; wait "$pid" || rc=$?
trap - EXIT; rm -f "$FIFO"

echo "date=$(date '+%Y-%m-%d %H:%M:%S %Z')"
echo "ffmpeg=$FFMPEG"
echo "ffmpeg_exit=$rc"
if [[ -z "$first_ms" ]]; then
  echo "startup_ms=NA"
  echo "stderr_tail=$(tail -3 "$ERR" | tr '\n' ' ')"
  echo "FAIL startup: no frame= line on -progress stdout"
  exit 1
fi
ms=$((first_ms - t0))
first_frame="$(grep -m1 '^frame=' "$PROG" | cut -d= -f2)"
echo "first_progress_frame=$first_frame"
echo "startup_ms=$ms"
echo "note=clock includes the perl timestamp call (~5 ms) and the first -progress flush (stats_period default 0.5 s)"
if [[ $ms -lt $LIMIT_MS ]]; then echo "PASS startup_ms=$ms<$LIMIT_MS"; exit 0
else echo "FAIL startup_ms=$ms>=$LIMIT_MS"; exit 1; fi
