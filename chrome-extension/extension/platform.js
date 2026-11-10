// FlowGT Recorder — 媒体平台识别（纯函数）
// 扩展（popup / background）与本机 host（host/flowgt-host.mjs 经 vm 加载）共用这一份，
// 白名单只在这里维护，两端不会漂移。

const FlowGTPlatform = (() => {
  const HOSTS = {
    youtube: new Set(['youtube.com', 'www.youtube.com', 'm.youtube.com', 'music.youtube.com', 'youtu.be']),
    bilibili: new Set(['bilibili.com', 'www.bilibili.com', 'm.bilibili.com', 'b23.tv']),
  };

  function parse(url) {
    try {
      const u = new URL(url);
      return u.protocol === 'https:' || u.protocol === 'http:' ? u : null;
    } catch {
      return null;
    }
  }

  // 域名级识别：host 的安全白名单用它（不在两平台域名内一律拒绝）
  function detectPlatform(url) {
    const u = parse(url);
    if (!u) return null;
    const host = u.hostname.toLowerCase();
    for (const [name, set] of Object.entries(HOSTS)) {
      if (set.has(host)) return name;
    }
    return null;
  }

  // 页面级识别：只有"单个视频页"才在 popup 里显示下载卡（首页/频道页不显示）
  function isMediaPage(url) {
    const platform = detectPlatform(url);
    if (!platform) return false;
    const u = parse(url);
    const p = u.pathname;
    if (platform === 'youtube') {
      if (u.hostname.toLowerCase() === 'youtu.be') return /^\/[\w-]{6,}/.test(p);
      if (p === '/watch') return !!u.searchParams.get('v');
      return /^\/(shorts|live)\/[\w-]{6,}/.test(p);
    }
    if (u.hostname.toLowerCase() === 'b23.tv') return /^\/[\w-]{4,}/.test(p);
    return /^\/(video\/(BV[\w]{10}|av\d+)|bangumi\/play\/(ep|ss)\d+)/i.test(p);
  }

  return { detectPlatform, isMediaPage };
})();

// Node（vm / 单测）环境下导出；浏览器环境为全局常量
if (typeof globalThis !== 'undefined') globalThis.FlowGTPlatform = FlowGTPlatform;
