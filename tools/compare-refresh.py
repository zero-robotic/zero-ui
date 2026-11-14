#!/usr/bin/env python3
"""Compare repeat runs, rejecting mismatched workloads or rendered output."""
import argparse
import csv
import math
import statistics
from pathlib import Path


FIELDS = ("scenario", "samples", "checksum", "p50_us", "p95_us", "update_us", "draw_us")
TIMINGS = ("p50_us", "p95_us", "update_us", "draw_us")


def read(path):
    with Path(path).open(encoding="utf-8-sig", newline="") as source:
        reader = csv.DictReader(source)
        headers = reader.fieldnames or []
        missing = set(FIELDS) - set(headers)
        if missing:
            raise ValueError(f"{path}: missing columns: {', '.join(sorted(missing))}")
        if len(headers) != len(set(headers)):
            raise ValueError(f"{path}: duplicate column names")
        runs = {}
        for row in reader:
            context = f"{path}:{reader.line_num}"
            if None in row or any(value is None for value in row.values()):
                raise ValueError(f"{context}: row does not match CSV header")
            name = row["scenario"].strip()
            if not name or name in runs:
                raise ValueError(f"{context}: empty or duplicate scenario: {name!r}")
            if not row["checksum"].strip():
                raise ValueError(f"{context}: empty checksum")
            try:
                row["samples"] = int(row["samples"])
                if row["samples"] <= 0:
                    raise ValueError
            except ValueError:
                raise ValueError(f"{context}: samples must be a positive integer") from None
            for field in TIMINGS:
                try:
                    value = float(row[field])
                    if not math.isfinite(value) or value < 0:
                        raise ValueError
                except ValueError:
                    raise ValueError(f"{context}: {field} must be finite and non-negative") from None
                row[field] = value
            if row["p95_us"] < row["p50_us"]:
                raise ValueError(f"{context}: p95_us is less than p50_us")
            runs[name] = row
        if not runs:
            raise ValueError(f"{path}: empty benchmark")
        return runs


def validate(before, after):
    if len(before) != len(after):
        raise ValueError("repeat counts differ")
    scenarios = list(before[0])
    for index, run in enumerate(before + after, start=1):
        if set(run) != set(scenarios):
            raise ValueError(f"run {index}: scenario sets differ")
        for name in scenarios:
            for field in ("samples", "checksum"):
                if run[name][field] != before[0][name][field]:
                    raise ValueError(f"run {index}, {name}: {field} differs")
    return scenarios


def reduction(before, after):
    return f"{(before - after) / before:.1%}" if before else "N/A"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", nargs="+", required=True)
    parser.add_argument("--after", nargs="+", required=True)
    args = parser.parse_args()
    try:
        before = [read(path) for path in args.before]
        after = [read(path) for path in args.after]
        scenarios = validate(before, after)
    except (OSError, ValueError, csv.Error) as error:
        parser.error(str(error))
    print(f"{len(before)} 轮统计取各轮分位数的中位数；所有场景逐帧像素校验和一致。单位：µs。\n")
    print("| 场景 | P50 优化前 | P50 优化后 | 耗时降低 | P95 优化前 | P95 优化后 |")
    print("|---|---:|---:|---:|---:|---:|")
    def median(runs, name, field):
        return statistics.median(float(run[name][field]) for run in runs)
    for name in scenarios:
        b, a = (median(runs, name, "p50_us") for runs in (before, after))
        bp, ap = (median(runs, name, "p95_us") for runs in (before, after))
        print(f"| {name} | {b:.1f} | {a:.1f} | {reduction(b, a)} | {bp:.1f} | {ap:.1f} |")
    print("\n刷新准备（update + draw，不含 CPU 光栅化），各轮均值的中位数：\n")
    print("| 场景 | 优化前 | 优化后 | 耗时降低 |")
    print("|---|---:|---:|---:|")
    for name in scenarios:
        b, a = (statistics.median(float(run[name]["update_us"]) + float(run[name]["draw_us"]) for run in runs) for runs in (before, after))
        print(f"| {name} | {b:.1f} | {a:.1f} | {reduction(b, a)} |")


if __name__ == "__main__":
    main()
