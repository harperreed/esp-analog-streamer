# ABOUTME: Exercises the actual collector process runner with real child processes.
# ABOUTME: Checks descendant cleanup when the leader exits but its child ignores SIGTERM.
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import signal
import time
import tempfile

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('board_runner', ROOT / 'run-board.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
with tempfile.TemporaryDirectory(prefix='quant-process-test-') as temporary:
    root = Path(temporary)
    assert runner.run_logged([sys.executable, '-c', 'print("ok")'], root / 'success.log', 5) == 0
    assert (root / 'success.log').read_text().strip() == 'ok'
    assert runner.run_logged([sys.executable, '-c', 'raise SystemExit(7)'], root / 'failure.log', 5) == 7
    child = ('import os,signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); '
             'print(os.getpid(),flush=True); time.sleep(60)')
    leader = 'import subprocess,sys,time; subprocess.Popen([sys.executable,"-c",sys.argv[1]]); time.sleep(60)'
    try:
        runner.run_logged([sys.executable, '-c', leader, child], root / 'group.log', 0.5)
    except subprocess.TimeoutExpired:
        pass
    else:
        raise AssertionError('timeout was not propagated')
    child_pid = int((root / 'group.log').read_text().strip())
    status = Path(f'/proc/{child_pid}/stat')
    assert not status.exists() or status.read_text().rpartition(')')[2].split()[0] == 'Z'
    worker = """import importlib.util,sys,time
from pathlib import Path
spec=importlib.util.spec_from_file_location('runner',sys.argv[1])
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
r.install_interrupt_handler()
root=Path(sys.argv[2]);(root/'ready').write_text('ready')
try:
 r.run_logged([sys.executable,'-c','import time;time.sleep(60)'],root/'interrupted.log',70)
except KeyboardInterrupt:
 pass
finally:
 r.run_logged([sys.executable,'-c','print("cleanup reached")'],root/'cleanup.log',5)
"""
    proc = subprocess.Popen([sys.executable, '-c', worker, str(ROOT/'run-board.py'), str(root)])
    try:
        deadline=time.monotonic()+5
        while not (root/'ready').exists():
            assert time.monotonic()<deadline
            time.sleep(0.01)
        time.sleep(0.1)
        proc.send_signal(signal.SIGTERM)
        assert proc.wait(timeout=15)==0
        assert (root/'cleanup.log').read_text().strip()=='cleanup reached'
    finally:
        if proc.poll() is None:
            proc.kill();proc.wait()
receipt = {'status': 'pass', 'real_process_cases': ['success', 'nonzero exit', 'leader exits while descendant ignores SIGTERM', 'SIGTERM reaches finally and runs cleanup process'],
           'uart_or_hardware_opened': False}
(ROOT / 'process-tests.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
