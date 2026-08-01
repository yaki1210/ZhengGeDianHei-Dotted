import json, urllib.request, io, zipfile

run_id = 30693136661
# Get the log archive URL
url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/logs"
req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
try:
    resp = urllib.request.urlopen(req)
    # Try to read as zip
    data = resp.read()
    # Check if it's a zip
    import struct
    if data[:2] == b'PK':
        z = zipfile.ZipFile(io.BytesIO(data))
        for name in z.namelist():
            print(f"=== {name} ===")
            content = z.read(name).decode('utf-8', errors='replace')
            # Print last 100 lines
            lines = content.splitlines()
            print('\n'.join(lines[-100:]))
    else:
        print(data[:2000].decode('utf-8', errors='replace'))
except Exception as e:
    print(f"Error: {e}")
    # Try annotations
    ann_url = f"https://api.github.com/repos/yaki1210/ZhengGeDianHei-Dotted/actions/runs/{run_id}/annotations"
    req = urllib.request.Request(ann_url, headers={"Accept": "application/vnd.github+json"})
    ann_data = json.load(urllib.request.urlopen(req))
    for ann in ann_data:
        print(f"\nPath: {ann['path']} Line: {ann.get('start_line','')}")
        print(f"Message: {ann.get('message','')}")