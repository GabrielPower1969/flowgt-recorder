// 未打包（Load unpacked）扩展的 ID = 扩展目录绝对路径的 SHA-256 前 128 位，
// 十六进制 0-f 映射为字母 a-p。install.sh 用它写 Native Messaging 的 allowed_origins；
// E2E 会拿 Chrome for Testing 实际分配的 ID 反向校验这个算法。
import crypto from 'node:crypto';

export function unpackedExtensionId(absPath) {
  const hex = crypto.createHash('sha256').update(absPath).digest('hex').slice(0, 32);
  return [...hex].map((c) => String.fromCharCode('a'.charCodeAt(0) + parseInt(c, 16))).join('');
}

// CLI: node host/ext-id.mjs <extension dir>
if (import.meta.url === `file://${process.argv[1]}`) {
  console.log(unpackedExtensionId(process.argv[2]));
}
