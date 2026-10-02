import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("catalog_tool", Path(__file__).parents[1] / "catalog.py")
cli = importlib.util.module_from_spec(SPEC)
with patch.object(sys, "path", [str(Path(__file__).parents[1]), *sys.path]):
    SPEC.loader.exec_module(cli)
catalog = cli.model


class CatalogToolTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "Drive folder"
        self.source = Path(self.temporary.name) / "test installer.exe"
        self.source.write_bytes(b"installer test fixture; never execute this file")
        self.invoke("init", "--root", self.root)

    def invoke(self, *arguments, expected=0):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            result = cli.main([str(arg) for arg in arguments])
        self.assertEqual(result, expected)

    def add(self, *extra):
        self.invoke("add", "--root", self.root, "--file", self.source, "--id", "test-app", "--name", "Test App", "--version", "1.0", "--category", "utilities", "--silent-arg=/S", *extra)

    def test_full_local_to_public_workflow_and_hashes(self):
        self.add()
        local = catalog.read_catalog(self.root / "catalog.json")
        artifact = local["packages"][0]["artifact"]
        self.assertEqual(artifact["sha256"], hashlib.sha256(self.source.read_bytes()).hexdigest())
        self.assertEqual((self.root / artifact["local_path"]).read_bytes(), self.source.read_bytes())
        self.invoke("validate", "--catalog", self.root / "catalog.json", "--verify-files")
        self.invoke("publish", "--root", self.root, expected=1)
        self.assertFalse((self.root / "catalog.public.json").exists())
        self.invoke("link", "--root", self.root, "--id", "test-app", "--drive-url", "https://drive.google.com/file/d/1234567890abcdef/view")
        self.invoke("publish", "--root", self.root)
        public = catalog.read_catalog(self.root / "catalog.public.json")
        self.assertNotIn("local_path", public["packages"][0]["artifact"])
        self.assertIn("local_path", catalog.read_catalog(self.root / "catalog.json")["packages"][0]["artifact"])
        self.invoke("validate", "--catalog", self.root / "catalog.public.json", "--public")

    def test_init_does_not_replace_existing_catalog_and_updates_keep_old_installer(self):
        self.add()
        before = (self.root / "catalog.json").read_bytes()
        self.invoke("init", "--root", self.root, expected=1)
        self.assertEqual(before, (self.root / "catalog.json").read_bytes())
        old = json.loads(before)["packages"][0]["artifact"]
        self.source.write_bytes(b"a new version fixture")
        self.add("--replace")
        self.assertTrue((self.root / old["local_path"]).exists())
        self.assertNotEqual(old["sha256"], catalog.read_catalog(self.root / "catalog.json")["packages"][0]["artifact"]["sha256"])

    def test_detects_changed_files(self):
        self.add()
        artifact = catalog.read_catalog(self.root / "catalog.json")["packages"][0]["artifact"]
        (self.root / artifact["local_path"]).write_bytes(b"corrupt")
        self.invoke("validate", "--catalog", self.root / "catalog.json", "--verify-files", expected=1)

    def test_addon_can_depend_on_an_official_program(self):
        self.add("--kind", "addon", "--depends-on", "httpdebugger")
        package = catalog.read_catalog(self.root / "catalog.json")["packages"][0]
        self.assertEqual(package["depends_on"], ["httpdebugger"])
        self.assertEqual(package["kind"], "addon")

    def test_fragment_uses_builtin_groups_and_parent_links(self):
        fragment = {
            "schema_version": 1, "title": "Extras",
            "categories": [{"id": "http-extras", "name": "HTTP extras", "parent": "network-tools"}],
            "packages": [{"id": "helper", "name": "Helper", "version": "1", "publisher": "Test", "description": "Test", "category": "http-extras", "depends_on": ["httpdebugger"], "enabled": False}],
        }
        catalog.validate_catalog(fragment)

    def test_bundled_sources_and_marketplace_dependencies_are_valid(self):
        for path in (catalog.BUILTIN_PATH, catalog.EXTENDED_PATH):
            catalog.validate_catalog(json.loads(path.read_text(encoding="utf-8")), public=True)
        base = catalog.builtin_catalog()
        cline = next(package for package in base["packages"] if package["id"] == "cline")
        self.assertEqual(cline["depends_on"], ["vscode"])
        self.assertEqual(cline["install"]["extension_id"], "saoudrizwan.claude-dev")

    def test_portable_and_interactive_files_round_trip_and_publish(self):
        self.invoke("add", "--root", self.root, "--file", self.source, "--id", "portable", "--name", "Portable", "--version", "1", "--category", "utilities", "--type", "portable", "--destination", "SoftDownloader/apps/fixture")
        self.invoke("add", "--root", self.root, "--file", self.source, "--id", "wizard", "--name", "Wizard", "--version", "1", "--category", "utilities", "--type", "interactive", "--admin")
        for identifier in ("portable", "wizard"):
            self.invoke("link", "--root", self.root, "--id", identifier, "--drive-url", "1234567890abcdef")
        self.invoke("publish", "--root", self.root)
        public = catalog.read_catalog(self.root / "catalog.public.json")
        self.assertEqual([package["install"]["type"] for package in public["packages"]], ["portable", "interactive"])
        self.assertNotIn("arguments", public["packages"][1]["install"])

    def test_manager_pairing_and_detection_validation_reject_unsafe_input(self):
        document = {"schema_version": 1, "title": "Test", "categories": [], "packages": [{
            "id": "tool", "name": "Tool", "version": "Последняя", "publisher": "Test", "description": "Test", "category": "utilities",
            "source": {"type": "winget", "package_id": "Vendor.Tool"}, "install": {"type": "winget"},
        }]}
        catalog.validate_catalog(document)
        document["packages"][0]["install"] = {"type": "exe", "silent_args": ["/S"]}
        with self.assertRaises(ValueError):
            catalog.validate_catalog(document)
        for detection in ({"commands": ["app.exe --run"]}, {"paths": [r"\\server\share\app.exe"]}, {"name_pattern": "unbounded.*"}):
            with self.assertRaises(ValueError):
                catalog.validate_detection(detection)

    def test_rejects_unsafe_paths_cycles_and_drive_folders(self):
        for value in ("../bad.exe", "C:/bad.exe", "a/../bad", "NUL.exe", "a:stream", "a./bad"):
            with self.assertRaises(ValueError):
                catalog.safe_relative(value)
        with self.assertRaises(ValueError):
            catalog.validate_graph({"a": ["b"], "b": ["a"]})
        with self.assertRaises(ValueError):
            catalog.drive_id("https://drive.google.com/drive/folders/1234567890")


if __name__ == "__main__":
    unittest.main()
