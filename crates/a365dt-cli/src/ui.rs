use std::{
	io::{self, IsTerminal, Write},
	time::Duration,
};

use console::{Style, set_colors_enabled, set_colors_enabled_stderr, style};
use indicatif::{ProgressBar, ProgressStyle};

use crate::error::Error;

pub fn init() {
	let color =
		io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
	set_colors_enabled(color);
	set_colors_enabled_stderr(color);
}

pub fn heading(text: &str) {
	println!("\n{}", style(text).bold().cyan());
}

pub fn note(text: impl std::fmt::Display) {
	println!("{} {text}", style("●").cyan());
}

pub fn spinner(text: &'static str) -> ProgressBar {
	let spinner = ProgressBar::new_spinner();
	spinner.set_style(
		ProgressStyle::with_template("{spinner:.cyan} {msg}")
			.expect("valid style"),
	);
	spinner.set_message(text);
	spinner.enable_steady_tick(Duration::from_millis(80));
	spinner
}

pub fn success(text: impl std::fmt::Display) {
	println!("{} {text}", style("✓").green().bold());
}

pub fn warning(text: impl std::fmt::Display) {
	eprintln!("{} {text}", style("!").yellow().bold());
}

pub fn failure(text: impl std::fmt::Display) {
	eprintln!("{} {text}", style("✗").red().bold());
}

pub fn prompt(label: &str) -> Result<String, Error> {
	print!("{} {} ", style("?").cyan().bold(), style(label).bold());
	io::stdout().flush().map_err(|error| {
		Error::with_debug("Could not display the input prompt.", error)
	})?;
	let mut input = String::new();
	let read = io::stdin().read_line(&mut input).map_err(|error| {
		Error::with_debug("Could not read input from the terminal.", error)
	})?;
	if read == 0 {
		return Err(Error::new("Input closed before a response was entered."));
	}
	Ok(input.trim().to_owned())
}

pub fn confirm(label: &str, default: bool) -> Result<bool, Error> {
	let hint = if default { "[Y/n]" } else { "[y/N]" };
	loop {
		match prompt(&format!("{label} {hint}"))?.to_lowercase().as_str() {
			"" => return Ok(default),
			"y" | "yes" => return Ok(true),
			"n" | "no" => return Ok(false),
			_ => warning("Enter yes or no."),
		}
	}
}

pub fn choose(label: &str, rows: &[String]) -> Result<usize, Error> {
	println!("{}", style(label).bold());
	for (index, row) in rows.iter().enumerate() {
		println!("  {}  {row}", style(index + 1).cyan().bold());
	}
	loop {
		let input = prompt(&format!("Choose 1-{}:", rows.len()))?;
		if let Ok(choice) = input.parse::<usize>()
			&& (1..=rows.len()).contains(&choice)
		{
			return Ok(choice - 1);
		}
		warning("Choose one of the listed numbers.");
	}
}

pub fn red(text: impl std::fmt::Display) -> String {
	Style::new().red().apply_to(text).to_string()
}
