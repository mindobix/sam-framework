#!/bin/bash
# SAM Start — launches all SAM services for a repo
# Usage: ./sam-start.sh [repo-path]

set -e

REPO="${1:-$(pwd)}"
SAM_DIR="$(cd "$(dirname "$0")" && pwd)"
UV="/usr/local/Cellar/uv/0.11.6/bin/uv"

if [ ! -d "$REPO/.sam" ]; then
    echo "Error: $REPO is not a SAM workspace (.sam/ not found)"
    echo "Run: sam init <url> first"
    exit 1
fi

echo "SAM Start — $REPO"
echo ""

# Kill any existing SAM processes for this repo
pkill -f "sam.*watch.*$REPO" 2>/dev/null || true
pkill -f "monograph serve" 2>/dev/null || true
sleep 1

# 1. Start MonoGraph
echo "Starting MonoGraph..."
cd "$SAM_DIR/monograph"
$UV run monograph serve --port 7474 > /dev/null 2>&1 &
MG_PID=$!
sleep 3

# 2. Build dependency graph
echo "Building dependency graph..."
curl -s -X POST http://127.0.0.1:7474/analyze \
  -H "Content-Type: application/json" \
  -d "{\"repo_path\": \"$REPO\"}" > /dev/null 2>&1
sleep 2

# 3. Setup ghost folders
echo "Setting up Finder integration..."
sam --repo "$REPO" setup 2>&1 | grep -E "✓|ℹ"

# Wait for Finder restart
sleep 3

# 4. Start watch daemon
echo "Starting watch daemon..."
sam --repo "$REPO" watch > /dev/null 2>&1 &
WATCH_PID=$!

echo ""
echo "✓ SAM is running"
echo "  MonoGraph:  PID $MG_PID (port 7474)"
echo "  Watch:      PID $WATCH_PID"
echo "  Repo:       $REPO"
echo ""
echo "Open Finder and double-click any dimmed folder to hydrate."
echo "To dehydrate: sam dehydrate <domain>"
echo "To stop:      ./sam-stop.sh"

open "$REPO"
