# Resource limits for interactive shells (DoS mitigation).
# Applies to login shells only; orchestrator limits set in S99wraith-init.
ulimit -u 400 2>/dev/null || true   # max user processes
ulimit -n 4096 2>/dev/null || true  # max open files
ulimit -c 0 2>/dev/null || true     # no core dumps
