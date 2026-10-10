# ABOUTME: Exercises canonical check evidence with real processes and temporary Git trees.
# ABOUTME: Never runs builds, tool activation, or hardware checks.
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest

CHECK = Path(__file__).resolve().with_name('check')


class EvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        assert 'if [[ "${BASH_SOURCE[0]}" == "$0" ]]' in CHECK.read_text(), "check must guard main before sourced process tests"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(['git', 'init', '-q', self.root], check=True)
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'user.name', 'Evidence test')
        (self.root / '.gitignore').write_text('/target/\n')
        (self.root / 'source').write_text('original\n')
        self.git('add', 'source', '.gitignore')
        self.git('commit', '-qm', 'initial')

    def git(self, *args):
        subprocess.run(['git', '-C', str(self.root), *args], check=True, capture_output=True)

    def run_shell(self, body):
        return subprocess.run(['bash', '-c', f'source "{CHECK}"; cd "{self.root}"; {body}'], text=True, capture_output=True)

    def run_dir(self):
        return next((self.root / 'target/check-logs').iterdir())

    def test_stage_preserves_parent_descriptor(self):
        self.assertNotIn('exec {log_fd}', CHECK.read_text(), 'Mac Bash 3.2 cannot allocate named descriptors')
        result = self.run_shell('exec 3> parent.log; check_init || exit; printf "before\\n" >&3; stage good true || exit; printf "after\\n" >&3; exec 3>&-')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'parent.log').read_text(), 'before\nafter\n')

    def test_child_143_is_an_ordinary_failure(self):
        result = self.run_shell('check_init || exit; check_install_signal_traps || exit; stage child bash -c "exit 143" || exit; stage after true || exit; check_finalize || exit; exit "$status"')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual((self.run_dir() / 'run.status').read_text(), 'complete\n')
        self.assertIn('child\t143\tchild.log', (self.run_dir() / 'stages.tsv').read_text())
        self.assertIn('after\t0\tafter.log', (self.run_dir() / 'stages.tsv').read_text())

    def test_real_exits_and_aggregate(self):
        result = self.run_shell('check_init || exit; stage bad bash -c "echo out; echo err >&2; exit 7"; stage good true; check_finalize; exit "$status"')
        self.assertEqual(result.returncode, 1, result.stderr)
        run = self.run_dir()
        self.assertEqual((run / 'stages.tsv').read_text(), 'stage\texit_code\tlog\nbad\t7\tbad.log\ngood\t0\tgood.log\n')
        self.assertEqual((run / 'bad.log').read_text(), 'out\nerr\n')
        self.assertIn('FAIL  bad', result.stdout)
        self.assertEqual((run / 'run.status').read_text(), 'complete\n')
        self.assertEqual((run / 'exit-code').read_text(), '1\n')

    def test_source_has_no_side_effects(self):
        result = self.run_shell('test ! -e target')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unique_retained_runs(self):
        self.assertEqual(self.run_shell('check_init && stage good true && check_finalize').returncode, 0)
        run = self.run_dir()
        before = {p.name: p.read_bytes() for p in run.iterdir()}
        self.assertEqual(self.run_shell('check_init && stage good true && check_finalize').returncode, 0)
        self.assertEqual(len(list(run.parent.iterdir())), 2)
        self.assertEqual(before, {p.name: p.read_bytes() for p in run.iterdir()})

    def test_infrastructure_errors(self):
        cases = [
            'mkdir "$LOG_DIR/bad.log"; stage bad touch marker',
            'rm "$LOG_DIR/stages.tsv"; mkdir "$LOG_DIR/stages.tsv"; stage bad true',
            'mkdir "$LOG_DIR/run.status.complete"; check_finalize',
            'rm -rf .git; check_finalize',
        ]
        for body in cases:
            with self.subTest(body=body):
                result = self.run_shell('check_init || exit; ' + body)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                runs = sorted((self.root / 'target/check-logs').iterdir(), key=lambda p: p.stat().st_mtime_ns)
                self.assertEqual((runs[-1] / 'run.status').read_text(), 'incomplete\n')
                self.assertFalse((self.root / 'marker').exists())
                if not (self.root / '.git').exists():
                    break

    def test_metadata_initialization_failure(self):
        self.git('checkout', '--orphan', 'unborn')
        result = self.run_shell('check_init')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.run_dir() / 'run.status').read_text(), 'incomplete\n')

    def test_status_destination_failure(self):
        result = self.run_shell('check_init || exit; mv "$LOG_DIR/run.status" "$LOG_DIR/saved.status"; mkdir "$LOG_DIR/run.status"; check_finalize')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.run_dir() / 'saved.status').read_text(), 'incomplete\n')
        self.assertFalse((self.run_dir() / 'run.status/run.status.complete').exists())

    def test_boundary_changes(self):
        for change in ['echo changed > source', 'echo staged > source; git add source', 'echo config > mise.toml', 'rm source']:
            with self.subTest(change=change):
                result = self.run_shell('check_init || exit; ' + change + '; check_finalize')
                self.assertNotEqual(result.returncode, 0)
                self.git('reset', '--hard', '-q', 'HEAD')
                (self.root / 'mise.toml').unlink(missing_ok=True)

    def test_dirty_and_untracked_limit(self):
        result = self.run_shell('echo dirty > source; echo first > loose; check_init || exit; echo second > loose; check_finalize')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(' M source', (self.run_dir() / 'start.status').read_text())

    def test_setup_failure(self):
        for expression in ['exit 9', 'printf "return 8"', 'printf "if"']:
            result = self.run_shell(f'check_init || exit; check_setup bash -c \'{expression}\'')
            self.assertNotEqual(result.returncode, 0)
        for run in (self.root / 'target/check-logs').iterdir():
            self.assertEqual((run / 'run.status').read_text(), 'incomplete\n')
            self.assertEqual((run / 'stages.tsv').read_text(), 'stage\texit_code\tlog\n')

    def test_warning_detector(self):
        for content, expected in [('fine\n', 0), ('warning: `esp-shazam` generated warnings\n', 1)]:
            (self.root / 'build.log').write_text(content)
            self.assertEqual(self.run_shell('firmware_warnings build.log').returncode, expected)
        (self.root / 'build.log').unlink()
        self.assertNotEqual(self.run_shell('firmware_warnings build.log').returncode, 0)
        (self.root / 'build.log').mkdir()
        self.assertNotEqual(self.run_shell('firmware_warnings build.log').returncode, 0)

    def test_interruption(self):
        process = subprocess.Popen(['bash', '-c', f'source "{CHECK}"; cd "{self.root}"; check_init || exit; check_install_signal_traps || exit; stage waiting bash -c "touch ready; sleep 30"; touch completed; check_finalize'], start_new_session=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 5
            while not (self.root / 'ready').exists() and time.monotonic() < deadline:
                time.sleep(.02)
            self.assertTrue((self.root / 'ready').exists())
            os.killpg(process.pid, signal.SIGTERM)
            process.communicate(timeout=5)
            self.assertEqual(process.returncode, 143)
            self.assertEqual((self.run_dir() / 'run.status').read_text(), 'incomplete\n')
            self.assertFalse((self.root / 'completed').exists())
            self.assertFalse((self.run_dir() / 'exit-code').exists())
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate(timeout=5)


if __name__ == '__main__':
    unittest.main()
