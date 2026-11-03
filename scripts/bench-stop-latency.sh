#!/usr/bin/env bash
# bench-stop-latency.sh — start the app's ffmpeg argv (-t 60, synthetic lavfi inputs) with stdin
# on a FIFO, write "q" after 5 s, and measure the milliseconds until the process has exited and
# the output file is ffprobe-able. PASS < 2000 ms. Outputs go to bench-out/ (git-ignored).
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
OUT_DIR="bench-out"
mkdir -p "$OUT_DIR"

FFMPEG="$(command -v ffmpeg || true)"
[[ -n "$FFMPEG" ]] || FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
FFPROBE="$(command -v ffprobe || true)"
[[ -n "$FFPROBE" ]] || FFPROBE="src-tauri/binaries/ffprobe-aarch64-apple-darwin"

LIMIT_MS=2000
RUN_BEFORE_Q_S=5
OUT="$OUT_DIR/stop-latency.mp4"
ERR="$OUT_DIR/stop-latency-stderr.txt"
PROG="$OUT_DIR/stop-latency-progress.txt"
FIFO="$OUT_DIR/stop-latency-stdin.fifo"

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
  -progress pipe:1 -nostats -t 60 -y "$OUT" )

now_ms() { perl -MTime::HiRes=time -e 'printf("%d\n", time()*1000)'; }

rm -f "$FIFO"; mkfifo "$FIFO"
# Hold the FIFO open read+write on fd 3 so ffmpeg's open of stdin never blocks and never sees EOF
# (the app keeps the stdin pipe open the same way); we write "q" into it later.
exec 3<>"$FIFO"
"${ARGV[@]}" <"$FIFO" >"$PROG" 2>"$ERR" &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; exec 3>&-; rm -f "$FIFO"' EXIT

sleep "$RUN_BEFORE_Q_S"
if ! kill -0 "$pid" 2>/dev/null; then
  rc=0; wait "$pid" || rc=$?
  echo "ffmpeg_exit=$rc"
  echo "stderr_tail=$(tail -3 "$ERR" | tr '\n' ' ')"
  echo "FAIL stop-latency: ffmpeg exited before q was sent"
  exit 1
fi
frames_before_q="$(grep '^frame=' "$PROG" | tail -1 | cut -d= -f2 || true)"

t0="$(now_ms)"
printf 'q' >&3
while kill -0 "$pid" 2>/dev/null; do sleep 0.01; done
t_exit="$(now_ms)"
rc=0; wait "$pid" || rc=$?

probe_ok=0; tries=0
while [[ $tries -lt 300 ]]; do
  if "$FFPROBE" -v error -show_entries format=duration -of default=nw=1:nk=1 "$OUT" >"$OUT_DIR/stop-latency-probe.txt" 2>/dev/null; then
    probe_ok=1; break
  fi
  sleep 0.01; tries=$((tries+1))
done
t_probe="$(now_ms)"
trap - EXIT; exec 3>&-; rm -f "$FIFO"

echo "date=$(date '+%Y-%m-%d %H:%M:%S %Z')"
echo "ffmpeg=$FFMPEG"
echo "ffmpeg_exit=$rc"
echo "run_before_q_s=$RUN_BEFORE_Q_S"
echo "frames_before_q=${frames_before_q:-NA}"
echo "frames_final=$(grep '^frame=' "$PROG" | tail -1 | cut -d= -f2 || echo NA)"
echo "progress_last=$(grep '^progress=' "$PROG" | tail -1 | cut -d= -f2 || echo NA)"
echo "exit_ms=$((t_exit - t0))"
echo "probe_ms=$((t_probe - t0))"
echo "file_duration_s=$(cat "$OUT_DIR/stop-latency-probe.txt" 2>/dev/null || echo NA)"
echo "file_bytes=$(stat -f %z "$OUT" 2>/dev/null || stat -c %s "$OUT" 2>/dev/null || echo NA)"
echo "note=ffmpeg polls stdin once per stats_period (default 0.5 s), so up to 500 ms of this is detection latency"
total=$((t_probe - t0))
if [[ $probe_ok -eq 1 && $total -lt $LIMIT_MS ]]; then echo "PASS stop_latency_ms=$total<$LIMIT_MS"; exit 0
else echo "FAIL stop_latency_ms=$total (limit $LIMIT_MS) probe_ok=$probe_ok"; exit 1; fi
