#!/bin/bash
# Serve the HTML report on a local web server

PORT=${1:-8765}
REPORT_DIR="/tmp"

case "${2:-start}" in
    start)
        # Kill any existing server on this port
        pkill -f "python3 -m http.server $PORT" 2>/dev/null || true
        cd "$REPORT_DIR" && python3 -m http.server "$PORT" &
        echo "Server started at http://localhost:$PORT/tax-report.html"
        ;;
    stop)
        pkill -f "python3 -m http.server $PORT" 2>/dev/null
        echo "Server stopped"
        ;;
    *)
        echo "Usage: $0 [port] [start|stop]"
        echo "  Default port: 8765"
        exit 1
        ;;
esac
