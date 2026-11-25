"""Exercise release signing orchestration with simulated macOS tools."""
import base64
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/notarize-macos"


class MacNotarizationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.app = self.root / "Level Editor.app"
        self.app.mkdir()
        self.tools = self.root / "tools"
        self.tools.mkdir()
        self.log = self.root / "calls.log"
        self.env = {
            **os.environ,
            "PATH": f"{self.tools}{os.pathsep}{os.environ['PATH']}",
            "RUNNER_TEMP": str(self.root),
            "NOTARY_TEST_LOG": str(self.log),
            "NOTARY_TEST_STATUS": "Accepted",
            "MACOS_CERTIFICATE_BASE64": base64.b64encode(b"fixture certificate").decode(),
            "MACOS_CERTIFICATE_PASSWORD": "fixture-certificate-password",
            "APPLE_ID": "fixture@example.invalid",
            "APPLE_TEAM_ID": "FIXTURE123",
            "APPLE_NOTARY_PASSWORD": "fixture-notary-password",
        }
        fake = self.tools / "fake-tool"
        fake.write_text(f"#!{sys.executable}\n" + '''
import json, os, sys
from pathlib import Path

tool, *args = sys.argv
tool = Path(tool).name
name = tool
if tool == "security":
    name += " " + args[0]
    if args[0] == "list-keychains" and "-s" in args:
        name += " -s"
elif tool == "xcrun":
    name += " " + " ".join(args[:2])
elif tool == "codesign":
    name += " verify" if "--verify" in args else " sign"

with Path(os.environ["NOTARY_TEST_LOG"]).open("a") as log:
    print(name, file=log)

if name == "security list-keychains":
    print('    "/fixture/Runner Login.keychain-db"')
elif name == "security list-keychains -s":
    assert args[-1] == "/fixture/Runner Login.keychain-db"
elif name == "security find-identity":
    print('  1) ' + 'A' * 40 + ' "Developer ID Application: Fixture (FIXTURE123)"')
elif name == "codesign sign":
    calls = Path(os.environ["NOTARY_TEST_LOG"]).read_text().splitlines()
    assert "security list-keychains -s" in calls
    assert "--timestamp" in args
    assert args[args.index("--options") + 1] == "runtime"
    if os.environ.get("NOTARY_TEST_SIGN_ERROR"):
        sys.exit(os.environ["NOTARY_TEST_SIGN_ERROR"])
    if calls.count("codesign sign") <= int(os.environ.get("NOTARY_TEST_TIMESTAMP_FAILURES", "0")):
        sys.exit("Level Editor.app: A timestamp was expected but was not found.")
elif name == "xcrun notarytool submit":
    assert "--wait" in args
    print(json.dumps({"id": "fixture-id", "status": os.environ["NOTARY_TEST_STATUS"]}))
''')
        fake.chmod(0o755)
        for name in ("security", "codesign", "ditto", "xcrun", "sleep"):
            (self.tools / name).symlink_to(fake)

    def run_script(self):
        return subprocess.run(
            ["bash", SCRIPT, self.app], env=self.env, text=True, capture_output=True, timeout=10
        )

    def test_accepted_submission_is_stapled_and_verified(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text().splitlines()
        self.assertLess(calls.index("codesign sign"), calls.index("xcrun notarytool submit"))
        self.assertLess(calls.index("xcrun notarytool submit"), calls.index("xcrun stapler staple"))
        self.assertLess(calls.index("xcrun stapler staple"), calls.index("xcrun stapler validate"))
        self.assertEqual(calls[-1], "security delete-keychain")

    def test_rejected_submission_does_not_staple(self):
        self.env["NOTARY_TEST_STATUS"] = "Invalid"
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        calls = self.log.read_text().splitlines()
        self.assertIn("xcrun notarytool log", calls)
        self.assertNotIn("xcrun stapler staple", calls)
        self.assertEqual(calls[-1], "security delete-keychain")

    def test_timestamp_failure_retries_signing_before_submitting(self):
        self.env["NOTARY_TEST_TIMESTAMP_FAILURES"] = "2"
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text().splitlines()
        self.assertEqual(calls.count("codesign sign"), 3)
        self.assertEqual(calls.count("sleep"), 2)
        self.assertEqual(calls.count("xcrun notarytool submit"), 1)
        self.assertLess(calls.index("codesign verify"), calls.index("xcrun notarytool submit"))

    def test_persistent_timestamp_failure_stops_before_submission(self):
        self.env["NOTARY_TEST_TIMESTAMP_FAILURES"] = "3"
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        calls = self.log.read_text().splitlines()
        self.assertEqual(calls.count("codesign sign"), 3)
        self.assertNotIn("xcrun notarytool submit", calls)

    def test_other_signing_failure_stops_without_retry(self):
        self.env["NOTARY_TEST_SIGN_ERROR"] = "Level Editor.app: no identity found"
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no identity found", result.stderr)
        calls = self.log.read_text().splitlines()
        self.assertEqual(calls.count("codesign sign"), 1)
        self.assertNotIn("xcrun notarytool submit", calls)

    def test_missing_secret_stops_before_signing(self):
        del self.env["APPLE_NOTARY_PASSWORD"]
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("APPLE_NOTARY_PASSWORD", result.stderr)
        self.assertFalse(self.log.exists())
