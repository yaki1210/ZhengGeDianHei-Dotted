import json, urllib.request

# Get the failed run
url = "https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs?branch=codex/rust-font-switcher&per_page=1&status=failure"
req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
data = json.load(urllib.request.urlopen(req))
runs = data.get("workflow_runs", [])
if not runs:
    print("No failed runs found")
    exit()

run_id = runs[0]["id"]
print(f"Run ID: {run_id}")

# Get jobs for this run
jobs_url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/jobs"
req = urllib.request.Request(jobs_url, headers={"Accept": "application/vnd.github+json"})
jobs_data = json.load(urllib.request.urlopen(req))
for job in jobs_data.get("jobs", []):
    print(f"\nJob: {job['name']} status={job['status']} conclusion={job['conclusion']}")
    for step in job.get("steps", []):
        print(f"  Step: {step['name']} status={step['status']} conclusion={step.get('conclusion', 'N/A')}")

# Get the logs
logs_url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/logs"
req = urllib.request.Request(logs_url, headers={"Accept": "application/vnd.github+json"})
try:
    # Get the first job's logs
    logs_data = json.load(urllib.request.urlopen(req))
    for log in logs_data:
        print(f"\nLog: {log.get('name', 'N/A')}")
except Exception as e:
    print(f"\nLogs URL: {logs_url}")
    print(f"Try downloading logs manually: {logs_url}")