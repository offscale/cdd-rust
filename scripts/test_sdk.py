#!/usr/bin/env python3
import sys
import os
import subprocess
import random
import time
import atexit
import shutil

server_process = None
mock_server_path = None

def cleanup():
    global server_process, mock_server_path
    if server_process:
        server_process.terminate()
        server_process.wait()
    if mock_server_path and os.path.exists(mock_server_path):
        os.remove(mock_server_path)

atexit.register(cleanup)

def main():
    if len(sys.argv) < 3:
        print("Usage: test_sdk.py <spec> <output_dir>")
        sys.exit(1)
        
    spec_file = sys.argv[1]
    output_dir = sys.argv[2]
    
    port = random.randint(8000, 8999)
    
    global mock_server_path
    mock_server_path = os.path.abspath(f"mock_server_{port}.py")
    
    mock_server_code = """
from http.server import HTTPServer, BaseHTTPRequestHandler
import sys
port = int(sys.argv[1])
class MockHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args): pass
    def do_GET(self): self.mock()
    def do_POST(self): self.mock()
    def do_PUT(self): self.mock()
    def do_DELETE(self): self.mock()
    def mock(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        path = self.path
        if "findByStatus" in path or "findByTags" in path or "createWithList" in path or "createWithArray" in path:
            if "pet" in path: self.wfile.write(b"[{\\"name\\": \\"mocked\\", \\"photoUrls\\": []}]")
            else: self.wfile.write(b"[]")
        else:
            if "pet" in path: self.wfile.write(b"{\\"name\\": \\"mocked\\", \\"photoUrls\\": []}")
            elif "user/login" in path: self.wfile.write(b"\\"token\\"")
            else: self.wfile.write(b"{}")
if __name__ == "__main__": HTTPServer(("", port), MockHandler).serve_forever()
"""
    with open(mock_server_path, "w") as f:
        f.write(mock_server_code)

    global server_process
    server_process = subprocess.Popen([sys.executable, mock_server_path, str(port)])
    
    time.sleep(2)
    
    if os.path.exists(output_dir):
        shutil.rmtree(output_dir)
        
    subprocess.run(["cargo", "run", "-p", "cdd-cli", "--", "from_openapi", "to_sdk", "-i", spec_file, "-o", output_dir, "--target", "client-reqwest", "--tests"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    
    with open(os.path.join(output_dir, "Cargo.toml"), "a") as f:
        f.write("\n[workspace]\n")
        
    api_contracts_path = os.path.join(output_dir, "tests", "api_contracts.rs")
    if os.path.exists(api_contracts_path):
        with open(api_contracts_path, "r") as f:
            content = f.read()
        content = content.replace("localhost:8080", f"127.0.0.1:{port}")
        with open(api_contracts_path, "w") as f:
            f.write(content)
            
    try:
        subprocess.run(["cargo", "test"], cwd=output_dir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    except subprocess.CalledProcessError:
        print(f"Test failed for {spec_file}")
        subprocess.run(["cargo", "test"], cwd=output_dir)
        sys.exit(1)

if __name__ == "__main__":
    main()