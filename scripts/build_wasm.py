#!/usr/bin/env python3
import subprocess
import sys

def main():
    print("Adding wasm32-wasip1 target...")
    try:
        subprocess.run(["rustup", "target", "add", "wasm32-wasip1"], check=True)
    except subprocess.CalledProcessError:
        print("Failed to add target")
        sys.exit(1)

    print("Building WASM target...")
    try:
        subprocess.run(
            ["cargo", "build", "-p", "cdd-cli", "--release", "--target", "wasm32-wasip1", "--no-default-features"], 
            check=True
        )
    except subprocess.CalledProcessError:
        print("WASM build failed")
        sys.exit(1)

if __name__ == "__main__":
    main()