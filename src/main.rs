use std::process::ExitCode;

use kindlestorygraph::reader::{DEFAULT_FINISH_THRESHOLD, Reader, Status};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: kindlestorygraph <path-to-cc.db> [finish_threshold]");
        return ExitCode::from(2);
    };
    let threshold = args
        .get(2)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(DEFAULT_FINISH_THRESHOLD);

    let reader = match Reader::open_read_only(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e:#}");
            return ExitCode::from(1);
        }
    };
    let snapshot = match reader.snapshot(threshold) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e:#}");
            return ExitCode::from(1);
        }
    };

    let current = snapshot
        .iter()
        .find(|b| b.status == Status::CurrentlyReading);

    let out = serde_json::json!({
        "finish_threshold": threshold,
        "current": current,
        "in_progress": snapshot
            .iter()
            .filter(|b| b.status == Status::CurrentlyReading)
            .collect::<Vec<_>>(),
        "finished": snapshot
            .iter()
            .filter(|b| b.status == Status::Read)
            .collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&out).expect("serialize"));
    ExitCode::SUCCESS
}
