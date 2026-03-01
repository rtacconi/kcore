#!/bin/bash
# Monitor and auto-restart build on seccomp errors

LOG_FILE="/tmp/podman-build-v2.log"
BUILD_SCRIPT="./build-iso-podman.sh"
MAX_RESTARTS=5
RESTART_COUNT=0

check_build() {
    if [ ! -f "$LOG_FILE" ]; then
        return 1
    fi
    
    # Check for seccomp errors
    if tail -100 "$LOG_FILE" 2>/dev/null | grep -q "seccomp BPF program: Invalid argument"; then
        return 1
    fi
    
    # Check if build process is running
    if ! ps aux | grep -E "podman.*run.*nixos" | grep -v grep > /dev/null; then
        # Check if it exited successfully
        if tail -10 "$LOG_FILE" 2>/dev/null | grep -q "Build complete\|ISO successfully"; then
            return 0
        else
            return 1
        fi
    fi
    
    return 0
}

echo "🔍 Monitoring build for errors..."
echo "Log file: $LOG_FILE"
echo ""

while [ $RESTART_COUNT -lt $MAX_RESTARTS ]; do
    if check_build; then
        echo "✓ Build running normally"
        sleep 30
        continue
    fi
    
    # Error detected
    echo ""
    echo "❌ Error detected in build!"
    tail -10 "$LOG_FILE" 2>&1 | grep -E "error|Error|seccomp" | tail -3
    
    RESTART_COUNT=$((RESTART_COUNT + 1))
    echo ""
    echo "🔄 Restarting build (attempt $RESTART_COUNT/$MAX_RESTARTS)..."
    
    # Clean up
    pkill -f "build-iso-podman" 2>/dev/null
    sleep 2
    podman ps -aq | xargs -r podman rm -f 2>/dev/null
    
    # Restart
    "$BUILD_SCRIPT" >> "$LOG_FILE" 2>&1 &
    sleep 15
    
    # Check if it started successfully
    if ! ps aux | grep -E "podman.*run.*nixos" | grep -v grep > /dev/null; then
        echo "⚠️  Build didn't start, waiting..."
        sleep 10
    fi
done

echo ""
echo "❌ Max restarts reached. Build failed."

