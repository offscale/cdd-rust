#!/bin/bash
set -e
SPEC_FILE=$1
SERVER_DIR=$2
CLIENT_DIR=$3

if [ -z "$SPEC_FILE" ] || [ -z "$SERVER_DIR" ] || [ -z "$CLIENT_DIR" ]; then 
  echo "Usage: test_server.sh <spec> <server_out> <client_out>"
  exit 1
fi

PORT=`python3 -c "import random; print(random.randint(8000, 9000))"`

cleanup() { 
    if [ -n "$SERVER_PID" ]; then
        kill $SERVER_PID || true
    fi
}
trap cleanup EXIT

# 1. Generate Server
rm -rf $SERVER_DIR
cargo run -p cdd-cli -- from_openapi to_server -i $SPEC_FILE -o $SERVER_DIR >/dev/null 2>&1
echo "[workspace]" >> $SERVER_DIR/Cargo.toml

cd $SERVER_DIR
cargo build >/dev/null 2>&1
CDD_WEB_BIND="127.0.0.1:$PORT" cargo run -- --ephemeral &
SERVER_PID=$!
cd - >/dev/null

sleep 5

# 2. Generate Client
rm -rf $CLIENT_DIR
cargo run -p cdd-cli -- from_openapi to_sdk -i $SPEC_FILE -o $CLIENT_DIR --target client-reqwest --tests >/dev/null 2>&1
echo "[workspace]" >> $CLIENT_DIR/Cargo.toml
if [[ "$OSTYPE" == "darwin"* ]]; then
  sed -i "" "s/localhost:8080/127.0.0.1:$PORT/g" $CLIENT_DIR/tests/api_contracts.rs
else
  sed -i "s/localhost:8080/127.0.0.1:$PORT/g" $CLIENT_DIR/tests/api_contracts.rs
fi

cd $CLIENT_DIR
if ! cargo test >/dev/null 2>&1; then 
    echo "Test failed for $SPEC_FILE against generated server"
    cargo test
    exit 1
fi
cd - >/dev/null
