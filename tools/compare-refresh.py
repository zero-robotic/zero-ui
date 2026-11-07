#!/usr/bin/env python3
"""Compare repeat runs, rejecting mismatched workloads or rendered output."""
import argparse
import csv
import statistics
from pathlib import Path


def read(path):
    with Path(path).open() as source:
        return {row["scenario"]: row for row in csv.DictReader(source)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", nargs="+", required=True)
    parser.add_argument("--after", nargs="+", required=True)
    args = parser.parse_args()
    before = [read(path) for path in args.before]
    after = [read(path) for path in args.after]
    assert len(before) == len(after), "repeat counts differ"
    scenarios = list(before[0])
    assert scenarios, "empty benchmark"
    for run in before + after:
        assert set(run) == set(scenarios), "scenario sets differ"
        for name in scenarios:
            for field in ("samples", "checksum"):
                assert run[name][field] == before[0][name][field], (name, field)
    print(f"{len(before)} 轮统计取各轮分位数的中位数；所有场景逐帧像素校验和一致。单位：µs。\n")
    print("| 场景 | P50 优化前 | P50 优化后 | 耗时降低 | P95 优化前 | P95 优化后 |")
    print("|---|---:|---:|---:|---:|---:|")
    def median(runs, name, field):
        return statistics.median(float(run[name][field]) for run in runs)
    for name in scenarios:
        b, a = (median(runs, name, "p50_us") for runs in (before, after))
        bp, ap = (median(runs, name, "p95_us") for runs in (before, after))
        print(f"| {name} | {b:.1f} | {a:.1f} | {(b-a)/b:.1%} | {bp:.1f} | {ap:.1f} |")
    print("\n刷新准备（update + draw，不含 CPU 光栅化），各轮均值的中位数：\n")
    print("| 场景 | 优化前 | 优化后 | 耗时降低 |")
    print("|---|---:|---:|---:|")
    for name in scenarios:
        b, a = (statistics.median(float(run[name]["update_us"]) + float(run[name]["draw_us"]) for run in runs) for runs in (before, after))
        print(f"| {name} | {b:.1f} | {a:.1f} | {(b-a)/b:.1%} |")


if __name__ == "__main__":
    main()
