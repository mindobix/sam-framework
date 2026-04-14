#!/bin/bash
# SAM Stop — kills all SAM background services

pkill -f "sam.*watch" 2>/dev/null || true
pkill -f "monograph serve" 2>/dev/null || true

echo "✓ SAM services stopped"
