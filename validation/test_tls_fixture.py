"""Port publication regression; no certificates, requests or browser profiles."""
import pathlib
import tempfile
import unittest

from tls_fixture import publish_port


class PortPublicationTests(unittest.TestCase):
    def test_parent_only_sees_the_complete_published_port(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            publish_port(root, 54321)
            self.assertEqual((root / 'tls-port.txt').read_text('ascii'), '54321')
            self.assertFalse((root / 'tls-port.tmp').exists())

    def test_invalid_ports_do_not_create_a_handshake(self):
        for value in (0, 65536, -1, True, '54321', 1.5):
            with tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                with self.assertRaises(ValueError):
                    publish_port(root, value)
                self.assertEqual(list(root.iterdir()), [])

    def test_existing_or_interrupted_handshake_is_not_overwritten(self):
        for name in ('tls-port.txt', 'tls-port.tmp'):
            with tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                original = root / name
                original.write_text('preserve', encoding='ascii')
                with self.assertRaises(FileExistsError):
                    publish_port(root, 54321)
                self.assertEqual(original.read_text('ascii'), 'preserve')


if __name__ == '__main__':
    unittest.main()
