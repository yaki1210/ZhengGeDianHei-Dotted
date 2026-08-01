import json, urllib.request

run_id = 30693136661
# Get annotations for the run
url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/attempts/1/jobs"
req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
data = json.load(urllib.request.urlopen(req))
for job in data.get("jobs", []):
    print(f"Job: {job['name']} id={job['id']}")
    for step in job.get("steps", []):
        if step.get("conclusion") == "failure":
            print(f"  Failed step: {step['name']} number={step['number']}")

# Get annotations
ann_url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/annotations"
req = urllib.request.Request(ann_url, headers={"Accept": "application/vnd.github+json"})
ann_data = json.load(urllib.request.urlopen(req))
for ann in ann_data:
    print(f"\nAnnotation: {ann['path']}:{ann.get('annotation_level','')}")
    print(f"  Message: {ann.get('message','')[:500]}")