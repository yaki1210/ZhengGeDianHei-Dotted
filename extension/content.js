// 正格点黑 16 全局字体 - content script
// 在 document_start 注入全局 !important 字体规则；
// 对计算样式命中图标字体的元素反向加 !important 内联样式，
// 避免图标（Material Icons / Font Awesome / iconfont 等）变成乱码方块。
const FAMILY = '"正格点黑 16"';
const STYLE_ID = 'zgdh-force-font';
const ICON_FONT_RE = /(icon|awesome|symbols?|emoji|glyph|wingdings?|dingbats?|mdi|material|ionicons?|octicons?|bootstrap|fontello|zocial|typicons|line-icons?|tinyicons)/i;

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

function isIconFont(el) {
  try {
    return ICON_FONT_RE.test(getComputedStyle(el).fontFamily || '');
  } catch (e) {
    return false;
  }
}

function exemptIcons() {
  let changed = false;
  for (const el of document.querySelectorAll('*')) {
    if (exempted.has(el)) continue;
    if (isIconFont(el)) {
      const fam = getComputedStyle(el).fontFamily.split(',')[0].trim();
      el.style.setProperty('font-family', fam, 'important');
      exempted.add(el);
      changed = true;
    }
  }
  return changed;
}

function refresh() {
  ensureStyle();
  styleEl.textContent = enabled
    ? '*{font-family:' + FAMILY + ',sans-serif !important;}'
    : '';
  if (!enabled) {
    for (const el of exempted) {
      if (el.isConnected) el.style.removeProperty('font-family');
    }
    exempted.clear();
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
