"""Regression checks for source normalization; no external art fixtures."""
import struct
import tempfile
import unittest
from pathlib import Path
from convert_models import aligned_m3,read_glb


class ConversionTests(unittest.TestCase):
    def test_unaligned_table_preserves_payload_and_entries(self):
        payload=b'authored mesh bytes'
        offset=12+len(payload)
        table=struct.pack('<4sIII',b'NGER',12,1,1)
        source=struct.pack('<4sII',b'43DM',offset,1)+payload+table
        fixed=aligned_m3(source)
        new_offset=struct.unpack_from('<I',fixed,4)[0]
        self.assertEqual(new_offset%4,0)
        self.assertEqual(fixed[12:len(source)],source[12:])
        self.assertEqual(fixed[new_offset:new_offset+16],table)
        self.assertEqual(aligned_m3(fixed),fixed)

    def test_truncated_index_rejected(self):
        with self.assertRaises(ValueError):aligned_m3(struct.pack('<4sII',b'43DM',12,999))

    def test_effect_only_glb_is_reportable(self):
        doc=b'{"asset":{"version":"2.0"}}'
        doc+=b' '*(-len(doc)%4)
        data=struct.pack('<4sIIII',b'glTF',2,20+len(doc),len(doc),0x4e4f534a)+doc
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'empty.glb';path.write_bytes(data)
            with self.assertRaisesRegex(ValueError,'no geometry'):read_glb(path)


if __name__=='__main__':unittest.main()
