# ABOUTME: Checks the network-fault-batch launcher's --pcap dry run: capture preview plus harness plan, sudo-free, no files.
# ABOUTME: Runs the real wrapper against loopback; never captures packets, installs a qdisc or contacts a board.
import subprocess
import tempfile
import unittest
from pathlib import Path


class NetworkFaultBatchPcapTest(unittest.TestCase):
    def test_pcap_dry_run_previews_capture_and_harness_without_files(self):
        scripts = Path(__file__).resolve().parent
        packet_capture = scripts.parent / 'target' / 'packet-capture'
        with tempfile.TemporaryDirectory() as directory:
            before = sorted(packet_capture.rglob('*')) if packet_capture.exists() else None
            result = subprocess.run([str(scripts / 'network-fault-batch'), '--pcap', '--dry-run', '--host', '127.0.0.1',
                                     '--', '--trials', 'null-1'],
                                    cwd=directory, capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Capture 3600 seconds on lo: host 127.0.0.1 and (arp or (tcp and (port 80 or port 8000)))\n',
                          result.stdout)
            self.assertIn('# install\n', result.stdout)
            self.assertEqual(list(Path(directory).iterdir()), [])
            after = sorted(packet_capture.rglob('*')) if packet_capture.exists() else None
            self.assertEqual(after, before)


if __name__ == '__main__':
    unittest.main()
