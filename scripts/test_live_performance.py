import unittest
from live_performance import validate_live


def fixture():
    scenarios = []
    for i in range(1, 12):
        sent = 4096 if i in (2, 7, 8) else 1
        initial = 100000 if i in (7, 11) else 0
        samples = []
        remaining = sent
        while remaining:
            data = min(1024, remaining)
            samples.append(dict(frame_ms=40., event_ms=1., enrichment_ms=0., projection_ms=2.,
                                selection_lookup_ms=0., queue_before=remaining, queue_after=remaining-data,
                                data_events=data, processed_events=data))
            remaining -= data
        scenarios.append(dict(id=f"LIVE-{i:02d}", initial=initial, sent=sent, consumed=sent,
                              stored=min(100000, initial+sent), dropped=max(0, initial+sent-100000), cleared=0,
                              max_queue=sent, max_shapes=100, mean_ms=40., p50_ms=40., p95_ms=40.,
                              p99_ms=40., max_ms=40., within_target=False, cancellation_ms=1. if i in (8,11) else None,
                              samples=samples))
    return dict(schema_version=1, profile="release", functional_gate_passed=True,
                timing_gate_required=False, scenarios=scenarios)


class LiveReportTests(unittest.TestCase):
    def test_slow_baseline_is_not_blocking(self):
        validate_live(fixture())

    def test_missing_scenario(self):
        data = fixture()
        data["scenarios"].pop()
        with self.assertRaises(ValueError):
            validate_live(data)

    def test_loss_and_queue_overflow(self):
        for key, value in (("consumed", 0), ("max_queue", 4097)):
            data = fixture()
            data["scenarios"][0][key] = value
            with self.assertRaises(ValueError):
                validate_live(data)

    def test_nonfinite_sample(self):
        data = fixture()
        data["scenarios"][0]["samples"][0]["frame_ms"] = float("nan")
        with self.assertRaises(ValueError):
            validate_live(data)
