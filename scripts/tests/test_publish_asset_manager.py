"""Unit tests for release compatibility, without modifying a runtime DLL."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("publish_asset_manager", ROOT / "publish_asset_manager.py")
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)


def metadata(ver="3.8.0", *, methods=None, capabilities=None):
    methods = methods or ["asset.mount_source_json_v1", "asset.decode_v1",
                          "asset.raw_bytes_v1", "asset.vfs_list_json_v1",
                          "asset.text_v1"]
    caps = [{
        "id": "asset_manager.api", "role": "Provides", "kind": "ServiceV1",
        "version": 1, "description": {
            "methods": [{"name": x, "payload": "bytes", "returns": "bytes"} for x in methods]
        }
    }]
    caps.extend(capabilities or [])
    return {"provider_id": "engine.assets.starvault", "provider_version": ver,
            "capabilities": caps}


class PublishContractTests(unittest.TestCase):
    def test_upgraded_provider_preserves_contract(self):
        publisher.ensure_compatible(metadata(), metadata("3.8.1"))

    def test_rejects_same_or_older_version(self):
        for ver in ("3.8.0", "3.7.99"):
            with self.assertRaisesRegex(RuntimeError, "not a newer release"):
                publisher.ensure_compatible(metadata(), metadata(ver))

    def test_rejects_missing_decode_method(self):
        bad = metadata("3.8.1", methods=["asset.raw_bytes_v1"])
        with self.assertRaisesRegex(RuntimeError, "missing service methods"):
            publisher.ensure_compatible(metadata(), bad)

    def test_rejects_changed_wire_contract(self):
        old = metadata()
        new = metadata("3.8.1")
        new["capabilities"][0]["description"]["methods"][0]["payload"] = "new-wire"
        with self.assertRaisesRegex(RuntimeError, "changed method contract"):
            publisher.ensure_compatible(old, new)

    def test_rejects_removed_backend_capability(self):
        old = metadata(capabilities=[{
            "id": "assets.streaming.backend", "role": "Provides",
            "kind": "Other", "version": 1, "description": {},
        }])
        with self.assertRaisesRegex(RuntimeError, "missing capabilities"):
            publisher.ensure_compatible(old, metadata("3.8.1"))

    def test_version_parsing_is_strict(self):
        self.assertEqual(publisher.version("3.8.1"), (3, 8, 1))
        for invalid in ("3.8", "3.8.0-release", "3.8.0.0", "v3.8.1"):
            with self.assertRaises(ValueError):
                publisher.version(invalid)


if __name__ == "__main__":
    unittest.main()
