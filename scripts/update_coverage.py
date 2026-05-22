import subprocess
import re
import os
import sys

def run_cmd(cmd, ignore_errors=False):
    try:
        result = subprocess.run(cmd, shell=True, text=True, capture_output=True, check=not ignore_errors)
        return result.stdout + result.stderr
    except subprocess.CalledProcessError as e:
        if ignore_errors:
            return e.stdout + e.stderr
        print(f"Command failed: {cmd}\n{e.stdout}\n{e.stderr}")
        return ""

def get_test_coverage():
    print("Calculating test coverage...")
    tarpaulin_check = run_cmd("cargo tarpaulin --version", ignore_errors=True)
    if "cargo-tarpaulin" not in tarpaulin_check:
        print("Warning: cargo-tarpaulin is not installed. Test coverage will be 0.00%.")
        return "0.00"
    
    output = run_cmd("cargo tarpaulin --workspace", ignore_errors=True)
    match = re.search(r'([0-9]+\.[0-9]+)% coverage', output)
    if match:
        return match.group(1)
    return "0.00"

def get_doc_coverage():
    print("Calculating doc coverage...")
    nightly_check = run_cmd("cargo +nightly --version", ignore_errors=True)
    if "cargo" not in nightly_check:
        print("Warning: nightly toolchain is not installed. Doc coverage will be 0.00%.")
        return "0.00"

    tree_output = run_cmd("cargo tree --workspace --depth 0", ignore_errors=True)
    packages = []
    for line in tree_output.splitlines():
        if re.match(r'^[a-zA-Z0-9_-]+ v[0-9.]+', line):
            pkg_name = line.split(' ')[0]
            packages.append(pkg_name)

    total_docs = 0
    documented = 0.0

    for pkg in packages:
        doc_output = run_cmd(f"cargo +nightly rustdoc -p {pkg} -- -Z unstable-options --show-coverage --output-format json", ignore_errors=True)
        # The JSON output is usually on the last line or mixed with Cargo output.
        # We can find the JSON object by looking for the line starting with "{"
        for line in doc_output.splitlines():
            if line.startswith("{") and line.endswith("}"):
                import json
                try:
                    data = json.loads(line)
                    for file_path, stats in data.items():
                        total_docs += stats.get("total", 0)
                        documented += stats.get("with_docs", 0)
                except Exception:
                    pass

    if total_docs == 0:
        return "0.00"
    
    return f"{(documented / total_docs * 100):.2f}"

def get_color(coverage):
    cov = float(coverage)
    if cov >= 90:
        return "brightgreen"
    elif cov >= 75:
        return "yellow"
    else:
        return "red"

def update_readme(test_cov, doc_cov, test_color, doc_color):
    if not os.path.exists("README.md"):
        return

    print("Updating README.md badges...")
    with open("README.md", "r") as f:
        content = f.read()

    content = re.sub(r'badge/test_coverage-[0-9.]+%25-[a-zA-Z]+', f'badge/test_coverage-{test_cov}%25-{test_color}', content)
    content = re.sub(r'badge/doc_coverage-[0-9.]+%25-[a-zA-Z]+', f'badge/doc_coverage-{doc_cov}%25-{doc_color}', content)

    with open("README.md", "w") as f:
        f.write(content)

    # Note: `pre-commit` will automatically handle staging if files are modified during a hook run.
    # So we don't need to manually `git add README.md`.

def main():
    test_cov = get_test_coverage()
    print(f"Test coverage: {test_cov}%")
    
    doc_cov = get_doc_coverage()
    print(f"Doc coverage: {doc_cov}%")

    test_color = get_color(test_cov)
    doc_color = get_color(doc_cov)

    update_readme(test_cov, doc_cov, test_color, doc_color)

if __name__ == "__main__":
    main()
