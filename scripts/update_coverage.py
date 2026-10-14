#!/usr/bin/env python3
import glob
import json
import os
import re
import subprocess
import sys


def get_color(cov):
    try:
        cov_float = float(cov)
        if cov_float >= 90:
            return "brightgreen"
        elif cov_float >= 75:
            return "yellow"
        else:
            return "red"
    except ValueError:
        return "red"


def check_no_coverage_cheats():
    forbidden = ["tarpaulin_include", "coverage(off)"]
    offenders = []
    for path in (
        glob.glob("core/src/**/*.rs", recursive=True)
        + glob.glob("cli/src/**/*.rs", recursive=True)
        + glob.glob("web/src/**/*.rs", recursive=True)
    ):
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        for token in forbidden:
            if token in content:
                offenders.append(f"{path} (contains '{token}')")
        if "/// Documented" in content:
            offenders.append(f"{path} (contains placeholder '/// Documented')")
    if offenders:
        print("Error: Coverage or documentation cheat markers detected:")
        for offender in offenders:
            print(f"  - {offender}")
        sys.exit(1)


def main():
    check_no_coverage_cheats()

    print("Calculating test coverage...")
    test_cov = "0.00"
    try:
        result = subprocess.run(
            ["cargo", "tarpaulin", "--workspace"], capture_output=True, text=True
        )
        output = result.stdout + result.stderr
        match = re.search(r"(\d+\.\d+)%\s*coverage", output)
        if match:
            test_cov = match.group(1)
    except FileNotFoundError:
        print("Warning: cargo-tarpaulin is not installed. Test coverage will be 0.00%.")
    except Exception as e:
        print(f"Error running tarpaulin: {e}")

    print(f"Test coverage: {test_cov}%")

    print("Calculating doc coverage...")
    doc_cov = "0.00"
    try:
        subprocess.run(["cargo", "+nightly", "--version"], capture_output=True, check=True)

        targets = [
            ("cdd-core", ["--lib"]),
            ("cdd-cli", ["--lib"]),
            ("cdd-cli", ["--bin", "cdd-rust"]),
            ("cdd-web", ["--lib"]),
            ("cdd-web", ["--bin", "cdd-web"]),
        ]

        total_docs = 0
        documented = 0

        for pkg, target_args in targets:
            res = subprocess.run(
                [
                    "cargo",
                    "+nightly",
                    "rustdoc",
                    "-p",
                    pkg,
                    *target_args,
                    "--",
                    "-Z",
                    "unstable-options",
                    "--show-coverage",
                    "--output-format",
                    "json",
                ],
                capture_output=True,
                text=True,
            )
            out = res.stdout + res.stderr
            match = re.search(r'Generated output into "([^"]+\.json)"', out)
            if match and os.path.exists(match.group(1)):
                with open(match.group(1), "r", encoding="utf-8") as jf:
                    data = json.load(jf)
                for file_stats in data.values():
                    total_docs += int(file_stats.get("total", 0))
                    documented += int(file_stats.get("with_docs", 0))

        if total_docs > 0:
            doc_cov = f"{(documented * 100.0 / total_docs):.2f}"
            if documented < total_docs:
                print(
                    f"Error: Doc coverage is {doc_cov}% ({documented}/{total_docs}), required 100.00%."
                )
                sys.exit(1)

    except (FileNotFoundError, subprocess.CalledProcessError) as e:
        print(
            f"Warning: nightly toolchain is not installed or error calculating doc coverage ({e}). Doc coverage will be 0.00%."
        )

    print(f"Doc coverage: {doc_cov}%")

    test_color = get_color(test_cov)
    doc_color = get_color(doc_cov)

    if os.path.exists("README.md"):
        print("Updating README.md badges...")
        with open("README.md", "r", encoding="utf-8") as f:
            content = f.read()

        content = re.sub(
            r"badge/test_coverage-[\d\.]+(?:%|%25)-([a-zA-Z]+)",
            f"badge/test_coverage-{test_cov}%25-{test_color}",
            content,
        )
        content = re.sub(
            r"badge/doc_coverage-[\d\.]+(?:%|%25)-([a-zA-Z]+)",
            f"badge/doc_coverage-{doc_cov}%25-{doc_color}",
            content,
        )

        with open("README.md", "w", encoding="utf-8") as f:
            f.write(content)


if __name__ == "__main__":
    main()
