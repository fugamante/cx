use super::CommandHelp;

pub const MAIN_COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        name: "version",
        usage: "version [--json]",
        description: "Print tool version",
    },
    CommandHelp {
        name: "contracts",
        usage: "contracts <export|validate> [--profile eval-lab|full] [--json]",
        description: "Export or validate stable machine-contract bundle metadata",
    },
    CommandHelp {
        name: "where",
        usage: "where",
        description: "Show binary/source/log resolution details",
    },
    CommandHelp {
        name: "routes",
        usage: "routes [--json] [cmd...]",
        description: "Show routing map/introspection",
    },
    CommandHelp {
        name: "diag",
        usage: "diag [--json|--text] [--window N] [--strict] [--actions] [--severity warning|critical]",
        description: "Non-interactive diagnostic report",
    },
    CommandHelp {
        name: "scheduler",
        usage: "scheduler [--json|--text] [--window N] [--strict] [--actions] [--severity warning|critical]",
        description: "Scheduler-focused diagnostics summary",
    },
    CommandHelp {
        name: "parity",
        usage: "parity",
        description: "Run Rust/Bash parity invariants",
    },
    CommandHelp {
        name: "schema",
        usage: "schema list [--json]",
        description: "List registered schemas",
    },
    CommandHelp {
        name: "logs",
        usage: "logs validate [--strict] [--legacy-ok]",
        description: "Validate execution log JSONL contract",
    },
    CommandHelp {
        name: "logs",
        usage: "logs migrate [--out PATH] [--in-place]",
        description: "Normalize legacy run logs to current contract",
    },
    CommandHelp {
        name: "logs",
        usage: "logs stats [N] [--json|--text] [--strict] [--severity]",
        description: "Telemetry health and contract-drift summary",
    },
    CommandHelp {
        name: "telemetry",
        usage: "telemetry [N] [--json|--text] [--strict] [--severity]",
        description: "Alias for 'logs stats'",
    },
    CommandHelp {
        name: "ci",
        usage: "ci validate [--strict] [--legacy-ok] [--json]",
        description: "CI-friendly validation gate (no network)",
    },
    CommandHelp {
        name: "core",
        usage: "core [--json]",
        description: "Show execution-core pipeline config",
    },
    CommandHelp {
        name: "mode",
        usage: "mode [show|explain] [--json] [--cli json|text] [--command-default json|text]",
        description: "Explain output-mode resolution (cli/env/state/auto/default)",
    },
    CommandHelp {
        name: "broker",
        usage: "broker <show [--json] | set --policy latency|quality|cost|balanced|quota_saver | benchmark [--backend primary|ollama|llamacpp|mlx]... [--window N] [--json] [--strict] [--min-runs N] [--severity warn|warning|critical]>",
        description: "Show/set broker policy and benchmark backend performance from local run logs",
    },
    CommandHelp {
        name: "launch",
        usage: "launch [--json] [--no-open] [--local|--remote] [--cxops-bin PATH]",
        description: "Start the cxops UI/server companion and open the dashboard",
    },
    CommandHelp {
        name: "task",
        usage: "task <op> [...]",
        description: "Task graph management (add/list/claim/complete/fail/show/fanout)",
    },
    CommandHelp {
        name: "doctor",
        usage: "doctor",
        description: "Run non-interactive environment checks",
    },
    CommandHelp {
        name: "supports",
        usage: "supports <name>",
        description: "Exit 0 if subcommand is supported by cxrs",
    },
    CommandHelp {
        name: "llm",
        usage: "llm <op> [...]",
        description: "Manage and verify LLM backend/model defaults (show|check|smoke|verify|resident show|resident probe-models|use|unset|set-backend|set-model|clear-model|models list|models add|models inspect|models remove)",
    },
    CommandHelp {
        name: "state",
        usage: "state <op> [...]",
        description: "Manage repo state JSON (show|get|set)",
    },
    CommandHelp {
        name: "policy",
        usage: "policy [show [--json]|check ...]",
        description: "Show safety rules or classify a command",
    },
    CommandHelp {
        name: "bench",
        usage: "bench <N> -- <cmd...>",
        description: "Benchmark command runtime and tokens",
    },
    CommandHelp {
        name: "cx",
        usage: "cx <cmd...>",
        description: "Compatibility LLM text-mode command; prefer cxo for explicit XSHELF output",
    },
    CommandHelp {
        name: "cxj",
        usage: "cxj <cmd...>",
        description: "Compatibility LLM JSONL command",
    },
    CommandHelp {
        name: "cxo",
        usage: "cxo <cmd...>",
        description: "Run command output and print last agent message",
    },
    CommandHelp {
        name: "cxol",
        usage: "cxol <cmd...>",
        description: "Compatibility LLM plain-output command",
    },
    CommandHelp {
        name: "cxcopy",
        usage: "cxcopy <cmd...>",
        description: "Compatibility helper that copies cxo output to clipboard",
    },
    CommandHelp {
        name: "fix",
        usage: "fix <cmd...>",
        description: "Explain failures and suggest next steps (text)",
    },
    CommandHelp {
        name: "budget",
        usage: "budget",
        description: "Show context budget settings and last clip fields",
    },
    CommandHelp {
        name: "log-tail",
        usage: "log-tail [N]",
        description: "Pretty-print last N log entries",
    },
    CommandHelp {
        name: "health",
        usage: "health",
        description: "Run end-to-end selected-LLM XSHELF/CX smoke checks",
    },
    CommandHelp {
        name: "capture-status",
        usage: "capture-status",
        description: "Show internal capture pipeline status",
    },
    CommandHelp {
        name: "log-on",
        usage: "log-on",
        description: "Enable XSHELF/CX logging (process-local)",
    },
    CommandHelp {
        name: "log-off",
        usage: "log-off",
        description: "Disable XSHELF/CX logging in this process",
    },
    CommandHelp {
        name: "alert-show",
        usage: "alert-show",
        description: "Show active alert thresholds/toggles",
    },
    CommandHelp {
        name: "alert-on",
        usage: "alert-on",
        description: "Enable alerts (process-local)",
    },
    CommandHelp {
        name: "alert-off",
        usage: "alert-off",
        description: "Disable alerts in this process",
    },
    CommandHelp {
        name: "chunk",
        usage: "chunk",
        description: "Chunk stdin text by context budget chars",
    },
    CommandHelp {
        name: "metrics",
        usage: "metrics [N]",
        description: "Token and duration aggregates from last N runs",
    },
    CommandHelp {
        name: "quota",
        usage: "quota [probe] [days] [--json] | quota catalog <show|refresh [--if-stale --max-age-hours N] [--json]|auto <show|on|off>> | quota set <backend|default> <total_tokens> | quota unset <backend|default|all> | quota guard <show|on|off|check>",
        description: "Token-burn, provider quota probe, and dynamic quota-guard warnings",
    },
    CommandHelp {
        name: "prompt-stats",
        usage: "prompt-stats [N] [--json]",
        description: "Prompt raw-vs-filtered efficiency stats from recent runs",
    },
    CommandHelp {
        name: "prompt",
        usage: "prompt <mode> <request>",
        description: "Generate agent-ready prompt block",
    },
    CommandHelp {
        name: "roles",
        usage: "roles [role]",
        description: "List roles or print role-specific prompt header",
    },
    CommandHelp {
        name: "fanout",
        usage: "fanout <objective>",
        description: "Generate role-tagged parallelizable subtasks",
    },
    CommandHelp {
        name: "promptlint",
        usage: "promptlint [N]",
        description: "Lint prompt/cost patterns from last N runs",
    },
    CommandHelp {
        name: "cx-compat",
        usage: "cx-compat <cmd...>",
        description: "Legacy compatibility shim for bash-style cx command names",
    },
    CommandHelp {
        name: "profile",
        usage: "profile [N]",
        description: "Summarize last N runs from resolved runtime log (default {RUN_WINDOW})",
    },
    CommandHelp {
        name: "alert",
        usage: "alert [N]",
        description: "Report anomalies from last N runs (default {RUN_WINDOW})",
    },
    CommandHelp {
        name: "optimize",
        usage: "optimize [N] [--json|--text] [--actions] [--strict] [--severity warning|critical]",
        description: "Recommend cost/latency improvements from last N runs",
    },
    CommandHelp {
        name: "worklog",
        usage: "worklog [N]",
        description: "Emit Markdown worklog from last N runs (default {RUN_WINDOW})",
    },
    CommandHelp {
        name: "trace",
        usage: "trace [N]",
        description: "Show Nth most-recent run from resolved runtime log (default 1)",
    },
    CommandHelp {
        name: "next",
        usage: "next <cmd...>",
        description: "Suggest next shell commands from command output (strict JSON)",
    },
    CommandHelp {
        name: "diffsum",
        usage: "diffsum",
        description: "Summarize unstaged diff (strict schema)",
    },
    CommandHelp {
        name: "diffsum-staged",
        usage: "diffsum-staged",
        description: "Summarize staged diff (strict schema)",
    },
    CommandHelp {
        name: "fix-run",
        usage: "fix-run <cmd...>",
        description: "Suggest remediation commands for a failed command",
    },
    CommandHelp {
        name: "commitjson",
        usage: "commitjson",
        description: "Generate strict JSON commit object from staged diff",
    },
    CommandHelp {
        name: "commitmsg",
        usage: "commitmsg",
        description: "Generate commit message text from staged diff",
    },
    CommandHelp {
        name: "replay",
        usage: "replay <id>",
        description: "Replay quarantined schema run in strict mode",
    },
    CommandHelp {
        name: "quarantine",
        usage: "quarantine list [N]",
        description: "Show recent quarantine entries (default {QUARANTINE_LIST})",
    },
    CommandHelp {
        name: "quarantine",
        usage: "quarantine show <id>",
        description: "Show quarantined entry payload",
    },
    CommandHelp {
        name: "help",
        usage: "help",
        description: "Print this help",
    },
];

pub const TASK_COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        name: "task add",
        usage: "{APP} task add \"<objective>\" [--role <architect|implementer|reviewer|tester|doc>] [--backend <auto|primary|ollama|llamacpp|mlx>] [--model <name>] [--profile <fast|balanced|quality|schema_strict>] [--converge <none|first_valid|majority|judge|score>] [--replicas <n>] [--max-concurrency <n>] [--mode <sequential|parallel>] [--depends-on <id1,id2>] [--resource <key>]",
        description: "Create a task with role, routing, and orchestration metadata",
    },
    CommandHelp {
        name: "task list",
        usage: "{APP} task list [--status pending|in_progress|complete|failed] [--json|--text]",
        description: "List tasks with optional status filter",
    },
    CommandHelp {
        name: "task claim",
        usage: "{APP} task claim <id>",
        description: "Mark task as in_progress",
    },
    CommandHelp {
        name: "task complete",
        usage: "{APP} task complete <id>",
        description: "Mark task as complete",
    },
    CommandHelp {
        name: "task fail",
        usage: "{APP} task fail <id>",
        description: "Mark task as failed",
    },
    CommandHelp {
        name: "task show",
        usage: "{APP} task show <id> | {APP} task show list [--status pending|in_progress|complete|failed] [--json|--text]",
        description: "Show one task record or route to list view",
    },
    CommandHelp {
        name: "task fanout",
        usage: "{APP} task fanout \"<objective>\" [--from staged-diff|worktree|log|file:PATH]",
        description: "Generate role-tagged subtasks",
    },
    CommandHelp {
        name: "task check",
        usage: "{APP} task check [--status pending|in_progress|complete|failed] [--strict-plan] [--json|--text]",
        description: "Preflight blocked tasks, strict-plan readiness, and recommended mode",
    },
    CommandHelp {
        name: "task sandbox",
        usage: "{APP} task sandbox <show|check|enable|disable|set-image|clear-image> [--json|--text|<image>]",
        description: "Inspect, preflight, or configure the repo-scoped Docker task sandbox",
    },
    CommandHelp {
        name: "task events",
        usage: "{APP} task events [--limit N] [--json|--jsonl] [--follow]",
        description: "Read task-events.v1 progress events emitted by run-all",
    },
    CommandHelp {
        name: "task run-plan",
        usage: "{APP} task run-plan [--status pending|in_progress|complete|failed] [--json]",
        description: "Preview deterministic execution waves before run-all",
    },
    CommandHelp {
        name: "task run",
        usage: "{APP} task run <id> [--mode lean|deterministic|verbose] [--backend primary|ollama|llamacpp|mlx] [--json|--text]",
        description: "Run one task objective",
    },
    CommandHelp {
        name: "task run-all",
        usage: "{APP} task run-all [--status pending] [--mode sequential|mixed|parallel] [--strict-plan] [--plan-json] [--dry-run] [--backend-pool primary,ollama,llamacpp,mlx] [--backend-cap backend=limit] [--max-workers N] [--fairness round_robin|least_loaded] [--halt-on-critical|--continue-on-critical] [--events-jsonl] [--summary text|json] [--json|--text]",
        description: "Run tasks by status (sequential default; mixed uses run-plan waves and broker-aware backend routing)",
    },
];
