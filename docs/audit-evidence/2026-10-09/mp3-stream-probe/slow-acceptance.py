# ABOUTME: Checks MP3 slow-reader reclamation beside a continuously drained real listener.
# ABOUTME: Throwaway check counts actual decoded samples and bounds every receive.
import concurrent.futures
import json
from pathlib import Path
import sys
import time
import acceptance as helper

host = sys.argv[1]
out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
result = {}
slow = fast = probe = None
try:
    slow, slow_at = helper.connect(host, 8000, '/stream.mp3', small=True)
    fast, fast_at = helper.connect(host, 8000, '/stream.mp3')
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(helper.receive, fast, 1667, fast_at + 60)
        time.sleep(max(0, slow_at + 25 - time.monotonic()))
        assert not future.done(), 'fast listener ended before reclamation probe'
        probe, admitted = helper.connect(host, 8000, '/stream.mp3')
        data = helper.receive(probe, 10, admitted + 5)
        probe.close()
        probe = None
        result['probe'] = helper.check(data, out, 'reclaimed-slot')
        result['slot_reclaimed'] = True
        result['fast'] = helper.check(future.result(), out, 'fast-isolation')
    result['passed'] = True
except Exception as error:
    result.update(passed=False, error=repr(error))
finally:
    for peer in [slow, fast, probe]:
        if peer is not None:
            peer.close()
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result), flush=True)
raise SystemExit(0 if result.get('passed') else 1)
