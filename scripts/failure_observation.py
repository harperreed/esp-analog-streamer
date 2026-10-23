# ABOUTME: Keeps existing collectors running for bounded evidence after a diagnostic failure.
# ABOUTME: Reports budget, UART exit and interrupt limits without changing the primary failure.

import argparse
import math
import time


def observation_seconds(value):
    try:
        duration = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError('must be finite seconds from 0 to 120') from error
    if not math.isfinite(duration) or not 0 <= duration <= 120:
        raise argparse.ArgumentTypeError('must be finite seconds from 0 to 120')
    return duration


def observe_failure(seconds, collector, check_budget, record):
    if seconds == 0:
        return None
    started = time.monotonic()
    deadline = started + seconds
    reason, secondary = 'deadline', None
    record('failure_observation_start', requested_seconds=seconds)
    try:
        while True:
            try:
                check_budget()
            except Exception as error:
                reason, secondary = 'output_budget', str(error)
                break
            if collector is None or collector.poll() is not None:
                reason = 'collector_exit'
                secondary = ('UART collector unavailable' if collector is None else
                             f'UART collector exited during failure observation: exit={collector.returncode}')
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            time.sleep(min(.1, remaining))
    except KeyboardInterrupt:
        reason, secondary = 'interrupted', 'interrupted during failure observation'
    result = dict(requested_seconds=seconds, observed_seconds=time.monotonic() - started,
                  stop_reason=reason, secondary_error=secondary)
    record('failure_observation_end', **result)
    return result
