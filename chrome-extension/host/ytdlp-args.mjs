// yt-dlp 参数构造与输出解析（纯函数，单测直接调用）。
// 所有参数以数组交给 spawn，永不拼 shell 字符串。

import path from 'node:path';

export const PROGRESS_PREFIX = 'FLOWGT_PROGRESS ';
export const FILE_PREFIX = 'FLOWGT_FILE:';

// 视频：最高画质视频流 + 优先 AAC 音轨（Opus 装进 mp4 在 QuickTime/Windows 播放器里没声音），
//       没有 m4a 音轨时退回任意最佳音轨，再不行取单文件最佳。
// 音频：最佳音轨 → 转 m4a（AAC，全平台可播）。
const FORMAT = {
  video: ['-f', 'bv*+ba[ext=m4a]/bv*+ba/b', '--merge-output-format', 'mp4'],
  audio: ['-f', 'ba/b', '-x', '--audio-format', 'm4a', '--audio-quality', '0'],
};

export function buildArgs({ url, kind, platform, useCookies, outDir, nodePath }) {
  if (!FORMAT[kind]) throw new Error(`unknown kind: ${kind}`);
  const args = [
    ...FORMAT[kind],
    '--no-playlist',
    '--newline',
    '--windows-filenames', // 文件可能发给 Windows 用户：统一剔除 Windows 非法字符
    '--no-mtime', // 否则文件时间被设成视频上传日，在下载目录里排到最底下
    '--progress',
    '--progress-template',
    `download:${PROGRESS_PREFIX}%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s`,
    '--print',
    `after_move:${FILE_PREFIX}%(filepath)s`,
    '-o',
    path.join(outDir, '%(title).80s [%(id)s].%(ext)s'),
  ];
  // YouTube 缺 JS 运行时会丢格式（拿不到真正的最高画质）；host 自己就跑在 node 上，直接复用
  if (platform === 'youtube' && nodePath) args.push('--js-runtimes', `node:${nodePath}`);
  // B 站未登录只给低画质；YouTube 永不带 cookies（没必要，且降低风控风险）
  if (platform === 'bilibili' && useCookies) args.push('--cookies-from-browser', 'chrome');
  args.push('--', url); // "--" 之后只能是 URL，杜绝以 "-" 开头的参数注入
  return args;
}

export function parseProgress(line) {
  if (!line.startsWith(PROGRESS_PREFIX)) return null;
  const [pct, speed, eta] = line.slice(PROGRESS_PREFIX.length).split('|').map((s) => s.trim());
  const percent = parseFloat(pct);
  if (Number.isNaN(percent)) return null;
  const clean = (s) => (!s || s === 'NA' || s === 'Unknown' ? '' : s);
  return { percent: Math.min(100, percent), speed: clean(speed), eta: clean(eta) };
}

export function parseFileLine(line) {
  return line.startsWith(FILE_PREFIX) ? line.slice(FILE_PREFIX.length).trim() : null;
}

// cookies 读取失败（钥匙串拒绝/解密失败）时，host 会自动去掉 cookies 重试一次
export function isCookieError(text) {
  return /cookie|keyring|keychain|decrypt/i.test(text);
}
