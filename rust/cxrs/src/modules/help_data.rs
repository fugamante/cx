use super::CommandHelp;

pub const MAIN_COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        name: "version",
        usage: "version",
        description: "Print tool version",
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
        usage: "diag [--json]",
        description: "Non-interactive diagnostic report",
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
        usage: "logs validate [--fix=false] [--legacy-ok]",
        description: "Validate execution log JSONL contract",
    },
    CommandHelp {
        name: "logs",
        usage: "logs migrate [--out PATH] [--in-place]",
        description: "Normalize legacy run logs to current contract",
    },
    CommandHelp {
        name: "logs",
        usage: "logs stats [N] [--json] [--strict] [--severity]",
        description: "Telemetry health and contract-drift summary",
    },
    CommandHelp {
        name: "telemetry",
        usage: "telemetry [N] [--json] [--strict] [--severity]",
        description: "Alias for 'logs stats'",
    },
    CommandHelp {
        name: "ci",
        usage: "ci validate [--strict] [--legacy-ok] [--json]",
        description: "CI-friendly validation gate (no network)",
    },
    CommandHelp {
        name: "core",
        usage: "core",
        description: "Show execution-core pipeline config",
    },
    CommandHelp {
        name: "broker",
        usage: "broker show [--json]",
        description: "Show backend broker policy, active selection, and provider availability",
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
        description: "Manage LLM backend/model defaults (show|use|unset|set-backend|set-model|clear-model)",
    },
    CommandHelp {
        name: "state",
        usage: "state <op> [...]",
        description: "Manage repo state JSON (show|get|set)",
    },
    CommandHelp {
        name: "policy",
        usage: "policy [show|check ...]",
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
        description: "Run command output through LLM text mode",
    },
    CommandHelp {
        name: "cxj",
        usage: "cxj <cmd...>",
        description: "Run command output through LLM JSONL mode",
    },
    CommandHelp {
        name: "cxo",
        usage: "cxo <cmd...>",
        description: "Run command output and print last agent message",
    },
    CommandHelp {
        name: "cxol",
        usage: "cxol <cmd...>",
        description: "Run command output through LLM plain mode",
    },
    CommandHelp {
        name: "cxcopy",
        usage: "cxcopy <cmd...>",
        description: "Copy cxo output to clipboard (pbcopy/wl-copy/xclip)",
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
        description: "Run end-to-end selected-LLM/cx smoke checks",
    },
    CommandHelp {
        name: "rtk-status",
        usage: "rtk-status",
        description: "Show RTK version/range decision and fallback mode",
    },
    CommandHelp {
        name: "log-on",
        usage: "log-on",
        description: "Enable cx logging (process-local)",
    },
    CommandHelp {
        name: "log-off",
        usage: "log-off",
        description: "Disable cx logging in this process",
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
        name: "prompt",
        usage: "prompt <mode> <request>",
        description: "Generate Codex-ready prompt block",
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
        description: "Compatibility shim for bash-style cx command names",
    },
    CommandHelp {
        name: "profile",
        usage: "profile [N]",
        description: "Summarize last N runs from resolved cx log (default {RUN_WINDOW})",
    },
    CommandHelp {
        name: "alert",
        usage: "alert [N]",
        description: "Report anomalies from last N runs (default {RUN_WINDOW})",
    },
    CommandHelp {
        name: "optimize",
        usage: "optimize [N] [--json]",
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
        description: "Show Nth most-recent run from resolved cx log (default 1)",
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
        usage: "cx task add \"<objective>\" [--role <architect|implementer|reviewer|tester|doc>] [--backend <auto|codex|ollama>] [--model <name>] [--profile <fast|balanced|quality|schema_strict>] [--converge <none|first_valid|majority|judge|score>] [--replicas <n>] [--max-concurrency <n>] [--mode <sequential|parallel>] [--depends-on <id1,id2>] [--resource <key>]",
        description: "Create a task with role, routing, and orchestration metadata",
    },
    CommandHelp {
        name: "task list",
        usage: "cx task list [--status pending|in_progress|complete|failed]",
        description: "List tasks with optional status filter",
    },
    CommandHelp {
        name: "task claim",
        usage: "cx task claim <id>",
        description: "Mark task as in_progress",
    },
    CommandHelp {
        name: "task complete",
        usage: "cx task complete <id>",
        description: "Mark task as complete",
    },
    CommandHelp {
        name: "task fail",
        usage: "cx task fail <id>",
        description: "Mark task as failed",
    },
    CommandHelp {
        name: "task show",
        usage: "cx task show <id>",
        description: "Show one task record",
    },
    CommandHelp {
        name: "task fanout",
        usage: "cx task fanout \"<objective>\" [--from staged-diff|worktree|log|file:PATH]",
        description: "Generate role-tagged subtasks",
    },
    CommandHelp {
        name: "task run-plan",
        usage: "cx task run-plan [--status pending|in_progress|complete|failed] [--json]",
        description: "Preview deterministic execution waves before run-all",
    },
    CommandHelp {
        name: "task run",
        usage: "cx task run <id> [--mode lean|deterministic|verbose] [--backend codex|ollama]",
        description: "Run one task objective",
    },
    CommandHelp {
        name: "task run-all",
        usage: "cx task run-all [--status pending] [--mode sequential|mixed] [--backend-pool codex,ollama] [--backend-cap backend=limit] [--max-workers N]",
        description: "Run tasks by status (sequential default; mixed uses run-plan waves and broker-aware backend routing)",
    },
];
