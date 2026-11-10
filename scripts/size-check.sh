#!/usr/bin/env bash
# size-check.sh — does the app's ffmpeg argv really land at ~400 MB/h?
#
# Encodes a MOVING synthetic screen (testsrc2, scrolling content, worst case) with the exact
# per-segment argv from crates/core/src/command.rs, but WITHOUT -re so it runs faster than
# realtime, then a STATIC control (flat colour). Prints MB/h, ffprobe's average video bitrate
# and the hevc_videotoolbox real-time factor. PASS when the dynamic clip is 350–450 MB/h.
#
# Usage (from the repo root):  bash scripts/size-check.sh [SECONDS=30]
# Outputs: bench-out/size-dynamic.mp4, bench-out/size-static.mp4 (+ .progress/.log, git-ignored)

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

DURATION="${1:-30}"
case "$DURATION" in
  ''|*[!0-9]*) echo "FAIL: SECONDS must be a positive integer, got '$DURATION'" >&2; exit 2 ;;
esac

OUT_DIR="bench-out"
mkdir -p "$OUT_DIR"

# ffmpeg/ffprobe from PATH, else the sidecar binaries fetched by scripts/fetch-ffmpeg.sh.
FFMPEG="$(command -v ffmpeg || true)"
[ -n "$FFMPEG" ] || FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
FFPROBE="$(command -v ffprobe || true)"
[ -n "$FFPROBE" ] || FFPROBE="src-tauri/binaries/ffprobe-aarch64-apple-darwin"
[ -x "$FFMPEG" ] || { echo "FAIL: ffmpeg not found on PATH or in src-tauri/binaries/" >&2; exit 2; }
[ -x "$FFPROBE" ] || { echo "FAIL: ffprobe not found on PATH or in src-tauri/binaries/" >&2; exit 2; }

# Budget: 400 MB/h = 889 kbps total = 793k video + 96k audio.
BUDGET_MIN_MBH=350
BUDGET_MAX_MBH=450

# Identical to the app's graph for a 1920x1080 screen with a 22 % PiP (236 px) at 1660,820.
FILTER='[0:v]scale=1920:1080:flags=bicubic[scr];[1:v]crop=ih:ih,hflip,scale=236:236:flags=bicubic,format=rgba[pip_c];nullsrc=s=236x236:r=30,format=gray,geq=lum='"'"'clip((29-hypot(max(abs(X-118+0.5)-89,0),max(abs(Y-118+0.5)-89,0)))*255+127.5,0,255)'"'"',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)[pip_m];[pip_c][pip_m]alphamerge[pip];[scr][pip]overlay=1660:820:format=auto[vout];[2:a]aresample=48000:async=1[aout]'

CAMERA_GRAPH="testsrc=size=640x480:rate=30"
MIC_GRAPH="sine=frequency=440:sample_rate=48000"

now() { perl -MTime::HiRes=time -e 'printf("%.3f\n", time)' 2>/dev/null || date +%s; }

# encode <screen_graph> <out.mp4>  → prints wall seconds
encode() {
  local screen_graph="$1" out="$2" t0 t1
  t0="$(now)"
  "$FFMPEG" -hide_banner -loglevel warning \
    -thread_queue_size 1024 -f lavfi -i "$screen_graph" \
    -thread_queue_size 1024 -f lavfi -i "$CAMERA_GRAPH" \
    -thread_queue_size 1024 -f lavfi -i "$MIC_GRAPH" \
    -filter_complex "$FILTER" \
    -map "[vout]" -map "[aout]" \
    -c:v hevc_videotoolbox -tag:v hvc1 -realtime 1 -allow_sw 1 -profile:v main \
    -b:v 793k -maxrate 1190k -bufsize 1586k -g 60 -pix_fmt yuv420p -r 30 -fps_mode cfr \
    -c:a aac -b:a 96k -ar 48000 -ac 2 \
    -movflags +frag_keyframe+empty_moov+default_base_moof -frag_duration 2000000 \
    -progress "${out%.mp4}.progress" -nostats -t "$DURATION" -y "$out" 2>"${out%.mp4}.log"
  t1="$(now)"
  awk -v a="$t0" -v b="$t1" 'BEGIN { printf("%.3f\n", b - a) }'
}

file_bytes() { stat -f %z "$1" 2>/dev/null || stat -c %s "$1"; }

# Average video bitrate in kbps. Fragmented MP4 has no sample table in the moov, so the stream's
# bit_rate field is often N/A; in that case sum the video packets ourselves.
video_kbps() {
  local f="$1" br
  br="$("$FFPROBE" -v error -select_streams v:0 -show_entries stream=bit_rate -of default=nw=1:nk=1 "$f" | head -n1)"
  case "$br" in
    ''|N/A|*[!0-9]*)
      "$FFPROBE" -v error -select_streams v:0 -show_entries packet=size -of csv=p=0 "$f" \
        | awk -v d="$DURATION" '{ s += $1 } END { printf("%.1f", s * 8 / d / 1000) }'
      echo " (computed from packets; stream bit_rate=${br:-empty})"
      ;;
    *) awk -v b="$br" 'BEGIN { printf("%.1f (stream bit_rate field)\n", b / 1000) }' ;;
  esac
}

mbh() { awk -v b="$1" -v d="$2" 'BEGIN { printf("%.1f", b / d * 3600 / 1e6) }'; }

echo "ffmpeg=$FFMPEG"
echo "seconds=$DURATION"

# 1. dynamic (worst case): scrolling testsrc2
DYN="$OUT_DIR/size-dynamic.mp4"
DYN_WALL="$(encode "testsrc2=size=1920x1080:rate=30" "$DYN")"
DYN_BYTES="$(file_bytes "$DYN")"
DYN_MBH="$(mbh "$DYN_BYTES" "$DURATION")"
DYN_RTF="$(awk -v d="$DURATION" -v w="$DYN_WALL" 'BEGIN { printf("%.2f", d / w) }')"
echo "dynamic: file=$DYN bytes=$DYN_BYTES MB/h=$DYN_MBH wall_s=$DYN_WALL realtime_factor=${DYN_RTF}x"
echo "dynamic: ffprobe video kbps=$(video_kbps "$DYN")"

# 2. static control: flat colour (VBR should stay far below budget)
STA="$OUT_DIR/size-static.mp4"
STA_WALL="$(encode "color=c=0x3355AA:s=1920x1080:r=30" "$STA")"
STA_BYTES="$(file_bytes "$STA")"
STA_MBH="$(mbh "$STA_BYTES" "$DURATION")"
STA_RTF="$(awk -v d="$DURATION" -v w="$STA_WALL" 'BEGIN { printf("%.2f", d / w) }')"
echo "static: file=$STA bytes=$STA_BYTES MB/h=$STA_MBH wall_s=$STA_WALL realtime_factor=${STA_RTF}x"
echo "static: ffprobe video kbps=$(video_kbps "$STA")"

if awk -v m="$DYN_MBH" -v lo="$BUDGET_MIN_MBH" -v hi="$BUDGET_MAX_MBH" 'BEGIN { exit !(m >= lo && m <= hi) }'; then
  echo "PASS: dynamic ${DYN_MBH} MB/h within ${BUDGET_MIN_MBH}-${BUDGET_MAX_MBH} (static ${STA_MBH} MB/h)"
  exit 0
else
  echo "FAIL: dynamic ${DYN_MBH} MB/h outside ${BUDGET_MIN_MBH}-${BUDGET_MAX_MBH} (static ${STA_MBH} MB/h)"
  exit 1
fi
