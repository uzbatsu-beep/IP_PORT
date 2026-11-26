"""Regression tests for preflight; no Rust processes or network are started."""
import subprocess
import unittest
from unittest.mock import patch

import preflight


class PreflightTests(unittest.TestCase):
    def test_pinned_channel_and_locked_commands(self):
        channel = preflight.pinned_channel()
        self.assertEqual(channel, "1.100.0")
        commands = preflight.commands(channel)
        self.assertEqual(len(commands), 9)
        self.assertEqual(commands[0], ["cargo", "+1.100.0", "fmt", "--all", "--check"])
        for command in commands[2:]:
            if command[0] == "cargo":
                self.assertEqual(command[:2], ["cargo", "+1.100.0"])
                self.assertIn("--locked", command)
        self.assertIn([preflight.sys.executable, "scripts/performance.py"], commands)
        self.assertEqual(commands[3][-2:], ["-D", "warnings"])
        self.assertEqual(preflight.commands(channel, True), commands[:2])

    def test_isolated_performance_commands(self):
        checks = preflight.test_commands("1.100.0")
        self.assertEqual(len(checks), 3)
        for name, command in zip(preflight.PERFORMANCE_TESTS[:2], checks[1:]):
            self.assertIn(name, checks[0])
            self.assertIn(name, command)
            self.assertIn("--exact", command)
            self.assertIn("--test-threads=1", command)
            self.assertIn("--nocapture", command)

    @patch("preflight.subprocess.run")
    def test_missing_performance_test_fails_closed(self, run):
        run.return_value.stdout = "unrelated: test\n"
        self.assertFalse(preflight.verify_performance_tests("1.100.0"))

    @patch("preflight.subprocess.run")
    def test_required_performance_tests_discovered(self, run):
        run.return_value.stdout = "\n".join(f"{name}: test" for name in preflight.PERFORMANCE_TESTS)
        self.assertTrue(preflight.verify_performance_tests("1.100.0"))

    @patch("preflight.subprocess.run")
    def test_failure_stops_pipeline(self, run):
        run.side_effect = subprocess.CalledProcessError(7, ["cargo"])
        self.assertEqual(preflight.execute([["cargo"], ["never"]]), 7)
        run.assert_called_once_with(["cargo"], cwd=preflight.ROOT, check=True)

    @patch("preflight.subprocess.run", side_effect=FileNotFoundError("missing executable"))
    def test_missing_executable_fails(self, run):
        self.assertEqual(preflight.execute([["missing"]]), 1)
        run.assert_called_once()

    @patch("preflight.subprocess.run")
    def test_success_runs_all(self, run):
        self.assertEqual(preflight.execute([["one"], ["two"]]), 0)
        self.assertEqual(run.call_count, 2)


if __name__ == "__main__":
    unittest.main()
