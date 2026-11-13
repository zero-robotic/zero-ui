#!/usr/bin/env python3
"""Enforce internal edges, including optional/dev/build/target dependencies.

Use Cargo metadata rather than source-text matching: aliases and optional edges
are checked even when a particular feature is not selected.
"""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
ALLOWED = {
    "zui-opengl": {"zui-types", "zui-api", "zui-paint", "zui-core", "zui-layout", "zui-text", "zui-render-cpu"},
    "zui-opengl-example": {"zui-opengl", "zui-app", "zui-api", "zui-types", "zui-platform", "zui-paint", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-render-cpu"},
    "zui-controls-example": {"zui-core", "zui-app", "zui-api", "zui-types", "zui-platform", "zui-paint", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-clipboard"},
    "zui-dashboard-example": {"zui-core", "zui-app", "zui-api", "zui-types", "zui-platform", "zui-paint", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-clipboard", "zui-extras"},

    "zui-types": set(),
    "zui-image": {"zui-types", "zui-paint"},
    "zui-extras": {"zui-paint", "zui-api", "zui-types", "zui-theme", "zui-widgets"},
    "zui-theme": {"zui-types"},
    "zui-input": {"zui-types"},
    "zui-edit": set(),
    "zui-virtual": set(),
    "zui-clipboard": {"zui-api"},
    "zui-api": {"zui-types", "zui-paint", "zui-theme", "zui-input"},
    "zui-layout": {"zui-types", "zui-api"},
    "zui-text": {"zui-types", "zui-api", "zui-paint"},
    "zui-widgets": {"zui-types", "zui-api", "zui-paint", "zui-theme", "zui-animation", "zui-edit", "zui-virtual"},
    "zui-core": {"zui-types", "zui-api", "zui-paint", "zui-platform", "zui-theme", "zui-animation"},
    "zui-paint": {"zui-types"},
    "zui-vello": {"zui-types", "zui-paint"},
    "zui-render-cpu": {"zui-types", "zui-paint", "zui-vello"},
    "zui-render-gpu": {"zui-types", "zui-paint", "zui-vello"},
    "zui-platform": {"zui-types", "zui-paint", "zui-input"},
    "zui-backend-winit": {"zui-types", "zui-paint", "zui-platform"},
    "zui-app": {
        "zui-api", "zui-core",
        "zui-types", "zui-paint", "zui-platform", "zui-backend-winit",
        "zui-render-cpu", "zui-render-gpu",
    },
    "zui-window-check": {"zui-render-cpu", "zui-render-gpu", "zui-image", "zui-extras", "zui-core", "zui-types", "zui-paint", "zui-platform", "zui-app", "zui-api", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-clipboard"},
    "zui-render-check": {"zui-image", "zui-extras", "zui-types", "zui-paint", "zui-api", "zui-layout", "zui-core", "zui-text", "zui-widgets", "zui-theme", "zui-render-cpu", "zui-render-gpu", "zui-custom-widget-example"},
    "zui-custom-widget-example": {"zui-api", "zui-paint", "zui-types"},
}


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"], cwd=ROOT
    ))
    members = set(metadata["workspace_members"])
    packages = {p["name"]: p for p in metadata["packages"] if p["id"] in members}
    failures = []
    graph = {}
    for name, package in packages.items():
        if name not in ALLOWED:
            failures.append(f"{name}: missing dependency policy")
        edges = {d["name"] for d in package["dependencies"] if d["name"] in packages}
        graph[name] = edges
        for dependency in edges - ALLOWED.get(name, set()):
            failures.append(f"forbidden edge: {name} -> {dependency}")
    visiting, done = set(), set()

    def visit(name):
        if name in visiting:
            failures.append(f"dependency cycle through {name}")
            return
        if name in done:
            return
        visiting.add(name)
        for dependency in graph[name]:
            visit(dependency)
        visiting.remove(name)
        done.add(name)

    for name in graph:
        visit(name)
    # Inspect selected transitive dependencies too: an isolated CPU application
    # must not acquire a GPU stack through a renamed or external dependency.
    for feature, forbidden in (
        ("cpu", {"wgpu", "wgpu-core", "wgpu-hal", "vello_gpu", "zui-render-gpu"}),
        ("gpu", {"vello_cpu", "zui-render-cpu"}),
    ):
        for application in ("zui-render-check", "zui-window-check", "zui-controls-example", "zui-dashboard-example"):
            tree = subprocess.check_output([
                "cargo", "tree", "--offline", "--locked", "-p", application,
                "--no-default-features", "--features", feature,
                "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
            ], cwd=ROOT, text=True)
            selected = {line.split()[0] for line in tree.splitlines() if line.strip()}
            for dependency in selected & forbidden:
                failures.append(f"{application} {feature}-only build unexpectedly depends on {dependency}")
    for features, forbidden in (
        ([], {"zui-api", "zui-types", "zui-paint", "zui-theme", "zui-widgets"}),
        (["--features", "charts"], {"zui-widgets", "zui-core", "zui-app"}),
        (["--features", "dashboard"], {"zui-core", "zui-app", "zui-render-cpu", "zui-render-gpu"}),
    ):
        tree = subprocess.check_output([
            "cargo", "tree", "--offline", "--locked", "-p", "zui-extras",
            "--no-default-features", *features,
            "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
        ], cwd=ROOT, text=True)
        selected = {line.split()[0] for line in tree.splitlines() if line.strip()}
        for dependency in selected & forbidden:
            failures.append(f"zui-extras {features or 'empty'} unexpectedly depends on {dependency}")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"Dependency policy passed: {len(packages)} packages; internal edges and CPU/GPU isolation checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
