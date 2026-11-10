#!/usr/bin/env bash
# quality-check.sh — SSIM / PSNR of the app's video flags against a lossless reference.
#
# 1. lossless reference: testsrc2 1920x1080 30 fps, ffv1 in .mkv
# 2. encode it with the app's video flags (hevc_videotoolbox 793k, no audio)
# 3. encode it again with h264_videotoolbox at the same bitrate as a comparison row
# 4. ffmpeg -i ref -i dist -lavfi "[1:v][0:v]ssim;[1:v][0:v]psnr" -f null -  → SSIM All, PSNR average
#
# Informational — no quality threshold. Exits 0 only if every number parsed.
# Usage (from the repo root):  bash scripts/quality-check.sh [SECONDS=10]
# Outputs: bench-out/quality-ref.mkv, bench-out/quality-hevc.mp4, bench-out/quality-h264.mp4, *.log

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

DURATION="${1:-10}"
case "$DURATION" in
  ''|*[!0-9]*) echo "FAIL: SECONDS must be a positive integer, got '$DURATION'" >&2; exit 2 ;;
esac

OUT_DIR="bench-out"
mkdir -p "$OUT_DIR"

FFMPEG="$(command -v ffmpeg || true)"
[ -n "$FFMPEG" ] || FFMPEG="src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
[ -x "$FFMPEG" ] || { echo "FAIL: ffmpeg not found on PATH or in src-tauri/binaries/" >&2; exit 2; }

now() { perl -MTime::HiRes=time -e 'printf("%.3f\n", time)' 2>/dev/null || date +%s; }
file_bytes() { stat -f %z "$1" 2>/dev/null || stat -c %s "$1"; }
is_number() { case "$1" in ''|*[!0-9.]*) return 1 ;; *) return 0 ;; esac; }

REF="$OUT_DIR/quality-ref.mkv"
echo "ffmpeg=$FFMPEG"
echo "seconds=$DURATION"

# 1. lossless reference (yuv420p so both encoders start from the same chroma subsampling)
"$FFMPEG" -hide_banner -loglevel warning -f lavfi -i "testsrc2=size=1920x1080:rate=30" \
  -t "$DURATION" -c:v ffv1 -level 3 -pix_fmt yuv420p -y "$REF" 2>"$OUT_DIR/quality-ref.log"
echo "reference: file=$REF bytes=$(file_bytes "$REF") codec=ffv1"

# encode <codec> <extra flags...> ; output name derived from codec; prints wall seconds
encode() {
  local codec="$1" out="$2" t0 t1
  shift 2
  t0="$(now)"
  "$FFMPEG" -hide_banner -loglevel warning -i "$REF" -an \
    -c:v "$codec" "$@" -realtime 1 -allow_sw 1 -profile:v main \
    -b:v 793k -maxrate 1190k -bufsize 1586k -g 60 -pix_fmt yuv420p -r 30 -fps_mode cfr \
    -movflags +frag_keyframe+empty_moov+default_base_moof -frag_duration 2000000 \
    -nostats -t "$DURATION" -y "$out" 2>"${out%.mp4}.log"
  t1="$(now)"
  awk -v a="$t0" -v b="$t1" 'BEGIN { printf("%.3f\n", b - a) }'
}

# compare <dist> → "ssim_all psnr_average" (parsed from the ssim/psnr filter summary lines)
compare() {
  local dist="$1" log="${1%.mp4}.compare.log" ssim psnr
  "$FFMPEG" -hide_banner -nostats -i "$REF" -i "$dist" \
    -lavfi "[1:v][0:v]ssim;[1:v][0:v]psnr" -f null - 2>"$log" || true
  ssim="$(sed -nE 's/.*SSIM .*All:([0-9.]+).*/\1/p' "$log" | tail -n1)"
  psnr="$(sed -nE 's/.*PSNR .*average:([0-9.]+|inf).*/\1/p' "$log" | tail -n1)"
  echo "${ssim:-NA} ${psnr:-NA}"
}

HEVC="$OUT_DIR/quality-hevc.mp4"
H264="$OUT_DIR/quality-h264.mp4"

HEVC_WALL="$(encode hevc_videotoolbox "$HEVC" -tag:v hvc1)"
H264_WALL="$(encode h264_videotoolbox "$H264")"

set -- $(compare "$HEVC"); HEVC_SSIM="$1"; HEVC_PSNR="$2"
set -- $(compare "$H264"); H264_SSIM="$1"; H264_PSNR="$2"

kbps() { awk -v b="$1" -v d="$2" 'BEGIN { printf("%.0f", b * 8 / d / 1000) }'; }
HEVC_BYTES="$(file_bytes "$HEVC")"
H264_BYTES="$(file_bytes "$H264")"

printf '%-18s %-9s %-9s %-10s %-10s %-8s %s\n' encoder target_kbps actual_kbps SSIM_All PSNR_avg wall_s file
printf '%-18s %-9s %-9s %-10s %-10s %-8s %s\n' hevc_videotoolbox 793 "$(kbps "$HEVC_BYTES" "$DURATION")" "$HEVC_SSIM" "$HEVC_PSNR" "$HEVC_WALL" "$HEVC"
printf '%-18s %-9s %-9s %-10s %-10s %-8s %s\n' h264_videotoolbox 793 "$(kbps "$H264_BYTES" "$DURATION")" "$H264_SSIM" "$H264_PSNR" "$H264_WALL" "$H264"

ok=1
for v in "$HEVC_SSIM" "$HEVC_PSNR" "$H264_SSIM" "$H264_PSNR"; do
  is_number "$v" || ok=0
done
if [ "$ok" = 1 ]; then
  echo "PASS: all four quality numbers parsed (informational, no threshold)"
  exit 0
else
  echo "FAIL: could not parse SSIM/PSNR (see $OUT_DIR/quality-*.compare.log)"
  exit 1
fi
