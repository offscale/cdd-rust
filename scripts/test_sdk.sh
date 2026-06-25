#!/bin/bash
set -e
SPEC_FILE=$1
OUTPUT_DIR=$2
if [ -z "$SPEC_FILE" ] || [ -z "$OUTPUT_DIR" ]; then exit 1; fi
PORT=`python3 -c "import random; print(random.randint(8000, 9000))"`
cleanup() { kill $SERVER_PID >/dev/null 2>&1 || true; rm -f $PWD/mock_server_$PORT.py; }
trap cleanup EXIT
cat << "PYEOF" > $PWD/mock_server_$PORT.py
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
            if "pet" in path: self.wfile.write(b"[{\"name\": \"mocked\", \"photoUrls\": []}]")
            else: self.wfile.write(b"[]")
        else:
            if "pet" in path: self.wfile.write(b"{\"name\": \"mocked\", \"photoUrls\": []}")
            elif "user/login" in path: self.wfile.write(b"\"token\"")
            else: self.wfile.write(b"{}")
if __name__ == "__main__": HTTPServer(("", port), MockHandler).serve_forever()
PYEOF
python3 $PWD/mock_server_$PORT.py $PORT &
SERVER_PID=$!
sleep 1
rm -rf $OUTPUT_DIR
cargo run -p cdd-cli -- from_openapi to_sdk -i $SPEC_FILE -o $OUTPUT_DIR --target client-reqwest --tests >/dev/null 2>&1
echo "[workspace]" >> $OUTPUT_DIR/Cargo.toml
sed -i "" "s/localhost:8080/localhost:$PORT/g" $OUTPUT_DIR/tests/api_contracts.rs
cd $OUTPUT_DIR
if ! cargo test >/dev/null 2>&1; then echo "Test failed for $SPEC_FILE"; cargo test; exit 1; fi
cd - >/dev/null
