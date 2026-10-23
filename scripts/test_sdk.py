#!/usr/bin/env python3
import sys
import os
import subprocess
import random
import time
import atexit
import shutil
import urllib.request
import urllib.error

server_process = None
container_id = None
port = 8080


def cleanup():
    global container_id
    if container_id:
        print(f"Stopping docker container {container_id}...")
        subprocess.run(
            ["docker", "rm", "-f", container_id],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        container_id = None


atexit.register(cleanup)


def detect_base_path(spec_file):
    try:
        with open(spec_file, "r", encoding="utf-8") as f:
            raw = f.read()
        if "/api/v3" in raw:
            return "/api/v3"
    except Exception:
        pass
    return "/v2"


def is_pingable(p, base_path="/v2"):
    try:
        req = urllib.request.Request(
            f"http://127.0.0.1:{p}{base_path}/swagger.json", method="GET"
        )
        with urllib.request.urlopen(req, timeout=5) as response:
            return response.status == 200
    except Exception:
        return False


def main():
    if len(sys.argv) < 3:
        print("Usage: test_sdk.py <spec> <output_dir>")
        sys.exit(1)

    spec_file = sys.argv[1]
    output_dir = sys.argv[2]
    base_path = detect_base_path(spec_file)

    global port
    global container_id

    if is_pingable(port, base_path):
        print(
            f"Reusing active swaggerapi/petstore instance on port {port} ({base_path})"
        )
    else:
        port = random.randint(8000, 8999)
        print(f"Starting swaggerapi/petstore on port {port} ({base_path})")
        # Start docker container
        try:
            res = subprocess.run(
                [
                    "docker",
                    "run",
                    "-d",
                    "-p",
                    f"{port}:8080",
                    "-e",
                    f"SWAGGER_BASE_PATH={base_path}",
                    "-e",
                    "JAVA_TOOL_OPTIONS=-XX:TieredStopAtLevel=1 -XX:+UseSerialGC",
                    "swaggerapi/petstore",
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            container_id = res.stdout.strip()

            # wait for Jetty inside the container to bind 0.0.0.0:8080 before hitting the port
            for i in range(60):
                logs = subprocess.run(
                    ["docker", "logs", container_id],
                    capture_output=True,
                    text=True,
                )
                combined_logs = (logs.stdout or "") + (logs.stderr or "")
                if i in (6, 12, 18) and "Logging initialized" not in combined_logs:
                    subprocess.run(
                        ["docker", "restart", container_id],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                    )
                if "Started ServerConnector" in combined_logs and is_pingable(
                    port, base_path
                ):
                    break
                time.sleep(1)
            else:
                print(
                    "Failed to start or connect to swaggerapi/petstore docker container"
                )
                sys.exit(1)
        except subprocess.CalledProcessError as e:
            print(f"Failed to start docker container: {e.stderr}")
            sys.exit(1)

    if os.path.exists(output_dir):
        shutil.rmtree(output_dir)

    subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "cdd-cli",
            "--",
            "from_openapi",
            "to_sdk",
            "-i",
            spec_file,
            "-o",
            output_dir,
            "--tests",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=True,
    )

    with open(os.path.join(output_dir, "Cargo.toml"), "a") as f:
        f.write("\n[workspace]\n")

    api_contracts_path = os.path.join(output_dir, "tests", "api_contracts.rs")
    if os.path.exists(api_contracts_path):
        with open(api_contracts_path, "r") as f:
            content = f.read()
        # Replace base URL logic for petstore API
        # By default the petstore runs at /api/v3 or /v2
        content = content.replace("localhost:8080", f"127.0.0.1:{port}")
        with open(api_contracts_path, "w") as f:
            f.write(content)

    try:
        subprocess.run(
            ["cargo", "test"],
            cwd=output_dir,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
        )
    except subprocess.CalledProcessError:
        print(f"Test failed for {spec_file}")
        subprocess.run(["cargo", "test"], cwd=output_dir)
        sys.exit(1)


if __name__ == "__main__":
    main()
