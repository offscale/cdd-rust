#!/usr/bin/env python3
import sys
import os
import subprocess
import random
import time
import atexit
import shutil

server_process = None

def cleanup():
    global server_process
    if server_process:
        server_process.terminate()
        server_process.wait()

atexit.register(cleanup)

def main():
    if len(sys.argv) < 4:
        print("Usage: test_server.py <spec> <server_out> <client_out>")
        sys.exit(1)
        
    spec_file = sys.argv[1]
    server_dir = sys.argv[2]
    client_dir = sys.argv[3]
    
    port = random.randint(8000, 8999)
    
    # 1. Generate Server
    if os.path.exists(server_dir):
        shutil.rmtree(server_dir)
        
    subprocess.run(["cargo", "run", "-p", "cdd-cli", "--", "from_openapi", "to_server", "-i", spec_file, "-o", server_dir], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    
    with open(os.path.join(server_dir, "Cargo.toml"), "a") as f:
        f.write("\n[workspace]\n")
        
    subprocess.run(["cargo", "build"], cwd=server_dir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    
    env = os.environ.copy()
    env["CDD_WEB_BIND"] = f"127.0.0.1:{port}"
    
    global server_process
    server_process = subprocess.Popen(["cargo", "run", "--", "--ephemeral"], cwd=server_dir, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    
    time.sleep(5)
    
    # 2. Generate Client
    if os.path.exists(client_dir):
        shutil.rmtree(client_dir)
        
    subprocess.run(["cargo", "run", "-p", "cdd-cli", "--", "from_openapi", "to_sdk", "-i", spec_file, "-o", client_dir, "--target", "client-reqwest", "--tests"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    
    with open(os.path.join(client_dir, "Cargo.toml"), "a") as f:
        f.write("\n[workspace]\n")
        
    api_contracts_path = os.path.join(client_dir, "tests", "api_contracts.rs")
    if os.path.exists(api_contracts_path):
        with open(api_contracts_path, "r") as f:
            content = f.read()
        content = content.replace("localhost:8080", f"127.0.0.1:{port}")
        with open(api_contracts_path, "w") as f:
            f.write(content)
            
    try:
        subprocess.run(["cargo", "test"], cwd=client_dir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    except subprocess.CalledProcessError:
        print(f"Test failed for {spec_file} against generated server")
        subprocess.run(["cargo", "test"], cwd=client_dir)
        sys.exit(1)

if __name__ == "__main__":
    main()