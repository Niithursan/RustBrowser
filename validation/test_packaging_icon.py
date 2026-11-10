"""Branding resources are reproducible artwork, not generated user data."""
import importlib.util
from pathlib import Path
import struct
import unittest

ROOT = Path(__file__).resolve().parents[1] / 'packaging' / 'windows'
SPEC = importlib.util.spec_from_file_location('generate_icon', ROOT / 'generate_icon.py')
GENERATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GENERATOR)


class IconTests(unittest.TestCase):
    def test_committed_artwork_matches_generator(self):
        self.assertEqual((ROOT / 'rust-browser.ico').read_bytes(), GENERATOR.generate())

    def test_all_six_frames_have_bounded_rgba_dib_and_mask(self):
        data = (ROOT / 'rust-browser.ico').read_bytes()
        self.assertEqual(struct.unpack_from('<HHH', data), (0, 1, 6))
        end = 6 + 6 * 16
        for i, expected in enumerate((16, 24, 32, 48, 64, 256)):
            width, height, _, _, planes, bits, length, offset = struct.unpack_from('<BBBBHHII', data, 6 + 16 * i)
            self.assertEqual((width or 256, height or 256, planes, bits), (expected, expected, 1, 32))
            self.assertEqual(offset, end)
            self.assertEqual(length, 40 + expected * expected * 4 + (expected + 31) // 32 * 4 * expected)
            self.assertEqual(struct.unpack_from('<IiiHH', data, offset), (40, expected, expected * 2, 1, 32))
            end += length
        self.assertEqual(end, len(data))


if __name__ == '__main__':
    unittest.main()
