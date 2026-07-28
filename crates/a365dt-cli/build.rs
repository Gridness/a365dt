use std::{env, process::Command};

fn main() {
	println!("cargo:rerun-if-changed=../../.git/HEAD");
	println!(
		"cargo:rustc-env=A365DT_BUILD_PROFILE={}",
		env::var("PROFILE").unwrap_or_else(|_| "unknown".into())
	);
	println!(
		"cargo:rustc-env=A365DT_COMMIT_SHA={}",
		output("git", &["rev-parse", "--short=8", "HEAD"])
			.unwrap_or_else(|| "unknown".into())
	);
	let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
	println!(
		"cargo:rustc-env=A365DT_RUSTC={}",
		output(&rustc, &["--version"]).unwrap_or_else(|| "unknown".into())
	);
}

fn output(program: &str, arguments: &[&str]) -> Option<String> {
	let output = Command::new(program).args(arguments).output().ok()?;
	output
		.status
		.success()
		.then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
