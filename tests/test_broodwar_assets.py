"""Asset activation is all-or-nothing; malformed art never replaces a live pack."""
import importlib.util
import json
import pathlib
import struct
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("model_assets",ROOT/"tools/broodwar_native/model_assets.py")
assets=importlib.util.module_from_spec(spec);spec.loader.exec_module(assets)

def glb(path, mutate=None):
    binary=struct.pack("<12f",0,0,0,1,0,0,0,1,0,0,0,1)
    binary+=struct.pack("<12H",0,1,2,0,3,1,0,2,3,1,3,2)
    doc={"asset":{"version":"2.0"},"buffers":[{"byteLength":len(binary)}],
         "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48},{"buffer":0,"byteOffset":48,"byteLength":24}],
         "accessors":[{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":1,"componentType":5123,"count":12,"type":"SCALAR"}],
         "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
         "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}
    if mutate: mutate(doc)
    encoded=json.dumps(doc).encode();encoded+=b" "*((-len(encoded))%4)
    data=struct.pack("<III",0x46546C67,2,28+len(encoded)+len(binary))
    path.write_bytes(data+struct.pack("<II",len(encoded),0x4E4F534A)+encoded+struct.pack("<II",len(binary),0x004E4942)+binary)

class AssetsTest(unittest.TestCase):
    def test_hd_sprite_height_uses_native_tile_units(self):
        self.assertEqual(assets.sprite_world_height(128, 4), 1)
        self.assertEqual(assets.sprite_world_height(64, 2), 1)
        for invalid in (0, -1, float("nan"), float("inf"), True, None):
            with self.assertRaises(ValueError):
                assets.sprite_world_height(128, invalid)

    def test_native_scale_rejects_meter_height_and_double_scaling(self):
        definition = {"targetHeight": 1.748,
                      "nativeScale": {"heightPixels": 103.82232666, "spriteScale": 4}}
        with self.assertRaisesRegex(ValueError, "native sprite/tile"):
            assets.check_native_scale(definition)
        definition["targetHeight"] = assets.sprite_world_height(103.82232666, 4)
        assets.check_native_scale(definition)
        definition["targetHeight"] *= 2
        with self.assertRaises(ValueError):
            assets.check_native_scale(definition)

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=pathlib.Path(self.temp.name);self.source=self.root/"source";self.source.mkdir()
        self.runtime=self.root/"runtime";self.model=self.source/"unit-65000.glb";glb(self.model)
    def test_real_geometry(self):
        r=assets.inspect_glb(self.model);self.assertEqual(r["triangles"],4);self.assertEqual(r["bounds"],[[0,0,0],[1,1,1]])
    def test_unknown_type_without_rebuild(self):
        self.assertIn("65000",assets.bundle(self.source))
    def test_external_buffers_rejected(self):
        glb(self.model,lambda d:d["buffers"][0].update(uri="https://example.com/buffer"))
        with self.assertRaisesRegex(ValueError,"embedded"):assets.bundle(self.source)
    def test_out_of_bounds_accessor_rejected(self):
        glb(self.model,lambda d:d["accessors"][0].update(count=999))
        with self.assertRaisesRegex(ValueError,"exceeds view"):assets.bundle(self.source)
    def test_node_cycles_rejected(self):
        glb(self.model,lambda d:d["nodes"][0].update(children=[0]))
        with self.assertRaisesRegex(ValueError,"Cyclic"):assets.bundle(self.source)
    def test_parent_path_rejected(self):
        (self.source/"catalog.json").write_text(json.dumps({"schemaVersion":1,"models":{"0":{"file":"../outside.glb"}}}))
        with self.assertRaisesRegex(ValueError,"traverses"):assets.bundle(self.source)
    def test_symlink_escape_rejected(self):
        self.model.unlink();other=self.root/"other.glb";glb(other);self.model.symlink_to(other)
        with self.assertRaisesRegex(ValueError,"escapes"):assets.bundle(self.source)
    def test_atomic_install_and_rollback(self):
        a=assets.install(self.source,self.runtime,check=False)
        original=(self.runtime/"models-3d/catalog.json").read_bytes()
        b=assets.install(self.source,self.runtime,check=False)
        self.assertNotEqual(a["release"],b["release"])
        assets.rollback(self.runtime)
        self.assertEqual(json.loads(original),json.loads((self.runtime/"models-3d/catalog.json").read_bytes()))
        self.assertTrue((self.runtime/"models-3d/releases"/b["release"]).exists())
    def test_bad_second_asset_keeps_current_catalog(self):
        assets.install(self.source,self.runtime,check=False)
        original=(self.runtime/"models-3d/catalog.json").read_bytes()
        (self.source/"unit-65001.glb").write_bytes(b"broken")
        with self.assertRaises(ValueError):assets.install(self.source,self.runtime,check=False)
        self.assertEqual(original,(self.runtime/"models-3d/catalog.json").read_bytes())
    def test_renderer_failure_never_activates(self):
        assets.install(self.source,self.runtime,check=False)
        original=(self.runtime/"models-3d/catalog.json").read_bytes()
        (self.runtime/"soju-bw-scene").touch()
        with mock.patch.object(assets.subprocess,"run",side_effect=subprocess.CalledProcessError(1,"render")):
            with self.assertRaises(subprocess.CalledProcessError):assets.install(self.source,self.runtime)
        self.assertEqual(original,(self.runtime/"models-3d/catalog.json").read_bytes())
    def test_bad_transform_rejected(self):
        self.model.with_suffix(".json").write_text('{"scale":-1}')
        with self.assertRaisesRegex(ValueError,"scale"):assets.bundle(self.source)
    def test_unknown_filename_requires_mapping(self):
        self.model.rename(self.source/"alien.glb")
        with self.assertRaisesRegex(ValueError,"catalog.json"):assets.bundle(self.source)
    def test_effects_are_separate_and_rollback_independently(self):
        assets.install(self.source,self.runtime,check=False)
        units=(self.runtime/"models-3d/catalog.json").read_bytes()
        first=assets.install(self.source,self.runtime,check=False,domain="effects")
        assets.install(self.source,self.runtime,check=False,domain="effects")
        assets.rollback(self.runtime,domain="effects")
        self.assertEqual(json.loads((self.runtime/"effects-3d/catalog.json").read_text())["release"],first["release"])
        self.assertEqual((self.runtime/"models-3d/catalog.json").read_bytes(),units)
    def test_bad_flight_height_rejected(self):
        self.model.with_suffix(".json").write_text('{"flightHeight":-1}')
        with self.assertRaisesRegex(ValueError,"flightHeight"):assets.bundle(self.source)
    def test_invalid_mask_and_escaping_texture_rejected(self):
        sidecar=self.model.with_suffix(".json")
        sidecar.write_text('{"teamMask":"blue"}')
        with self.assertRaisesRegex(ValueError,"teamMask"):assets.bundle(self.source)
        sidecar.write_text('{"teamMask":"texture","teamMaskTexture":"../mask.png"}')
        with self.assertRaises(ValueError):assets.bundle(self.source)
    def test_mask_texture_survives_install(self):
        (self.source/"mask.png").write_bytes(b"private-mask")
        self.model.with_suffix(".json").write_text('{"teamMask":"texture","teamMaskTexture":"mask.png"}')
        assets.install(self.source,self.runtime,check=False)
        definition=json.loads((self.runtime/"models-3d/catalog.json").read_text())["models"]["65000"]
        self.assertEqual((self.runtime/"models-3d"/definition["teamMaskTexture"]).read_bytes(),b"private-mask")
    def test_bad_team_materials_rejected(self):
        self.model.with_suffix(".json").write_text('{"teamMaterials":"all materials"}')
        with self.assertRaisesRegex(ValueError,"teamMaterials"):assets.bundle(self.source)

if __name__=="__main__":unittest.main()
