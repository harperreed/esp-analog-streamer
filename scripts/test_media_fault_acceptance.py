# ABOUTME: Rejects incomplete or contradictory real-device media health evidence.
# ABOUTME: Uses recorded transition values only, without pretending to exercise hardware.
import unittest
import subprocess
import tempfile
import socket
from pathlib import Path
from encoder_fault_check import validate_health_case, validate_capture_deltas, decode_saved_prefix, fresh_listener_closes, read_status_line


def record(kind, at, raw=100, generation=1, sequence=10, production=None):
    if production is None:
        production = at if kind == 'recovered' else 0
    return (f'media-health: transition={kind} event_us={at} operator_epoch=1 '
            f'raw_generation=1 raw_committed={raw} encoded_generation={generation} raw_valid=1 encoded_valid=1 '
            f'encoded_sequence={sequence} source_end={100 if kind == "terminal" else raw} encoded_production_us={production}')


class HealthEvidenceTests(unittest.TestCase):
    def good(self):
        return [record('reset_requested', 2_100_000),
                record('reset_ack', 2_600_000, 120, 1, 10),
                record('recovered', 2_700_000, 130, 2, 1)]

    def test_cooperative(self):
        self.assertEqual(len(validate_health_case(self.good(), 'cooperative', 0, 2_500_000)), 3)

    def test_missing_late_wrong_repeated_and_no_progress_rejected(self):
        for records in [self.good()[:2], self.good()+[self.good()[0]],
                        [self.good()[0], self.good()[1], record('recovered', 4_500_001, 130, 2, 1)],
                        [self.good()[0], self.good()[1], record('terminal', 2_700_000)],
                        [self.good()[0], self.good()[1], record('recovered', 2_700_000, 100, 2, 0)]]:
            with self.subTest(records=records), self.assertRaises(RuntimeError):
                validate_health_case(records, 'cooperative', 0, 2_500_000)

    def test_terminal(self):
        validate_health_case([record('reset_requested', 2_100_000),
                              record('terminal', 4_100_000, 200)], 'terminal', 0, 17_000_000)

    def test_policy_deadlines_use_production_and_bounded_observer_time(self):
        for records, case in [
            ([record('reset_requested', 2_100_000, production=100_001), *self.good()[1:]], 'cooperative'),
            ([self.good()[0], self.good()[1], record('recovered', 4_200_000, 130, 2, 1, production=4_100_001)], 'cooperative'),
            ([self.good()[0], record('terminal', 4_500_001, 200)], 'terminal'),
            ([record('reset_requested', 2_400_001), record('terminal', 4_500_000, 200)], 'terminal')]:
            with self.subTest(records=records), self.assertRaises(RuntimeError):
                validate_health_case(records, case, 0, 17_000_000 if case == 'terminal' else 2_500_000)
        validate_health_case([self.good()[0], record('terminal', 4_500_000, 200)], 'terminal', 0, 17_000_000)
        validate_health_case([self.good()[0], self.good()[1],
                              record('recovered', 4_400_000, 130, 2, 1, production=4_100_000)],
                             'cooperative', 0, 2_500_000)

    def test_invalid_snapshot_flags_never_certify_stage_receipts(self):
        for flag in ('raw_valid', 'encoded_valid'):
            for index in range(3):
                records = self.good()
                records[index] = records[index].replace(f'{flag}=1', f'{flag}=0')
                with self.subTest(flag=flag, index=index), self.assertRaises(RuntimeError):
                    validate_health_case(records, 'cooperative', 0, 2_500_000)

    def test_impossible_publication_and_unknown_field_rejected(self):
        for production in (2_200_000, 2_550_000, 2_700_001):
            records = self.good()
            records[-1] = record('recovered', 2_700_000, 130, 2, 1, production)
            with self.subTest(production=production), self.assertRaises(RuntimeError):
                validate_health_case(records, 'cooperative', 0, 2_500_000)
        records = self.good()
        records[0] += ' surprise=1'
        with self.assertRaises(RuntimeError):
            validate_health_case(records, 'cooperative', 0, 2_500_000)

    def test_actual_device_cross_generation_publication(self):
        records = [
            'media-health: transition=reset_requested event_us=32344234 operator_epoch=2 raw_valid=1 encoded_valid=1 raw_generation=2 raw_committed=1512480 encoded_generation=0 encoded_sequence=1077 source_end=1410624 encoded_production_us=30239539',
            'media-health: transition=reset_ack event_us=32867193 operator_epoch=2 raw_valid=1 encoded_valid=1 raw_generation=2 raw_committed=1537680 encoded_generation=0 encoded_sequence=1077 source_end=1410624 encoded_production_us=30239539',
            'media-health: transition=recovered event_us=33004262 operator_epoch=2 raw_valid=1 encoded_valid=1 raw_generation=2 raw_committed=1544160 encoded_generation=1 encoded_sequence=4 source_end=1542288 encoded_production_us=32991976',
        ]
        validate_health_case(records, 'cooperative', 30_249_000, 32_754_000)
        same_generation = records[:2] + [records[2].replace('encoded_generation=1', 'encoded_generation=0').replace('encoded_sequence=4', 'encoded_sequence=1078')]
        validate_health_case(same_generation, 'cooperative', 30_249_000, 32_754_000)
        for generation, sequence in [(0, 1077), (0, 1076), (1, 0)]:
            invalid = records[:2] + [records[2].replace('encoded_generation=1', f'encoded_generation={generation}').replace('encoded_sequence=4', f'encoded_sequence={sequence}')]
            with self.subTest(generation=generation, sequence=sequence), self.assertRaises(RuntimeError):
                validate_health_case(invalid, 'cooperative', 30_249_000, 32_754_000)
        regressed = [line.replace('encoded_generation=0', 'encoded_generation=2') for line in records[:2]] + [records[2]]
        with self.assertRaises(RuntimeError):
            validate_health_case(regressed, 'cooperative', 30_249_000, 32_754_000)

    def test_capture_counter_fault(self):
        base = dict(frames=1, queue_overflows=0, software_dropped_frames=0,
                    read_errors=0, restarts=0, discontinuities=0)
        for key in base:
            sample = dict(base, frames=2)
            if key != 'frames':
                sample[key] += 1
            else:
                sample[key] = 1
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                validate_capture_deltas(base, [sample])


class ListenerEvidenceTests(unittest.TestCase):
    def receipts(self, peer, joined_at=1000, close_at=2000):
        from test_harness_modules import LISTENER_RECEIPT
        return [f'stream: {peer} joined /stream.mp3 (1 of 2) event_us={joined_at}',
                f'stream: {peer} left after 1.0 s: stopped reading event_us={close_at-1}',
                f'stream: {peer} socket closed in 1 ms; releasing slot (0 held) event_us={close_at}',
                LISTENER_RECEIPT]

    def test_deferred_original_release_cannot_certify_fresh_stream(self):
        from encoder_fault_check import completed_listener_closes
        original = self.receipts('old', 1000, 2000)
        # The old runtime gate counted this checked original release after RESUME.
        self.assertEqual(len(completed_listener_closes(original)), 1)
        self.assertEqual(fresh_listener_closes(original, {'old'}, 1500, '/stream.mp3'), {})
        self.assertEqual(fresh_listener_closes(self.receipts('fresh', 1000, 2000), {'old'}, 1500, '/stream.mp3'), {})
        fresh = self.receipts('fresh', 1600, 2000)
        self.assertEqual(set(fresh_listener_closes(fresh, {'old'}, 1500, '/stream.mp3')), {'fresh'})
        self.assertEqual(fresh_listener_closes(fresh, {'old'}, 1500, '/stream.flac'), {})

    def test_fragmented_real_socket_status_and_incomplete_status(self):
        for fragments, valid in [([b'HTTP/1.', b'1 5', b'03 Busy\r', b'\n'], True),
                                 ([b'HTTP/1.1 503'], False)]:
            receiving, sending = socket.socketpair()
            try:
                for fragment in fragments:
                    sending.sendall(fragment)
                sending.shutdown(socket.SHUT_WR)
                if valid:
                    self.assertEqual(read_status_line(receiving), b'HTTP/1.1 503 Busy\r\n')
                else:
                    with self.assertRaises(RuntimeError):
                        read_status_line(receiving)
            finally:
                receiving.close()
                sending.close()


class PrefixDecodeTests(unittest.TestCase):
    def test_real_compressed_frames_and_separate_tail(self):
        for path, codec in [('/stream.mp3', 'libmp3lame'), ('/stream.flac', 'flac')]:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as temporary:
                out = Path(temporary)
                body = out / 'original-0.body'
                subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi',
                                '-i', 'anullsrc=r=48000:cl=stereo', '-t', '2',
                                '-c:a', codec, '-f', path.split('.')[-1], str(body)],
                               check=True, capture_output=True, timeout=10)
                result = decode_saved_prefix(out, 0, path)
                self.assertGreaterEqual(result['decoded_audio_frames'], 48000)
                self.assertGreater(result['tail_bytes'], 0)

    def test_wav_decoder_command_counts_real_ten_seconds(self):
        from encoder_fault_check import decoder_command, FrameHash
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            source = out / 'source.wav'
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi',
                            '-i', 'anullsrc=r=48000:cl=stereo', '-t', '10',
                            '-c:a', 'pcm_s24le', str(source)],
                           check=True, capture_output=True, timeout=10)
            hashes, progress = out / 'decoded.framehash', out / 'decoded.progress'
            command = decoder_command(SimpleNamespace(stream_port=8000, path='/stream.wav'), 'unused', progress, hashes)
            command[command.index('-i')+1] = str(source)
            result = subprocess.run(command, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            counter = FrameHash(hashes)
            try:
                counter.update(final=True)
                self.assertEqual(counter.frames, 480000)
            finally:
                counter.close()

    def test_non_audio_cannot_pass_as_prefix(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            (out / 'original-0.body').write_bytes(b'not compressed audio')
            with self.assertRaises(RuntimeError):
                decode_saved_prefix(out, 0, '/stream.mp3')


if __name__ == '__main__':
    unittest.main()
