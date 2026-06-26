#!/usr/bin/env python3
import subprocess
import os
import re

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

def main():
    print("Calculating test coverage...")
    test_cov = "0.00"
    try:
        result = subprocess.run(["cargo", "tarpaulin", "--workspace"], capture_output=True, text=True)
        # Parse test coverage
        output = result.stdout + result.stderr
        match = re.search(r'(\d+\.\d+)%\s*coverage', output)
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
        # Check nightly
        subprocess.run(["cargo", "+nightly", "--version"], capture_output=True, check=True)
        
        # Get packages
        result = subprocess.run(["cargo", "tree", "--workspace", "--depth", "0"], capture_output=True, text=True, check=True)
        packages = []
        for line in result.stdout.splitlines():
            if ' v' in line:
                packages.append(line.split(' v')[0])
                
        total_docs = 0
        documented = 0
        
        for pkg in packages:
            res = subprocess.run(
                ["cargo", "+nightly", "rustdoc", "-p", pkg, "--", "-Z", "unstable-options", "--show-coverage", "--output-format", "json"],
                capture_output=True, text=True
            )
            # Find "total": X and "with_docs": Y
            out = res.stdout + res.stderr
            total_matches = re.findall(r'"total":\s*(\d+)', out)
            docs_matches = re.findall(r'"with_docs":\s*(\d+)', out)
            
            for m in total_matches:
                total_docs += int(m)
            for m in docs_matches:
                documented += int(m)
                
        if total_docs > 0:
            doc_cov = f"{(documented * 100.0 / total_docs):.2f}"
            
    except (FileNotFoundError, subprocess.CalledProcessError):
        print("Warning: nightly toolchain is not installed or error calculating doc coverage. Doc coverage will be 0.00%.")

    print(f"Doc coverage: {doc_cov}%")

    test_color = get_color(test_cov)
    doc_color = get_color(doc_cov)

    if os.path.exists("README.md"):
        print("Updating README.md badges...")
        with open("README.md", "r") as f:
            content = f.read()
            
        content = re.sub(r'badge/test_coverage-[\d\.]+(?:%|%25)-([a-zA-Z]+)', f'badge/test_coverage-{test_cov}%25-{test_color}', content)
        content = re.sub(r'badge/doc_coverage-[\d\.]+(?:%|%25)-([a-zA-Z]+)', f'badge/doc_coverage-{doc_cov}%25-{doc_color}', content)
        
        with open("README.md", "w") as f:
            f.write(content)

if __name__ == "__main__":
    main()