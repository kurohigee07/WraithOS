Wraith Orchestrator — Termux Package
=====================================

Quick start:
  cd ~/wraith                        # after extract here
  ./run.sh                           # start orchestrator
  ./status.sh                        # check status
  ./stop.sh                          # stop orchestrator

Access from phone:
  Browser:   http://127.0.0.1:9090/ui
  Healthz:   http://127.0.0.1:9090/healthz
  Metrics:   http://127.0.0.1:9090/metrics

Add Wasm modules:
  curl -X POST http://127.0.0.1:9090/modules \
    -H "X-Module-Id: my-mod" \
    --data-binary @path/to/module.wasm

Config: orchestrator.toml
Logs:   logs/wraith.log
DB:     orchestrator.db (SQLite)

Note:
- Orchestrator runs on 127.0.0.1:9090 (localhost only)
- To access from LAN: edit metrics_bind in orchestrator.toml
  to "0.0.0.0:9090" (security: anyone on wifi can access)
