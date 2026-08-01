import json, urllib.request
url = "https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs?branch=codex/rust-font-switcher&per_page=3"
req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
data = json.load(urllib.request.urlopen(req))
for r in data.get("workflow_runs", []):
    print(f"{r['id']} {r['name']} status={r['status']} conclusion={r['conclusion']} created={r['created_at']}")
if not data.get("workflow_runs"):
    print("No workflow runs found yet")