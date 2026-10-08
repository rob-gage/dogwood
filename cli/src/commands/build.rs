// Copyright Rob Gage 2026

pub use std::{
    fs,
    io,
    path::PathBuf,
    process::{
        Command,
        exit,
        Output
    },
};

/// Builds a project and returns its Cargo target directory
pub fn build(
    cargo_path: PathBuf,
    path: PathBuf,
    debug: bool,
    output_directory: Option<PathBuf>,
    verbose: bool,
) -> PathBuf {
    let mut metadata_command: Command = Command::new(&cargo_path);
    metadata_command
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .current_dir(&path);
    if verbose {
        println!("Running metadata command: {metadata_command:?}");
        let _ = io::Write::flush(&mut io::stdout());
    }
    let metadata_output: Output = match metadata_command.output() {
        Ok(output) => output,
        Err(error) => {
            if verbose {
                eprintln!("Could not inspect Cargo project: {error}");
            }
            println!("Build failed.");
            exit(1);
        }
    };
    if !metadata_output.status.success() {
        let _ = io::Write::write_all(&mut io::stderr(), &metadata_output.stderr);
        let _ = io::Write::write_all(&mut io::stdout(), &metadata_output.stdout);
        println!("Build failed.");
        exit(metadata_output.status.code().unwrap_or(1));
    }
    let metadata: serde_json::Value = match serde_json::from_slice(&metadata_output.stdout) {
        Ok(metadata) => metadata,
        Err(error) => {
            if verbose { eprintln!("Could not read Cargo project metadata: {error}"); }
            println!("Build failed.");
            exit(1);
        }
    };
    let target_directory: PathBuf = if let Some(output_directory) = output_directory {
        if output_directory.is_absolute() { output_directory } else { path.join(output_directory) }
    } else if let Some(target_directory) = metadata["target_directory"].as_str() {
        let target_directory: PathBuf = PathBuf::from(target_directory);
        if target_directory.is_absolute() {
            target_directory
        } else {
            path.join(target_directory)
        }
    } else {
        if verbose {
            eprintln!("Cargo metadata did not contain a target directory");
        }
        println!("Build failed.");
        exit(1);
    };
    let mut command: Command = Command::new(&cargo_path);
    command
        .arg("build")
        .arg("--message-format=json-render-diagnostics")
        .current_dir(&path);
    if !debug {
        command.arg("--release");
    }
    command.arg("--target-dir").arg(&target_directory);
    if verbose {
        println!("Running build command: {command:?}");
        let _ = io::Write::flush(&mut io::stdout());
    }
    let output: Output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            if verbose {
                eprintln!("Could not build with Cargo: {error}");
            }
            println!("Build failed.");
            exit(1);
        }
    };
    let stdout: String = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut diagnostics: Vec<String> = Vec::new();
    let mut other_stdout: Vec<String> = Vec::new();
    for output_line in stdout.lines() {
        let message: serde_json::Value = match serde_json::from_str(output_line) {
            Ok(message) => message,
            Err(_) => {
                other_stdout.push(output_line.to_owned());
                continue;
            }
        };
        if message["reason"] == "compiler-message" {
            if let Some(rendered_message) = message["message"]["rendered"].as_str() {
                diagnostics.push(rendered_message.to_owned());
            }
        } else if verbose {
            other_stdout.push(output_line.to_owned());
        }
    }
    if verbose || !output.status.success() {
        let _ = io::Write::write_all(&mut io::stderr(), &output.stderr);
        for diagnostic in diagnostics { eprint!("{diagnostic}"); }
        if verbose {
            for output_line in other_stdout {
                println!("{output_line}");
            }
        }
        let _ = io::Write::flush(&mut io::stderr());
    }
    if !output.status.success() {
        println!("Build failed.");
        exit(output.status.code().unwrap_or(1));
    }
    let target_directory: PathBuf = match fs::canonicalize(target_directory) {
        Ok(target_directory) => target_directory,
        Err(error) => {
            if verbose {
                eprintln!("Could not resolve Cargo target directory: {error}");
            }
            println!("Build failed.");
            exit(1);
        }
    };
    println!("Build complete.");
    target_directory
}
