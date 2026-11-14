"""Exercise the CLI, including validation with Python assertions disabled."""
import csv
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "compare-refresh.py"
FIELDS = "scenario,samples,checksum,p50_us,p95_us,update_us,draw_us".split(",")


class CompareRefreshTests(unittest.TestCase):
    def run_comparison(self, before, after, fields=FIELDS):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ("before.csv", "after.csv")]
            for path, rows in zip(paths, (before, after)):
                with path.open("w", newline="", encoding="utf-8") as output:
                    writer = csv.writer(output)
                    writer.writerow(fields)
                    writer.writerows(rows)
            return subprocess.run(
                [sys.executable, "-O", str(SCRIPT), "--before", str(paths[0]),
                 "--after", str(paths[1])], text=True, capture_output=True,
            )

    def test_valid_comparison(self):
        result = self.run_comparison(
            [["scroll", 10, "abc", 20, 30, 2, 4]],
            [["scroll", 10, "abc", 10, 20, 1, 2]],
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("50.0%", result.stdout)

    def test_zero_baseline(self):
        result = self.run_comparison(
            [["scroll", 10, "abc", 0, 0, 0, 0]],
            [["scroll", 10, "abc", 1, 2, 0, 0]],
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("N/A", result.stdout)

    def test_invalid_inputs_fail_without_partial_report(self):
        valid = ["scroll", 10, "abc", 20, 30, 2, 4]
        cases = [([], "empty benchmark"), ([valid, valid], "duplicate scenario")]
        for column, value, message in (
            (0, "other", "scenario sets differ"),
            (1, 11, "samples differs"),
            (1, 0, "positive integer"),
            (2, "xyz", "checksum differs"),
            (2, "", "empty checksum"),
            (3, "nan", "finite"),
            (3, "inf", "finite"),
            (3, -1, "non-negative"),
            (4, 1, "less than"),
        ):
            row = valid.copy()
            row[column] = value
            cases.append(([row], message))
        cases.append(([valid[:-1]], "does not match"))
        for rows, message in cases:
            with self.subTest(message=message, rows=rows):
                result = self.run_comparison([valid], rows)
                self.assertEqual(result.returncode, 2)
                self.assertIn(message, result.stderr)
                self.assertNotIn("Traceback", result.stderr)
                self.assertEqual(result.stdout, "")

    def test_missing_columns(self):
        result = self.run_comparison([], [], fields=["scenario"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("missing columns", result.stderr)


if __name__ == "__main__":
    unittest.main()
