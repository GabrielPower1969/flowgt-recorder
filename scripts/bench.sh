#!/usr/bin/env bash
# bench.sh — CPU / memory / dropped-frame bench for the FlowGT Recorder ffmpeg pipeline.
#
#   bash scripts/bench.sh [--seconds N]   pipeline mode (default): run the app's exact ffmpeg
#                                         argv on synthetic lavfi inputs WITH -re (real time),
#                                         sample the ffmpeg process every second with ps.
#   bash scripts/bench.sh --app [--seconds N]
#                                         sample the running app (flowgt-recorder) and its ffmpeg
#                                         children while a human is recording. exit 2 if no app.
#   bash scripts/bench.sh --idle          sample flowgt-recorder alone for 10 s. exit 2 if no app.
#
# Outputs go to bench-out/ (git-ignored). Prints key=value lines and a final PASS/FAIL line;
# exits non-zero on FAIL. Note: `ps %cpu` on macOS is a decaying short-window average, so the
# first samples of a fresh process under-report; cpu_from_cputime_pct is the lifetime average.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
OUT_DIR="bench-out"
mkdir -p "$OUT_DIR"

MODE="pipeline"
SECONDS_ARG=30
while [[ $# -gt 0 ]]; do
  case "$1" in
    --seconds) SECONDS_ARG="$2"; shift 2 ;;
    --app) MODE="app"; shift ;;
    --idle) MODE="idle"; shift ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 64 ;;
  esac
done

FFMPEG="$(command -v ffmpeg || true)"
[[ -n "$FFMPEG" ]] || FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
FFPROBE="$(command -v ffprobe || true)"
[[ -n "$FFPROBE" ]] || FFPROBE="src-tauri/binaries/ffprobe-aarch64-apple-darwin"

RSS_LIMIT_MB=300      # pipeline / app: peak RSS budget
APP_CPU_BUDGET=15     # app mode: mean CPU budget (per CLAUDE.md / docs/BENCH.md)
IDLE_CPU_BUDGET=1     # idle mode: mean CPU budget

# Synthetic stand-ins for the screen and camera devices (input 0 and 1).
SCREEN_GRAPH="testsrc2=size=1920x1080:rate=30"
CAMERA_GRAPH="testsrc=size=1280x720:rate=30"
MIC_GRAPH="sine=frequency=440:sample_rate=48000"
# Mirrors src-tauri/crates/core/src/command.rs: PiP 22 % of 1080 = 236 px, bottom-right, margin 24.
FILTER="[0:v]scale=1920:1080:flags=bicubic[scr];[1:v]crop=ih:ih,hflip,scale=236:236:flags=bicubic,format=rgba[pip_c];nullsrc=s=236x236:r=30,format=gray,geq=lum='clip((29-hypot(max(abs(X-118+0.5)-89,0),max(abs(Y-118+0.5)-89,0)))*255+127.5,0,255)',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)[pip_m];[pip_c][pip_m]alphamerge[pip];[scr][pip]overlay=1660:820:format=auto[vout];[2:a]aresample=48000:async=1[aout]"

# build_argv SECONDS OUT  -> fills ARGV (always with -re: pipeline mode is a real-time capture)
build_argv() {
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
    -progress pipe:1 -nostats -t "$1" -y "$2" )
}

# sample_pids "pid,pid,..."  -> prints "sum_cpu_pct sum_rss_kb" or nothing when no pid is alive
sample_pids() {
  ps -o %cpu=,rss= -p "$1" 2>/dev/null | awk '{c+=$1; r+=$2; n++} END {if (n>0) printf "%.1f %d\n", c, r}' || true
}

# cputime_secs PID -> accumulated cpu seconds of one process (ps cputime is [H:]MM:SS.ss)
cputime_secs() {
  ps -o cputime= -p "$1" 2>/dev/null | awk -F: '{s=0; for(i=1;i<=NF;i++) s=s*60+$i; printf "%.2f\n", s}' || true
}

# stats SAMPLES_FILE -> mean_cpu peak_cpu peak_rss_mb samples
stats() {
  awk '{c+=$1; if($1>pc)pc=$1; if($2>pr)pr=$2; n++}
       END {if(n==0){print "0 0 0 0"} else printf "%.1f %.1f %.1f %d\n", c/n, pc, pr/1024, n}' "$1"
}

echo "mode=$MODE"
echo "date=$(date '+%Y-%m-%d %H:%M:%S %Z')"
echo "ffmpeg=$FFMPEG"

case "$MODE" in
pipeline)
  OUT="$OUT_DIR/pipeline-${SECONDS_ARG}s.mp4"
  PROG="$OUT_DIR/pipeline-progress.txt"
  ERR="$OUT_DIR/pipeline-stderr.txt"
  SAMPLES="$OUT_DIR/pipeline-samples.txt"
  : >"$SAMPLES"
  build_argv "$SECONDS_ARG" "$OUT"
  echo "seconds=$SECONDS_ARG"
  echo "out=$OUT"
  "${ARGV[@]}" >"$PROG" 2>"$ERR" &
  pid=$!
  trap 'kill "$pid" 2>/dev/null || true' EXIT
  last_cpu_secs="0"
  while kill -0 "$pid" 2>/dev/null; do
    line="$(sample_pids "$pid")"
    if [[ -n "$line" ]]; then
      echo "$line" >>"$SAMPLES"
      ct="$(cputime_secs "$pid")"; [[ -n "$ct" ]] && last_cpu_secs="$ct"
    fi
    sleep 1
  done
  rc=0; wait "$pid" || rc=$?
  trap - EXIT
  echo "ffmpeg_exit=$rc"
  read -r mean_cpu peak_cpu peak_rss_mb n < <(stats "$SAMPLES")
  drop="$(grep '^drop_frames=' "$PROG" | tail -1 | cut -d= -f2 || true)"; drop="${drop:-NA}"
  dup="$(grep '^dup_frames=' "$PROG" | tail -1 | cut -d= -f2 || true)"; dup="${dup:-NA}"
  frames="$(grep '^frame=' "$PROG" | tail -1 | cut -d= -f2 || true)"; frames="${frames:-NA}"
  speed="$(grep '^speed=' "$PROG" | tail -1 | cut -d= -f2 || true)"; speed="${speed:-NA}"
  cpu_from_cputime="$(awk -v s="$last_cpu_secs" -v t="$SECONDS_ARG" 'BEGIN{ if(t>0) printf "%.1f", 100*s/t; else print "NA"}')"
  echo "samples=$n"
  echo "mean_cpu_pct=$mean_cpu"
  echo "peak_cpu_pct=$peak_cpu"
  echo "cpu_from_cputime_pct=$cpu_from_cputime"
  echo "peak_rss_mb=$peak_rss_mb"
  echo "frames=$frames"
  echo "drop_frames=$drop"
  echo "dup_frames=$dup"
  echo "speed=$speed"
  if [[ -f "$OUT" ]]; then
    size_bytes="$(stat -f %z "$OUT" 2>/dev/null || stat -c %s "$OUT")"
    dur="$("$FFPROBE" -v error -show_entries format=duration -of default=nw=1:nk=1 "$OUT" 2>/dev/null || echo 0)"
    echo "file_bytes=$size_bytes"
    echo "file_duration_s=$dur"
    echo "file_kbps=$(awk -v b="$size_bytes" -v d="$dur" 'BEGIN{ if(d>0) printf "%.0f", b*8/d/1000; else print "NA"}')"
    echo "file_mb_per_hour=$(awk -v b="$size_bytes" -v d="$dur" 'BEGIN{ if(d>0) printf "%.0f", b/1048576/d*3600; else print "NA"}')"
  fi
  echo "note=pipeline mode includes lavfi source generation; the 15 % app budget does not apply"
  fail=""
  [[ "$rc" -eq 0 ]] || fail="$fail ffmpeg_exit=$rc"
  [[ "$drop" == "0" ]] || fail="$fail drop_frames=$drop"
  awk -v r="$peak_rss_mb" -v l="$RSS_LIMIT_MB" 'BEGIN{exit !(r<=l)}' || fail="$fail peak_rss_mb=$peak_rss_mb>$RSS_LIMIT_MB"
  if [[ -z "$fail" ]]; then echo "PASS pipeline drop_frames=0 peak_rss_mb=$peak_rss_mb<=$RSS_LIMIT_MB"; exit 0
  else echo "FAIL pipeline:$fail"; exit 1; fi
  ;;

app)
  app_pids="$(pgrep -x flowgt-recorder || true)"
  if [[ -z "$app_pids" ]]; then echo "no app running"; exit 2; fi
  ff_pids=""
  for p in $app_pids; do
    c="$(pgrep -P "$p" -f ffmpeg || true)"; [[ -n "$c" ]] && ff_pids="$ff_pids $c"
  done
  if [[ -z "$ff_pids" ]]; then
    ff_pids="$(pgrep -x ffmpeg || true)"
    [[ -n "$ff_pids" ]] && echo "warning=no ffmpeg child of the app found; sampling ALL ffmpeg processes (may include unrelated jobs)"
  fi
  all="$(echo $app_pids $ff_pids | tr ' ' ',')"
  echo "app_pids=$(echo $app_pids | tr ' ' ',')"
  echo "ffmpeg_pids=$(echo $ff_pids | tr ' ' ',')"
  echo "seconds=$SECONDS_ARG"
  SAMPLES="$OUT_DIR/app-samples.txt"; : >"$SAMPLES"
  i=0
  while [[ $i -lt $SECONDS_ARG ]]; do
    line="$(sample_pids "$all")"; [[ -n "$line" ]] && echo "$line" >>"$SAMPLES"
    sleep 1; i=$((i+1))
  done
  read -r mean_cpu peak_cpu peak_rss_mb n < <(stats "$SAMPLES")
  echo "samples=$n"; echo "mean_cpu_pct=$mean_cpu"; echo "peak_cpu_pct=$peak_cpu"; echo "peak_rss_mb=$peak_rss_mb"
  echo "note=CPU is the sum over app + ffmpeg processes; RSS is the sum of their resident sets"
  ok=1
  awk -v c="$mean_cpu" -v l="$APP_CPU_BUDGET" 'BEGIN{exit !(c<=l)}' || ok=0
  awk -v r="$peak_rss_mb" -v l="$RSS_LIMIT_MB" 'BEGIN{exit !(r<=l)}' || ok=0
  if [[ $ok -eq 1 ]]; then echo "PASS app mean_cpu_pct=$mean_cpu<=$APP_CPU_BUDGET peak_rss_mb=$peak_rss_mb<=$RSS_LIMIT_MB"; exit 0
  else echo "FAIL app mean_cpu_pct=$mean_cpu (budget $APP_CPU_BUDGET) peak_rss_mb=$peak_rss_mb (budget $RSS_LIMIT_MB)"; exit 1; fi
  ;;

idle)
  app_pids="$(pgrep -x flowgt-recorder || true)"
  if [[ -z "$app_pids" ]]; then echo "no app running"; exit 2; fi
  all="$(echo $app_pids | tr ' ' ',')"
  echo "app_pids=$all"
  echo "seconds=10"
  SAMPLES="$OUT_DIR/idle-samples.txt"; : >"$SAMPLES"
  i=0
  while [[ $i -lt 10 ]]; do
    line="$(sample_pids "$all")"; [[ -n "$line" ]] && echo "$line" >>"$SAMPLES"
    sleep 1; i=$((i+1))
  done
  read -r mean_cpu peak_cpu peak_rss_mb n < <(stats "$SAMPLES")
  echo "samples=$n"; echo "mean_cpu_pct=$mean_cpu"; echo "peak_cpu_pct=$peak_cpu"; echo "peak_rss_mb=$peak_rss_mb"
  if awk -v c="$mean_cpu" -v l="$IDLE_CPU_BUDGET" 'BEGIN{exit !(c<l)}'; then
    echo "PASS idle mean_cpu_pct=$mean_cpu<$IDLE_CPU_BUDGET"; exit 0
  else
    echo "FAIL idle mean_cpu_pct=$mean_cpu (budget <$IDLE_CPU_BUDGET)"; exit 1
  fi
  ;;
esac
