import json, urllib.request, io, zipfile, os

run_id = 30693136661
token = os.environ.get("GITHUB_TOKEN", "")

# Try to get annotations (they're public)
ann_url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/annotations"
req = urllib.request.Request(ann_url, headers={"Accept": "application/vnd.github+json", "User-Agent": "Python"})
try:
    resp = urllib.request.urlopen(req)
    ann_data = json.load(resp)
    for ann in ann_data:
        print(f"Path: {ann['path']} Line: {ann.get('start_line','')}")
        print(f"Message: {ann.get('message','')}")
        print(f"---")
except Exception as e:
    print(f"Annotations error: {e}")

# Try to get the job log via the raw log URL
# The raw log URL format is: https://github.com/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/job/{job_id}/logs
# But we need the job id
# Let's try to get the list of jobs
jobs_url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/jobs"
req = urllib.request.Request(jobs_url, headers={"Accept": "application/vnd.github+json", "User-Agent": "Python"})
try:
    resp = urllib.request.urlopen(req)
    jobs_data = json.load(resp)
    for job in jobs_data.get("jobs", []):
        print(f"\nJob: {job['name']} id={job['id']}")
        for step in job.get("steps", []):
            conclusion = step.get("conclusion", "N/A")
            if conclusion == "failure":
                print(f"  FAILED: {step['name']}")
                # Try to get the log for this step
                # The raw log URL
                log_url = f"https://github.com/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/job/{job['id']}/logs"
                print(f"  Log URL: {log_url}")
except Exception as e:
    print(f"Jobs error: {e}")