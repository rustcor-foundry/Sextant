//! Probe the Sextant `LocalBrain` against a local inference server.
//!
//! Usage:
//!   cargo run -p sextant-pilot --example brain_probe -- <backend> <endpoint> <model> [intent]
//!     backend: ollama | llamacpp | vllm
//!
//! Exercises the real `sextant-inference` -> local server path and the
//! `LocalBrain::reason` prompt/parse, printing the resulting action plan.

use sextant_inference::InferenceBackend;
use sextant_pilot::{LocalBrain, PilotBrain};
use url::Url;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let backend = args.get(1).map(|s| s.as_str()).unwrap_or("ollama");
    let endpoint = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let model = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "qwen2.5:3b".to_string());
    let intent = args
        .get(4)
        .cloned()
        .unwrap_or_else(|| "Open example.com and summarize the page".to_string());

    let be = match backend {
        "ollama" => InferenceBackend::Ollama,
        "llamacpp" => InferenceBackend::LlamaCpp,
        "vllm" => InferenceBackend::VLLM,
        other => {
            eprintln!("unknown backend: {other} (use ollama|llamacpp|vllm)");
            std::process::exit(2);
        }
    };

    println!("backend={backend} endpoint={endpoint} model={model}");
    println!("intent: {intent}\n");

    let brain = LocalBrain::new(be, Url::parse(&endpoint).expect("bad endpoint url"), &model);
    let started = std::time::Instant::now();
    match brain.reason(&intent, &[]).await {
        Ok(actions) => {
            println!(
                "PLAN OK in {:?} ({} actions):",
                started.elapsed(),
                actions.len()
            );
            for (i, action) in actions.iter().enumerate() {
                println!("  [{i}] {action:?}");
            }
        }
        Err(error) => println!("REASON ERROR after {:?}: {error}", started.elapsed()),
    }
}
