// 正格点黑 16 全局字体 - content script
// 在 document_start 注入全局 !important 字体规则；
// 对图标元素反向加 !important 内联样式以保留原图标字体，
// 避免 Material Icons / Font Awesome / iconfont 等图标变成乱码或文字。
//
// 识别策略（任一命中即视为图标元素）：
//   1) 计算样式 font-family 命中图标字体关键词
//   2) 元素 class 命中常见图标库前缀（fa-/material-icons/ri-/bx-…）
//   3) 元素直接文本节点含 PUA 码点（U+E000-F8FF / U+F0000-F8FFD / U+100000-10FFFD）
//   4) ::before / ::after 的 content 含 PUA 码点
const FAMILY = '"正格点黑 16"';
const STYLE_ID = 'zgdh-force-font';

// 图标字体 family 名关键词（扩充：Feather / Remix / Tabler / Phosphor / Boxicons /
// Iconoir / Themify / Unicons / Dripicons / Simple-Line / Heroicons / Cryptofont 等）
const ICON_FONT_RE = /(icon|awesome|symbols?|emoji|glyph|wingdings?|dingbats?|mdi|material|ionicons?|octicons?|bootstrap|fontello|zocial|typicons?|line-icons?|tinyicons|feather|remix|heroicons?|tabler|phosphor|boxicons?|iconfont|fonticon|ricons|themify|unicons?|vaadin|foundation|rpg-awesome|weather|cryptofont|cryptoco|mapicon|open-iconic|iconoir|bytesize|vc-icons|pe-icon|simple-line|sl-icon|dripicons?|payfont|prestashop|whhg|fontelico|websymbols|si-glyph)/i;

// 图标库 class 前缀（覆盖 FA / Material / Glyphicon / Tabler / Remix / Boxicons /
// Phosphor / Iconoir / Feather / Themify / Typicons / Unicons / Simple-Line / LineAwesome /
// WeatherIcons / dripicons / Open Iconic / UIkit 等）
const ICON_CLASS_RE = /^(fa[srbldp]?|material-icons?|material-symbols|mi-?|md-|iconfont|icon-|glyphicon-?|gl-|ti-|typ-|oi-|dripicons?-?|dr-|uim?|uis-|uikit-|uk-|bx[sydlp]?-?|bxs-|bxd-|bxp-|bxl-|ri-|ris-|rib-|rf-|fe-|feather-|iconoir-|ph-|phs-|phc-|pha-|phf-|pht-|phl-|tabler-icon|tabler-|ti-|sl-|si-|la-|la[sldprb]?-|wi-|wi-(day|night|forecast|na|wind|directional|solar|lunar)-|ls-)/i;

// PUA 码点：BMP 私用区 + Plane 15 + Plane 16
const PUA_RE = /[\uE000-\uF8FF]|[\u{F0000}-\u{FFFFD}]|[\u{100000}-\u{10FFFD}]/u;

let enabled = true;
let excludeIcons = true;
let styleEl = null;
const exempted = new Set();
let timer = null;

function ensureStyle() {
  if (styleEl) return;
  styleEl = document.createElement('style');
  styleEl.id = STYLE_ID;
  (document.head || document.documentElement).appendChild(styleEl);
}

function readContent(el, pseudo) {
  try {
    return getComputedStyle(el, pseudo).content;
  } catch (e) {
    return '';
  }
}

// 综合判定元素是否为图标元素；命中时返回应保留的 font-family 值，否则返回 false
function detectIconFamily(el) {
  let fam;
  try {
    fam = getComputedStyle(el).fontFamily || '';
  } catch (e) {
    return false;
  }

  // 1) font-family 命中关键词
  if (ICON_FONT_RE.test(fam)) return fam;

  // 2) class 命中图标库前缀
  const cls = el.className;
  if (typeof cls === 'string' && cls) {
    const parts = cls.split(/\s+/);
    for (let i = 0; i < parts.length; i++) {
      if (ICON_CLASS_RE.test(parts[i])) return fam;
    }
  }

  // 3) 直接文本节点含 PUA 码点
  const children = el.childNodes;
  for (let i = 0; i < children.length; i++) {
    const n = children[i];
    if (n.nodeType === 3 && n.nodeValue && PUA_RE.test(n.nodeValue)) return fam;
  }

  // 4) 伪元素 content 含 PUA 码点
  const before = readContent(el, '::before');
  if (before && before !== 'none' && before !== 'normal' && PUA_RE.test(before)) return fam;
  const after = readContent(el, '::after');
  if (after && after !== 'none' && after !== 'normal' && PUA_RE.test(after)) return fam;

  return false;
}

function exemptIcons() {
  let changed = false;
  for (const el of document.querySelectorAll('*')) {
    if (exempted.has(el)) continue;
    const fam = detectIconFamily(el);
    if (fam) {
      // 保留完整的原始 family 列表（不截取第一个），保证 fallback 链不丢
      el.style.setProperty('font-family', fam, 'important');
      // 启用 ligature / 连字，支持 Material Icons / Tabler 等 ligature 图标
      el.style.setProperty('font-feature-settings', '"liga" 1, "calt" 1', 'important');
      el.style.setProperty('-webkit-font-feature-settings', '"liga" 1, "calt" 1', 'important');
      exempted.add(el);
      changed = true;
    }
  }
  return changed;
}

function clearExempted() {
  for (const el of exempted) {
    if (!el.isConnected) continue;
    el.style.removeProperty('font-family');
    el.style.removeProperty('font-feature-settings');
    el.style.removeProperty('-webkit-font-feature-settings');
  }
  exempted.clear();
}

function refresh() {
  ensureStyle();
  styleEl.textContent = enabled
    ? '*{font-family:' + FAMILY + ',sans-serif !important;}'
    : '';
  if (!enabled) {
    clearExempted();
    return;
  }
  if (excludeIcons) exemptIcons();
}

function schedule() {
  if (!enabled || !excludeIcons) return;
  clearTimeout(timer);
  timer = setTimeout(function () {
    if (exemptIcons()) schedule();
  }, 600);
}

// 动态加载的内容（懒加载图片后的文字、SPA 路由等）也要扫一遍
new MutationObserver(schedule).observe(document.documentElement, {
  childList: true,
  subtree: true,
});

chrome.storage.sync.get({ enabled: true, excludeIcons: true }, function (s) {
  enabled = s.enabled;
  excludeIcons = s.excludeIcons;
  refresh();
});

chrome.storage.onChanged.addListener(function (ch, area) {
  if (area !== 'sync') return;
  if (ch.enabled) enabled = ch.enabled.newValue;
  if (ch.excludeIcons) excludeIcons = ch.excludeIcons.newValue;
  refresh();
});
