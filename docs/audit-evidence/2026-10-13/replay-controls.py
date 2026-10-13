#!/usr/bin/env python3
# ABOUTME: Evaluates original control-fault observations with the corrected close-batch gate.
# ABOUTME: Keeps offline verdicts separate from immutable failed device acquisitions.
import hashlib
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace

base = Path(__file__).resolve().parent
source = base / 'control-check.py'
spec = importlib.util.spec_from_file_location('control_evaluator', source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
receipts = {}
for name, mode in [('retention-two', 'retention'), ('cancel', 'cancel')]:
    result_path = base / name / 'result.json'
    original = result_path.read_bytes()
    trial = module.ControlTrial.__new__(module.ControlTrial)
    trial.result = json.loads(original)
    assert trial.result['passed'] is False and trial.result['cleanup_complete']
    assert not trial.result.get('error') and not trial.result['observation_errors']
    trial.args = SimpleNamespace(mode=mode)
    trial.intent = trial.result['intent']
    trial.network_generation = trial.result['network_generation']
    trial.queue = trial.result['queue']
    trial.speaker = trial.result['selected_identity']['speaker']
    trial.samples = [row for line in (base / name / 'events.jsonl').read_text().splitlines()
                     if (row := json.loads(line))['kind'] == 'snapshot']
    raw = (base / (name + '-uart.log')).read_bytes()
    trial.evaluate(raw, module.markers(raw))
    assert result_path.read_bytes() == original
    receipts[name] = dict(
        offline_evaluation_passed=True, original_acquisition_passed=False,
        original_errors=trial.result['cleanup_errors'],
        original_result_sha256=hashlib.sha256(original).hexdigest(),
        full_uart_sha256=hashlib.sha256(raw).hexdigest(),
        actual_stall_duration_us=trial.result['actual_stall_duration_us'],
        capture_gate=trial.result['capture_gate'], payload_receipt=trial.result['payload_receipt'],
        observation_end_us=trial.result['observation_end_us'],
        stop_receipt=trial.result['stop_receipt'],
        observations=len(trial.samples))
receipts['evaluator_sha256'] = hashlib.sha256(source.read_bytes()).hexdigest()
(base / 'offline-evaluations.json').write_text(json.dumps(receipts, indent=2) + '\n')
print(json.dumps({key: value['offline_evaluation_passed'] for key, value in receipts.items()
                  if isinstance(value, dict)}))
