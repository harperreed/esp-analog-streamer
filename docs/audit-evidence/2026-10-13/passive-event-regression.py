#!/usr/bin/env python3
# ABOUTME: Exercises the passive driver's actual snapshot fields through the real Trial.event writer.
# ABOUTME: Extracts the row expression without running hardware code and verifies a temporary JSONL receipt.
import ast
import json
from pathlib import Path
import runpy
import tempfile
import time
from types import SimpleNamespace

driver = Path(__file__).with_name('passive-check.py')
namespace = runpy.run_path(str(driver))
tree = ast.parse(driver.read_text())
trial_class = next(node for node in tree.body
                   if isinstance(node, ast.ClassDef) and node.name == 'PassiveTrial')
run = next(node for node in trial_class.body
           if isinstance(node, ast.FunctionDef) and node.name == 'run')
row_assignment = next(node for node in ast.walk(run)
                      if isinstance(node, ast.Assign)
                      and any(isinstance(target, ast.Name) and target.id == 'row'
                              for target in node.targets))
row_expression = ast.Expression(row_assignment.value)
start = time.monotonic()
row = eval(compile(row_expression, str(driver), 'eval'), {'time': time, 'start': start})
with tempfile.TemporaryDirectory(prefix='passive-event-regression-') as directory:
    trial = namespace['Trial'].__new__(namespace['Trial'])
    trial.args = SimpleNamespace(out=Path(directory))
    trial.sequence = 0
    trial.event('snapshot', **row)
    lines = (Path(directory) / 'events.jsonl').read_text().splitlines()
    assert len(lines) == 1, lines
    receipt = json.loads(lines[0])
    assert receipt['kind'] == 'snapshot' and receipt['sequence'] == 1, receipt
    assert all(receipt[name] == value for name, value in row.items()), receipt
    assert 'sample_started_monotonic' in receipt and 'monotonic' in receipt, receipt
    assert receipt['monotonic'] >= receipt['sample_started_monotonic'], receipt
    print(json.dumps(dict(passed=True, writer='Trial.event', fields=sorted(receipt),
                          network_calls=0)))
