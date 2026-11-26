"""Validate live QA without imposing unmeasured timing thresholds."""
import math


def number(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def integer(value):
    return type(value) is int and value >= 0


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_live(report):
    require(report["schema_version"] == 1 and report["profile"] == "release", "Live report schema/profile")
    require(report["functional_gate_passed"] is True and report["timing_gate_required"] is False,
            "Live gate policy")
    scenarios = report["scenarios"]
    require(len(scenarios) == 11 and {s["id"] for s in scenarios} ==
            {f"LIVE-{i:02d}" for i in range(1, 12)}, "Missing/duplicate live scenarios")
    for s in scenarios:
        label = s["id"]
        for key in ("initial", "sent", "consumed", "stored", "dropped", "cleared", "max_queue", "max_shapes"):
            require(integer(s[key]), f"{label}: invalid {key}")
        require(s["sent"] > 0 and s["sent"] == s["consumed"], f"{label}: lost data")
        require(s["initial"] + s["sent"] == s["stored"] + s["dropped"] + s["cleared"],
                f"{label}: inconsistent storage accounting")
        require(s["stored"] <= 100000 and s["max_queue"] <= 4096 and s["max_shapes"] < 5000,
                f"{label}: exceeded bound")
        if label in ("LIVE-02", "LIVE-07", "LIVE-08"):
            require(s["max_queue"] == 4096, f"{label}: burst missing")
        if label in ("LIVE-07", "LIVE-11"):
            require(s["stored"] == 100000 and s["dropped"] > 0, f"{label}: saturation missing")
        else:
            require(s["dropped"] == 0, f"{label}: unexpected loss")
        if label in ("LIVE-08", "LIVE-11"):
            require(number(s["cancellation_ms"]), f"{label}: cancellation measurement missing")
        samples = s["samples"]
        require(0 < len(samples) <= 512, f"{label}: invalid sample count")
        for sample in samples:
            for key in ("frame_ms", "event_ms", "enrichment_ms", "projection_ms", "selection_lookup_ms"):
                require(number(sample[key]), f"{label}: invalid {key}")
            before, after = sample["queue_before"], sample["queue_after"]
            data, processed = sample["data_events"], sample["processed_events"]
            require(all(integer(v) for v in (before, after, data, processed))
                    and 0 <= after <= before <= 4096 and before - after == data
                    and data <= processed <= 1024 and processed - data <= 1,
                    f"{label}: inconsistent queue")
        require(sum(x["data_events"] for x in samples) == s["sent"], f"{label}: sample accounting")
        require(max(x["queue_before"] for x in samples) == s["max_queue"], f"{label}: queue peak mismatch")
        frames = sorted(x["frame_ms"] for x in samples)
        mean = sum(frames) / len(frames)
        p95 = frames[(len(frames) * 95 + 99) // 100 - 1]
        require(s["within_target"] is (mean < 16.0 and p95 < 25.0), f"{label}: target mismatch")
        for key in ("mean_ms", "p50_ms", "p95_ms", "p99_ms", "max_ms"):
            require(number(s[key]), f"{label}: invalid summary {key}")
