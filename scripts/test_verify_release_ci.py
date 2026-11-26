import unittest

from verify_release_ci import require_gate, select_run

SHA = "a" * 40


def run(**kwargs):
    return dict({"head_sha": SHA, "head_branch": "main", "event": "push", "id": 1,
                 "created_at": "2026-11-26T00:00:00Z", "status": "completed",
                 "conclusion": "success"}, **kwargs)


class ReleaseGateTests(unittest.TestCase):
    def test_exact_main_push(self):
        self.assertEqual(select_run([run()], SHA)["id"], 1)
        require_gate([{"name": "Quality Gate", "status": "completed", "conclusion": "success"}])

    def test_wrong_sha_or_pr(self):
        for runs in ([run(head_sha="b" * 40)], [run(event="pull_request")], [run(head_branch="other")]):
            with self.assertRaises(ValueError):
                select_run(runs, SHA)

    def test_newest_failed_run_blocks_old_success(self):
        with self.assertRaises(ValueError):
            select_run([run(), run(id=2, conclusion="failure")], SHA)

    def test_missing_skipped_or_failed_gate(self):
        for jobs in ([], [{"name": "Quality Gate", "status": "completed", "conclusion": "skipped"}],
                     [{"name": "Quality Gate", "status": "completed", "conclusion": "failure"}]):
            with self.assertRaises(ValueError):
                require_gate(jobs)
