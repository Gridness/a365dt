use std::{
	env, fmt,
	io::{self, Write},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::DynamicImage;
use reqwest::Method;

use crate::{
	api::{Anime365, Series},
	ui::selector,
};

const HEIGHT: u32 = 16;

#[derive(Clone, Copy)]
enum ItermTransport {
	Direct,
	Tmux,
}

enum Protocol {
	Iterm(ItermTransport),
	Kitty,
}

pub async fn show(api: &Anime365, series: &Series) {
	let Some(protocol) = protocol() else {
		return;
	};
	let Some(url) = series.poster_url_small.as_deref() else {
		return;
	};
	let Ok(response) = api.asset(Method::GET, url, None).await else {
		return;
	};
	if !response.status().is_success() {
		return;
	}
	let Ok(bytes) = response.bytes().await else {
		return;
	};
	let mut output = io::stdout().lock();
	let _ = match protocol {
		Protocol::Iterm(transport) => {
			write_iterm(&mut output, &bytes, transport)
		}
		Protocol::Kitty => image::load_from_memory(&bytes)
			.map_err(io::Error::other)
			.and_then(|image| write_kitty(&mut output, &image)),
	};
}

fn protocol() -> Option<Protocol> {
	if !selector::interactive_terminal() {
		return None;
	}
	let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();
	let term_program = env::var("TERM_PROGRAM")
		.unwrap_or_default()
		.to_ascii_lowercase();
	let lc_terminal = env::var("LC_TERMINAL")
		.unwrap_or_default()
		.to_ascii_lowercase();
	let multiplexed = term.starts_with("screen") || term.starts_with("tmux");
	let kitty = env::var_os("KITTY_WINDOW_ID").is_some()
		|| term.contains("kitty")
		|| term.contains("ghostty")
		|| term_program.contains("ghostty");
	if kitty && !multiplexed {
		return Some(Protocol::Kitty);
	}
	let iterm = cfg!(target_os = "macos")
		|| env::var_os("ITERM_SESSION_ID").is_some()
		|| env::var_os("KONSOLE_VERSION").is_some()
		|| [&term_program, &lc_terminal].iter().any(|value| {
			["iterm", "wezterm", "mintty", "rio", "warpterminal"]
				.iter()
				.any(|terminal| value.contains(terminal))
		});
	iterm.then_some(Protocol::Iterm(if multiplexed {
		ItermTransport::Tmux
	} else {
		ItermTransport::Direct
	}))
}

fn write_iterm(
	output: &mut impl Write,
	image: &[u8],
	transport: ItermTransport,
) -> io::Result<()> {
	match transport {
		ItermTransport::Direct => {
			let encoded = STANDARD.encode(image);
			write_osc(
				output,
				format_args!(
					"1337;File=inline=1;size={};height={HEIGHT};preserveAspectRatio=1:{encoded}",
					image.len()
				),
				transport,
			)?;
		}
		ItermTransport::Tmux => {
			write_osc(
				output,
				format_args!(
					"1337;MultipartFile=inline=1;size={};height={HEIGHT};preserveAspectRatio=1",
					image.len()
				),
				transport,
			)?;
			for part in image.chunks(150) {
				let part = STANDARD.encode(part);
				write_osc(
					output,
					format_args!("1337;FilePart={part}"),
					transport,
				)?;
			}
			write_osc(output, format_args!("1337;FileEnd"), transport)?;
		}
	}
	writeln!(output)?;
	output.flush()
}

fn write_osc(
	output: &mut impl Write,
	body: fmt::Arguments<'_>,
	transport: ItermTransport,
) -> io::Result<()> {
	match transport {
		ItermTransport::Direct => write!(output, "\x1b]{body}\x07"),
		ItermTransport::Tmux => {
			write!(output, "\x1bPtmux;\x1b\x1b]{body}\x07\x1b\\")
		}
	}
}

fn write_kitty(
	output: &mut impl Write,
	image: &DynamicImage,
) -> io::Result<()> {
	let image = image.to_rgba8();
	let parts = image.as_raw().len().div_ceil(3_072);
	for (index, part) in image.as_raw().chunks(3_072).enumerate() {
		let part = STANDARD.encode(part);
		let more = usize::from(index + 1 < parts);
		if index == 0 {
			write!(
				output,
				"\x1b_Ga=T,f=32,s={},v={},r={HEIGHT},q=2,m={more};{part}\x1b\\",
				image.width(),
				image.height()
			)?;
		} else {
			write!(output, "\x1b_Gm={more};{part}\x1b\\")?;
		}
	}
	writeln!(output)?;
	output.flush()
}

#[cfg(test)]
#[path = "poster_tests.rs"]
mod tests;
