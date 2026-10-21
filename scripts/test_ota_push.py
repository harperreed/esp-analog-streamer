# ABOUTME: Checks the OTA push helper on the host: the image head check on real image bytes, token precedence and the
# ABOUTME: wait-for-board loop with an injected fetch; never contacts a board.
import unittest

import ota_push

# The first 288 bytes of the home board's input-gain image (espflash save-image of ELF 008bc545…).
HEAD_HEX = ('e906022fbc520840ee00000000000000008f0100000000012000403ff4e305003254cdab000000000000000000000000'
            '3232636665333500000000000000000000000000000000000000000000000000'
            '6c69626573706964660000000000000000000000000000000000000000000000'
            '30383a30353a323300000000000000004f6374203131203230323600000000007635'
            '2e352e350000000000000000000000000000000000000000000000000000'
            '008bc545b14430a4c230f39495885ec11b7579ed1b1a24975a42652f533e92dc'
            '000063001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000')
HEAD = bytes.fromhex(HEAD_HEX).ljust(ota_push.HEAD_BYTES, b'\0')


class HeadTests(unittest.TestCase):
    def test_a_real_image_head_describes_itself(self):
        d = ota_push.describe(HEAD)
        self.assertEqual((d['version'], d['project'], d['idf_version']), ('22cfe35', 'libespidf', 'v5.5.5'))
        self.assertEqual(d['elf_sha256'], '008bc545b14430a4c230f39495885ec11b7579ed1b1a24975a42652f533e92dc')

    def test_elf_short_and_foreign_images_are_refused_with_a_reason(self):
        with self.assertRaises(ota_push.Failure) as caught:
            ota_push.describe(b'\x7fELF' + bytes(ota_push.HEAD_BYTES))
        self.assertIn('espflash save-image', str(caught.exception))
        with self.assertRaises(ota_push.Failure):
            ota_push.describe(HEAD[:100])
        other = bytearray(HEAD); other[12] = 0x09
        with self.assertRaises(ota_push.Failure) as caught:
            ota_push.describe(bytes(other))
        self.assertIn('another chip', str(caught.exception))


class TokenTests(unittest.TestCase):
    def test_precedence_is_flag_then_environment_then_cfg(self):
        cfg = 'hostname = "x"\nota_token = "from-cfg"\n'
        self.assertEqual(ota_push.token_from('flag', {'OTA_TOKEN': 'env'}, cfg), 'flag')
        self.assertEqual(ota_push.token_from(None, {'OTA_TOKEN': 'env'}, cfg), 'env')
        self.assertEqual(ota_push.token_from(None, {}, cfg), 'from-cfg')
        self.assertEqual(ota_push.token_from(None, {}, 'ota_token = ""\n'), '')
        self.assertEqual(ota_push.token_from(None, {}, None), '')


class MainTests(unittest.TestCase):
    def test_an_unreachable_board_is_one_line_not_a_traceback(self):
        import contextlib, io, tempfile, os
        with tempfile.NamedTemporaryFile(suffix='.bin', delete=False) as f:
            f.write(HEAD); name = f.name
        err = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = ota_push.main([ '127.0.0.1:1', name, '--token', 't', '--cfg', '/nonexistent', '--timeout', '1', '--wait', '0'])
        os.unlink(name)
        self.assertEqual(code, 1)
        self.assertIn('unreachable', err.getvalue())


class WaitTests(unittest.TestCase):
    def test_wait_returns_when_the_board_reports_the_pushed_sha_from_a_new_boot(self):
        clock = [0.0]
        # Pushing the image the board already runs: the same sha from the old boot token is not success.
        answers = iter([Exception('down'), (None, None), ('new', 'boot-old'), ('new', 'boot-new')])
        def fetch():
            value = next(answers)
            if isinstance(value, Exception): raise value
            return value
        seen, seconds = ota_push.wait_for_board('h', 'new', 60, 'boot-old', now=lambda: clock[0], sleep=lambda s: clock.__setitem__(0, clock[0] + s), fetch=fetch)
        self.assertEqual((seen, seconds), (('new', 'boot-new'), 6.0))

    def test_wait_gives_up_at_the_deadline_with_the_last_identity_seen(self):
        clock = [0.0]
        seen, seconds = ota_push.wait_for_board('h', 'new', 10, 'boot-old', now=lambda: clock[0], sleep=lambda s: clock.__setitem__(0, clock[0] + s), fetch=lambda: ('old', 'boot-old'))
        self.assertEqual(seen, ('old', 'boot-old'))
        self.assertGreaterEqual(seconds, 10)


if __name__ == '__main__':
    unittest.main()
