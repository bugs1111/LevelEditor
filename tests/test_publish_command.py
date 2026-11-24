"""Exercise the release version policy and tag push without network access."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("publish", ROOT / "scripts/publish.py")
publish = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publish)


class PublishTests(unittest.TestCase):
    def test_automatic_patch_uses_the_higher_baseline(self):
        self.assertEqual(publish.select_version(None, "0.1.1", {"v0.1.0"}, set()), "0.1.2")
        self.assertEqual(publish.select_version(None, "0.1.1", {"v0.2.9"}, set()), "0.2.10")

    def test_manual_version_can_match_unpublished_source_or_be_higher(self):
        for version in ("0.1.1", "0.2.0"):
            self.assertEqual(publish.select_version(version, "0.1.1", {"v0.1.0"}, set()), version)

    def test_rejects_invalid_old_and_existing_versions(self):
        for version in ("v0.2.0", "0.2", "0.02.0", "0.2.0-rc.1", "0.2.0+build", "0.1.0"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                publish.select_version(version, "0.1.1", {"v0.1.0"}, set())
        for version in ("0.2.0", "0.2.2", "0.3.0"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                publish.select_version(version, "0.1.1", {"v0.2.2"}, {"v0.3.0"})

    def test_updates_cargo_versions_and_pushes_one_tag_at_main_commit(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            manifest = root / "Cargo.toml"
            manifest.write_text('[package]\nname = "LevelEditor"\nversion = "0.1.1"\n')
            lock = root / "Cargo.lock"
            lock.write_text('version = 4\n\n[[package]]\nname = "LevelEditor"\nversion = "0.1.1"\n\n[[package]]\nname = "other"\nversion = "0.1.1"\n')

            def capture(*command):
                if command[:3] == ("git", "remote", "get-url"):
                    return "https://github.com/bugs1111/LevelEditor.git"
                if command[:2] == ("gh", "api"):
                    return "v0.1.0\tfalse\tfalse\nv0.4.0\ttrue\tfalse\nv1.0.0-rc.1\tfalse\ttrue"
                if command[:2] in (("git", "status"), ("git", "ls-remote")):
                    return ""
                self.fail(f"Unexpected command: {command}")

            with patch.object(publish, "ROOT", root), patch.object(publish, "capture", side_effect=capture), patch.object(publish, "run") as run:
                publish.publish(None)
            self.assertIn('version = "0.1.2"', manifest.read_text())
            self.assertIn('name = "LevelEditor"\nversion = "0.1.2"', lock.read_text())
            self.assertIn('name = "other"\nversion = "0.1.1"', lock.read_text())
            pushes = [call.args for call in run.call_args_list if call.args[:2] == ("git", "push")]
            self.assertEqual(pushes, [("git", "push", "--atomic", "origin", "HEAD:refs/heads/main", "HEAD:refs/tags/v0.1.2")])


if __name__ == "__main__":
    unittest.main()
