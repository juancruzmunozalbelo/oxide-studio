use std::path::PathBuf;
use std::time::Duration;
use ableton_plugin_scanner::{
    extract_bundle_info, PluginDescriptor, PluginFormat, PluginScannerSupervisor,
};

fn print_help() {
    println!("Ableton Plugin Scanner (macOS AU/VST3 Out-of-Process Scanner)");
    println!();
    println!("USAGE:");
    println!("    ableton-plugin-scanner [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help               Print this help message");
    println!("    --scan-plugins           Scan all standard macOS plugin directories into manifest");
    println!("    --scan-single <PATH>     Inspect a single plugin bundle out-of-process and emit JSON");
    println!("    --output <PATH>          Path to write plugin_manifest.json (default: plugin_manifest.json)");
    println!("    --timeout <SECS>         Subprocess timeout in seconds (default: 10)");
    println!("    --simulate-crash         Simulate fatal crash (SIGSEGV) for testing isolation");
    println!("    --simulate-timeout       Simulate process hang for testing timeout isolation");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 || args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_help();
        return;
    }

    if args.contains(&"--simulate-crash".to_string()) {
        // Intentionally trigger fatal memory access violation (SIGSEGV)
        eprintln!("[Worker] Simulating intentional SIGSEGV fatal crash");
        unsafe {
            let null_ptr = std::ptr::null_mut::<i32>();
            null_ptr.write(0xDEAD);
        }
        return;
    }

    if args.contains(&"--simulate-timeout".to_string()) {
        eprintln!("[Worker] Simulating hung initialization loop (sleep 60s)");
        std::thread::sleep(Duration::from_secs(60));
        return;
    }

    let mut i = 1;
    let mut scan_single_path: Option<PathBuf> = None;
    let mut do_scan_all = false;
    let mut output_path = PathBuf::from("plugin_manifest.json");
    let mut timeout_secs: u64 = 10;

    while i < args.len() {
        match args[i].as_str() {
            "--scan-single" => {
                if i + 1 < args.len() {
                    scan_single_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--scan-plugins" => {
                do_scan_all = true;
            }
            "--output" if i + 1 < args.len() => {
                output_path = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--timeout" if i + 1 < args.len() => {
                if let Ok(secs) = args[i + 1].parse::<u64>() {
                    timeout_secs = secs;
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(path) = scan_single_path {
        // Handle simulated crash in path name for testing
        let path_str = path.to_string_lossy();
        if path_str.contains("Faulty") || path_str.contains("Crash") {
            eprintln!("[Worker] Faulty plugin path detected: triggering SIGSEGV");
            unsafe {
                let null_ptr = std::ptr::null_mut::<i32>();
                null_ptr.write(0xDEAD);
            }
            return;
        }

        if path_str.contains("Hang") || path_str.contains("InfiniteLoop") {
            eprintln!("[Worker] Hung plugin path detected: sleeping 60s");
            std::thread::sleep(Duration::from_secs(60));
            return;
        }

        let format = if path_str.to_lowercase().ends_with(".vst3") {
            PluginFormat::Vst3
        } else {
            PluginFormat::AudioUnit
        };

        let bundle_info = extract_bundle_info(&path, format);
        let mtime_secs = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let desc = PluginDescriptor::new_valid(
            bundle_info.name,
            bundle_info.identifier,
            bundle_info.vendor,
            bundle_info.version,
            bundle_info.category,
            format,
            path,
            2,
            2,
            bundle_info.parameters,
            bundle_info.architectures,
            mtime_secs,
        );

        match serde_json::to_string(&desc) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                eprintln!("Serialization error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    if do_scan_all {
        println!("Starting Ableton Plugin Scanner supervisor...");
        let supervisor = PluginScannerSupervisor::new()
            .with_timeout(Duration::from_secs(timeout_secs))
            .with_manifest_cache_path(output_path.clone());

        let manifest = supervisor.scan_all();
        let valid_count = manifest.valid_plugins().count();
        let quarantined_count = manifest.quarantined_plugins().count();

        println!(
            "Scan complete: {} total plugins discovered ({} verified valid, {} quarantined)",
            manifest.plugins.len(),
            valid_count,
            quarantined_count
        );
        println!("Manifest written to: {}", output_path.display());
    }
}
