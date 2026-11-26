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
        self.assertEqual(len(commands), 6)
        self.assertEqual(commands[0], ["cargo", "+1.100.0", "fmt", "--all", "--check"])
        for command in commands[2:]:
            self.assertEqual(command[:2], ["cargo", "+1.100.0"])
            self.assertIn("--locked", command)
        self.assertEqual(commands[3][-2:], ["-D", "warnings"])
        self.assertEqual(preflight.commands(channel, True), commands[:2])

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
