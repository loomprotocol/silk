use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::Command;

fn get_last_metrics(metric: &str, db: &str, name: &str, branch: &str) -> Result<String, String> {
    let query = format!(
        r#"SELECT last("{}") FROM "{}"."autogen"."{}" WHERE "branch"='{}'"#,
        metric, db, name, branch
    );

    let response = solana_metrics::query(&query)?;

    match serde_json::from_str(&response) {
        Result::Ok(v) => {
            let v: Value = v;
            let data = &v["results"][0]["series"][0]["values"][0][1];
            if data.is_null() {
                return Result::Err("Key not found".to_string());
            }
            Result::Ok(data.to_string())
        }
        Result::Err(err) => Result::Err(err.to_string()),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // Open the path in read-only mode, returns `io::Result<File>`
    let fname = &args[1];
    let file = match File::open(fname) {
        Err(why) => panic!("couldn't open {}: {:?}", fname, why),
        Ok(file) => file,
    };

    let branch = &args[2];
    let upload_metrics = args.len() > 2;

    let git_output = Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
        .expect("failed to execute git rev-parse");
    let git_commit_hash = String::from_utf8_lossy(&git_output.stdout);
    let trimmed_hash = git_commit_hash.trim().to_string();

    let mut last_commit = None;
    let mut results = HashMap::new();

    let db = env::var("INFLUX_DATABASE").unwrap_or_else(|_| "scratch".to_string());

    for line in BufReader::new(file).lines() {
        if let Ok(v) = serde_json::from_str(&line.unwrap()) {
            let v: Value = v;
            if v["type"] == "bench" {
                let name = v["name"].as_str().unwrap().trim_matches('\"').to_string();

                if last_commit.is_none() {
                    last_commit = get_last_metrics(&"commit".to_string(), &db, &name, branch).ok();
                }

                let median: i64 = v["median"].to_string().parse().unwrap();
                let deviation: i64 = v["deviation"].to_string().parse().unwrap();
                if upload_metrics {
                    panic!("TODO...");
                    /*
                    solana_metrics::datapoint_info!(
                        &v["name"].as_str().unwrap().trim_matches('\"'),
                        ("test", "bench", String),
                        ("branch", branch.to_string(), String),
                        ("median", median, i64),
                        ("deviation", deviation, i64),
                        ("commit", git_commit_hash.trim().to_string(), String)
                    );
                    */
                }
                let last_median =
                    get_last_metrics(&"median".to_string(), &db, &name, branch).unwrap_or_default();
                let last_deviation = get_last_metrics(&"deviation".to_string(), &db, &name, branch)
                    .unwrap_or_default();

                results.insert(name, (median, deviation, last_median, last_deviation));
            }
        }
    }

    if let Some(commit) = last_commit {
        println!(
            "Comparing current commits: {} against baseline {} on {} branch",
            trimmed_hash, commit, branch
        );
        println!("bench_name, median, last_median, deviation, last_deviation");
        for (entry, values) in results {
            println!(
                "{}, {:#10?}, {:#10?}, {:#10?}, {:#10?}",
                entry,
                values.0,
                values.2.parse::<i32>().unwrap_or_default(),
                values.1,
                values.3.parse::<i32>().unwrap_or_default(),
            );
        }
    } else {
        println!("No previous results found for {} branch", branch);
        println!("hash: {}", trimmed_hash);
        println!("bench_name, median, deviation");
        for (entry, values) in results {
            println!("{}, {:10?}, {:10?}", entry, values.0, values.1);
        }
    }
    solana_metrics::flush();
}
