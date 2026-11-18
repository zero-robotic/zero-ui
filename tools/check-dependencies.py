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
    "minimal": {"zui-app", "zui-api", "zui-types", "zui-platform", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-core"},
    "settings": {"zui-app", "zui-api", "zui-types", "zui-platform", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-core"},
    "zui-opengl": {"zui-types", "zui-api", "zui-paint", "zui-core", "zui-layout", "zui-text", "zui-render-cpu"},
    "opengl": {"zui-opengl", "zui-app", "zui-api", "zui-types", "zui-platform", "zui-paint", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-render-cpu"},
    "widget-gallery": {"zui-core", "zui-app", "zui-api", "zui-types", "zui-platform", "zui-paint", "zui-layout", "zui-text", "zui-theme", "zui-widgets"},
    "dashboard": {"zui-core", "zui-app", "zui-api", "zui-types", "zui-platform", "zui-paint", "zui-layout", "zui-text", "zui-theme", "zui-widgets", "zui-extras"},

    "zui-types": set(),
    "zui-state": set(),
    "zui-image": {"zui-types", "zui-paint"},
    "zui-extras": {"zui-paint", "zui-api", "zui-types", "zui-theme", "zui-widgets"},
    "zui-theme": {"zui-types"},
    "zui-input": {"zui-types"},
    "zui-api": {"zui-types", "zui-paint", "zui-theme", "zui-input"},
    "zui-layout": {"zui-types", "zui-api"},
    "zui-text": {"zui-types", "zui-api", "zui-paint"},
    "zui-widgets": {"zui-types", "zui-api", "zui-paint", "zui-theme", "zui-animation"},
    "zui-core": {"zui-types", "zui-api", "zui-paint", "zui-platform", "zui-theme", "zui-animation"},
    "zui-paint": {"zui-types"},
    "zui-vello": {"zui-types", "zui-paint"},
    "zui-render-cpu": {"zui-types", "zui-paint", "zui-vello"},
    "zui-render-gpu": {"zui-types", "zui-paint", "zui-vello"},
    "zui-platform": {"zui-types", "zui-paint", "zui-input"},
    "zui-backend-winit": {"zui-types", "zui-paint", "zui-platform"},
    "zui-app": {
        "zui-api", "zui-core", "zui-state",
        "zui-types", "zui-paint", "zui-platform", "zui-backend-winit",
        "zui-render-cpu", "zui-render-gpu",
    },
    "zui-window-check": {"zui-render-cpu", "zui-render-gpu", "zui-image", "zui-extras", "zui-core", "zui-types", "zui-paint", "zui-platform", "zui-app", "zui-api", "zui-layout", "zui-text", "zui-theme", "zui-widgets"},
    "zui-render-check": {"zui-image", "zui-extras", "zui-types", "zui-paint", "zui-api", "zui-layout", "zui-core", "zui-text", "zui-widgets", "zui-theme", "zui-render-cpu", "zui-render-gpu", "custom-widget"},
    "custom-widget": {"zui-api", "zui-paint", "zui-types"},
}


def selected_dependencies(package, features=()):
    tree = subprocess.check_output([
        "cargo", "tree", "--offline", "--locked", "-p", package,
        "--no-default-features", *features,
        "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
    ], cwd=ROOT, text=True)
    return {line.split()[0] for line in tree.splitlines() if line.strip()}


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
        for application in ("minimal", "settings", "zui-render-check", "zui-window-check", "widget-gallery", "dashboard"):
            selected = selected_dependencies(application, ["--features", feature])
            for dependency in selected & forbidden:
                failures.append(f"{application} {feature}-only build unexpectedly depends on {dependency}")
    for features, forbidden in (
        ([], {"zui-api", "zui-types", "zui-paint", "zui-theme", "zui-widgets"}),
        (["--features", "charts"], {"zui-widgets", "zui-core", "zui-app"}),
        (["--features", "dashboard"], {"zui-core", "zui-app", "zui-render-cpu", "zui-render-gpu"}),
    ):
        selected = selected_dependencies("zui-extras", features)
        for dependency in selected & forbidden:
            failures.append(f"zui-extras {features or 'empty'} unexpectedly depends on {dependency}")
    for features, clipboard_expected in (
        ([], False),
        (["--features", "cpu"], False),
        (["--features", "gpu"], False),
        (["--features", "clipboard"], True),
        (["--features", "clipboard-wayland"], True),
    ):
        selected = selected_dependencies("zui-app", features)
        if ("arboard" in selected) != clipboard_expected:
            failures.append(f"zui-app {features or 'empty'} has unexpected clipboard dependencies")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"Dependency policy passed: {len(packages)} packages; internal edges and CPU/GPU isolation checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
