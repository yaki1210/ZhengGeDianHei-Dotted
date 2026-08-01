function $(id) { return document.getElementById(id); }

chrome.storage.sync.get({ enabled: true, excludeIcons: true }, function (s) {
  $('enable').checked = s.enabled;
  $('icons').checked = s.excludeIcons;
});

$('enable').addEventListener('change', function () {
  chrome.storage.sync.set({ enabled: $('enable').checked });
});

$('icons').addEventListener('change', function () {
  chrome.storage.sync.set({ excludeIcons: $('icons').checked });
});
