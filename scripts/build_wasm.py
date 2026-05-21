import subprocess
import sys

def main():
    print("Adding wasm32-wasip1 target...")
    try:
        subprocess.run("rustup target add wasm32-wasip1", shell=True, check=True)
    except subprocess.CalledProcessError as e:
        print(f"Failed to add target: {e}")
        sys.exit(1)

    print("Building WASM target...")
    try:
        subprocess.run("cargo build -p cdd-cli --release --target wasm32-wasip1 --no-default-features", shell=True, check=True)
    except subprocess.CalledProcessError as e:
        print(f"WASM build failed: {e}")
        # Make.bat had a fallback, we'll just print a warning and exit 0 or 1 depending on strictness.
        # Let's exit 1 so pre-commit correctly reports failure.
        sys.exit(1)

if __name__ == "__main__":
    main()
