#!/bin/bash
# FlowGT Recorder — 本机下载助手（Native Messaging host）安装脚本（macOS）
#
# 用法: host/install.sh [扩展ID]
#   不传 ID 时按"未打包扩展 ID = 目录路径 SHA-256"算法自动计算（Load unpacked 场景）。
#   若你是从别的位置加载的扩展，从 chrome://extensions 复制 ID 作为参数传入。
#
# 做的事：① 找 node / yt-dlp / ffmpeg ② 生成启动 wrapper（Chrome 启动 host 时没有
# login shell 的 PATH，必须写死绝对路径）③ 写 Native Messaging manifest ④ 自检 ping。
set -euo pipefail

HOST_NAME="nz.co.flowgt.recorder"
HOST_DIR="$(cd "$(dirname "$0")" && pwd -P)"
EXT_DIR="$(cd "$HOST_DIR/../extension" && pwd -P)"
MANIFEST_DIR="${FLOWGT_NM_DIR:-$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts}"
WRAPPER="$HOST_DIR/flowgt-host.sh"

find_bin() {
  for c in "$(command -v "$1" 2>/dev/null || true)" "/opt/homebrew/bin/$1" "/usr/local/bin/$1" "$HOME/.local/bin/$1"; do
    [ -n "$c" ] && [ -x "$c" ] && { echo "$c"; return 0; }
  done
  return 1
}

NODE="$(find_bin node)" || { echo "✗ 未找到 node。先安装 Node.js（https://nodejs.org 或 brew install node）"; exit 1; }
YTDLP="$(find_bin yt-dlp)" || {
  echo "✗ 未找到 yt-dlp。任选一种安装："
  echo "    brew install yt-dlp"
  echo "    python3 -m pip install --user 'yt-dlp[default]'"
  exit 1
}
FFMPEG="$(find_bin ffmpeg)" || echo "⚠ 未找到 ffmpeg：视频合流/音频转 m4a 会失败。brew install ffmpeg"

EXT_ID="${1:-$("$NODE" "$HOST_DIR/ext-id.mjs" "$EXT_DIR")}"
if ! [[ "$EXT_ID" =~ ^[a-p]{32}$ ]]; then
  echo "✗ 扩展 ID 格式不对: $EXT_ID（应为 32 位 a-p 字母）"; exit 1
fi

cat > "$WRAPPER" <<EOF
#!/bin/sh
# 由 host/install.sh 生成，勿手改（重新运行 install.sh 即可更新）
export PATH="$(dirname "${FFMPEG:-/opt/homebrew/bin/ffmpeg}"):$(dirname "$YTDLP"):/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
export FLOWGT_YTDLP="$YTDLP"
exec "$NODE" "$HOST_DIR/flowgt-host.mjs" "\$@"
EOF
chmod +x "$WRAPPER" "$HOST_DIR/flowgt-host.mjs"

mkdir -p "$MANIFEST_DIR"
cat > "$MANIFEST_DIR/$HOST_NAME.json" <<EOF
{
  "name": "$HOST_NAME",
  "description": "FlowGT Recorder local download helper (yt-dlp)",
  "path": "$WRAPPER",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXT_ID/"]
}
EOF

echo "✓ node    $NODE"
echo "✓ yt-dlp  $YTDLP ($("$YTDLP" --version))"
[ -n "${FFMPEG:-}" ] && echo "✓ ffmpeg  $FFMPEG"
echo "✓ 扩展 ID $EXT_ID"
echo "✓ manifest → $MANIFEST_DIR/$HOST_NAME.json"

# 自检：像 Chrome 一样发一条 ping 帧，读回 pong
PONG="$("$NODE" -e '
const { spawn } = require("child_process");
const p = spawn(process.argv[1], [], { stdio: ["pipe", "pipe", "inherit"] });
const body = Buffer.from(JSON.stringify({ type: "ping" }));
const head = Buffer.alloc(4); head.writeUInt32LE(body.length);
p.stdin.write(Buffer.concat([head, body]));
let buf = Buffer.alloc(0);
p.stdout.on("data", (c) => {
  buf = Buffer.concat([buf, c]);
  if (buf.length >= 4 && buf.length >= 4 + buf.readUInt32LE(0)) {
    console.log(buf.subarray(4, 4 + buf.readUInt32LE(0)).toString()); p.stdin.end();
  }
});
setTimeout(() => { console.log("TIMEOUT"); process.exit(1); }, 15000).unref();
' "$WRAPPER")"
echo "✓ 自检 ping → $PONG"
echo
echo "完成。到 chrome://extensions 点 FlowGT Recorder 的刷新按钮，然后在 YouTube / B 站视频页打开插件即可。"
