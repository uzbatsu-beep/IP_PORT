import copy
import unittest

from performance import COUNTS, VIEWS, validate


def report():
    return {
        "schema_version": 1, "profile": "release", "gate_passed": True,
        "scenarios": [
            {"endpoints": n, "hosts": n // 10, "view": v, "frame_ms": [5.0] * 60,
             "warmup_ms": [100.0] * 5, "max_shapes": 100,
             "timing_gate_required": n == 20000, "within_target": True}
            for n in COUNTS for v in VIEWS
        ],
    }


class ReportTests(unittest.TestCase):
    def test_complete_report(self):
        validate(report())

    def test_missing_or_duplicate_scenario(self):
        data = report()
        data["scenarios"][-1] = copy.deepcopy(data["scenarios"][0])
        with self.assertRaises(ValueError):
            validate(data)

    def test_failed_20k_gate(self):
        data = report()
        item = next(s for s in data["scenarios"] if s["endpoints"] == 20000)
        item["frame_ms"] = [17.0] * 60
        item["within_target"] = False
        with self.assertRaises(ValueError):
            validate(data)

    def test_missing_frame(self):
        data = report()
        data["scenarios"][0]["frame_ms"].pop()
        with self.assertRaises(ValueError):
            validate(data)
