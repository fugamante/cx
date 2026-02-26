use std::collections::HashMap;

use crate::logs::load_runs;
use crate::paths::resolve_log_file;

pub fn print_worklog(n: usize) -> i32 {
    let Some(log_file) = resolve_log_file() else {
        eprintln!("cxrs: unable to resolve log file");
        return 1;
    };
    if !log_file.exists() {
        println!("# cxrs Worklog");
        println!();
        println!("Window: last {n} runs");
        println!();
        println!("No runs found.");
        println!();
        println!("_log_file: {}_", log_file.display());
        return 0;
    }
    let runs = match load_runs(&log_file, n) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cxrs worklog: {e}");
            return 1;
        }
    };

    println!("# cxrs Worklog");
    println!();
    println!("Window: last {n} runs");
    println!();

    let mut by_tool: HashMap<String, (u64, u64, u64)> = HashMap::new();
    for r in &runs {
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let entry = by_tool.entry(tool).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += r.duration_ms.unwrap_or(0);
        entry.2 += r.effective_input_tokens.unwrap_or(0);
    }

    let mut grouped: Vec<(String, u64, u64, u64)> = by_tool
        .into_iter()
        .map(|(tool, (count, sum_dur, sum_eff))| {
            let avg_dur = if count == 0 { 0 } else { sum_dur / count };
            let avg_eff = if count == 0 { 0 } else { sum_eff / count };
            (tool, count, avg_dur, avg_eff)
        })
        .collect();
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));

    println!("## By Tool");
    println!();
    println!("| Tool | Runs | Avg Duration (ms) | Avg Effective Tokens |");
    println!("|---|---:|---:|---:|");
    for (tool, count, avg_dur, avg_eff) in grouped {
        println!("| {tool} | {count} | {avg_dur} | {avg_eff} |");
    }
    println!();

    println!("## Chronological Runs");
    println!();
    for r in &runs {
        let ts = r.ts.clone().unwrap_or_else(|| "n/a".to_string());
        let tool = r.tool.clone().unwrap_or_else(|| "unknown".to_string());
        let dur = r.duration_ms.unwrap_or(0);
        let eff = r.effective_input_tokens.unwrap_or(0);
        println!("- {ts} | {tool} | {dur}ms | {eff} effective tokens");
    }
    println!();
    println!("_log_file: {}_", log_file.display());
    0
}
