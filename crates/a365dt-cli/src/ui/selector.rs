use std::{
	collections::HashSet,
	io::{self, IsTerminal},
};

use console::{
	Key, Term, measure_text_width, strip_ansi_codes, style, truncate_str,
};

use super::{alert, aligned_rows, prompt, warning};
use crate::{
	error::Error,
	search::Search,
	telemetry::{Operation, Recorder},
};

const MAX_VISIBLE: usize = 10;

pub fn choose<const N: usize>(
	label: &str,
	rows: &[[String; N]],
) -> Result<usize, Error> {
	if rows.is_empty() {
		return Err("No choices are available.".into());
	}
	if !interactive_terminal() {
		return choose_line(label, rows);
	}
	choose_interactive(label, rows)
}

fn choose_line<const N: usize>(
	label: &str,
	rows: &[[String; N]],
) -> Result<usize, Error> {
	println!("{}", style(label).bold());
	let index_width = rows.len().to_string().len();
	let width = super::available_width(index_width + 4);
	for (index, row) in aligned_rows(rows, width).iter().enumerate() {
		let index = format!("{:>index_width$}", index + 1);
		println!("  {}  {row}", style(index).cyan().bold());
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

fn choose_interactive<const N: usize>(
	label: &str,
	rows: &[[String; N]],
) -> Result<usize, Error> {
	let term = Term::buffered_stdout();
	let mut state = State::new(Search::new(rows));
	let mut layout = Layout::new(&term, rows);
	let mut lines = draw(&term, label, rows, &mut layout, &mut state)
		.map_err(term_error)?;
	loop {
		let key = match term.read_key_raw() {
			Ok(key) => key,
			Err(error) => {
				clear(&term, lines).map_err(term_error)?;
				term.flush().map_err(term_error)?;
				return Err(term_error(error));
			}
		};
		let visible = visible_rows(&term);
		match state.handle(key, visible) {
			Action::Selected(index) => {
				clear(&term, lines).map_err(term_error)?;
				write_choice(&term, label, rows, index).map_err(term_error)?;
				return Ok(index);
			}
			Action::Cancelled => {
				clear(&term, lines).map_err(term_error)?;
				term.flush().map_err(term_error)?;
				return Err("Cancelled.".into());
			}
			Action::Changed | Action::Continue => {}
		}
		clear(&term, lines).map_err(term_error)?;
		lines = draw(&term, label, rows, &mut layout, &mut state)
			.map_err(term_error)?;
	}
}

pub(crate) fn interactive_terminal() -> bool {
	io::stdin().is_terminal()
		&& io::stdout().is_terminal()
		&& std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

#[derive(Debug)]
pub(crate) enum Action {
	Continue,
	Changed,
	Selected(usize),
	Cancelled,
}

#[derive(Debug)]
pub(crate) struct State {
	search: Search,
	input: String,
	cursor: usize,
	preferred: Vec<usize>,
	matches: Vec<usize>,
	selected: usize,
	offset: usize,
	telemetry: Recorder,
}

impl State {
	fn new(search: Search) -> Self {
		Self::with_telemetry(search, Recorder::default())
	}

	fn with_telemetry(search: Search, telemetry: Recorder) -> Self {
		let matches = search.ranked("");
		Self {
			search,
			input: String::new(),
			cursor: 0,
			preferred: Vec::new(),
			matches,
			selected: 0,
			offset: 0,
			telemetry,
		}
	}

	pub(crate) fn from_rows<const N: usize>(
		rows: &[[String; N]],
		input: String,
		telemetry: Recorder,
	) -> Self {
		let measurement =
			telemetry.measure_items(Operation::SearchIndex, rows.len());
		let search = Search::new(rows);
		drop(measurement);
		let mut state = Self::with_telemetry(search, telemetry);
		state.cursor = input.len();
		state.input = input;
		state.refresh();
		state
	}

	pub(crate) fn replace<const N: usize>(&mut self, rows: &[[String; N]]) {
		let selected = self.matches.get(self.selected).copied();
		let screen_row = self.selected.saturating_sub(self.offset);
		let measurement = self
			.telemetry
			.measure_items(Operation::SearchIndex, rows.len());
		self.search = Search::new(rows);
		drop(measurement);
		self.matches = self.ranked();
		if let Some(selected) = selected
			&& let Some(position) =
				self.matches.iter().position(|index| *index == selected)
		{
			self.selected = position;
			self.offset = position.saturating_sub(screen_row);
		} else {
			self.selected = 0;
			self.offset = 0;
		}
	}

	pub(crate) fn query(&self) -> &str {
		query_and_choice(&self.input).0
	}

	pub(crate) fn has_matches(&self) -> bool {
		!self.matches.is_empty()
	}

	pub(crate) fn prefer(&mut self, rows: Vec<usize>) {
		let selected = self.selected_row();
		let screen_row = self.selected.saturating_sub(self.offset);
		self.preferred = rows;
		self.matches = self.ranked();
		if let Some(position) = selected.and_then(|selected| {
			self.matches.iter().position(|index| *index == selected)
		}) {
			self.selected = position;
			self.offset = position.saturating_sub(screen_row);
		} else {
			self.select_first();
		}
	}

	pub(crate) fn selected_row(&self) -> Option<usize> {
		self.matches.get(self.selected).copied()
	}

	pub(crate) fn select_row(&mut self, row: usize) {
		let screen_row = self.selected.saturating_sub(self.offset);
		if let Some(position) =
			self.matches.iter().position(|index| *index == row)
		{
			self.selected = position;
			self.offset = position.saturating_sub(screen_row);
		} else {
			self.select_first();
		}
	}

	pub(crate) fn select_first(&mut self) {
		self.selected = 0;
		self.offset = 0;
	}

	pub(crate) fn handle(&mut self, key: Key, visible: usize) -> Action {
		match key {
			Key::ArrowUp => self.up(visible),
			Key::ArrowDown => self.down(visible),
			Key::ArrowLeft => self.left(),
			Key::ArrowRight => self.right(),
			Key::Home => self.cursor = 0,
			Key::End => self.cursor = self.input.len(),
			Key::Backspace => {
				let input = self.input.len();
				self.backspace();
				if self.input.len() != input {
					return Action::Changed;
				}
			}
			Key::Del => {
				let input = self.input.len();
				self.delete();
				if self.input.len() != input {
					return Action::Changed;
				}
			}
			Key::Char(character) if !character.is_control() => {
				self.insert(character);
				return Action::Changed;
			}
			Key::Enter => {
				if let Some(index) = self.choice() {
					return Action::Selected(index);
				}
				alert();
			}
			Key::Escape if self.input.is_empty() => {
				return Action::Cancelled;
			}
			Key::Escape => {
				self.clear();
				return Action::Changed;
			}
			Key::CtrlC => return Action::Cancelled,
			_ => {}
		}
		Action::Continue
	}

	fn up(&mut self, visible: usize) {
		if self.matches.is_empty() {
			return;
		}
		if self.selected == 0 {
			self.selected = self.matches.len() - 1;
			self.offset = self.matches.len().saturating_sub(visible);
		} else {
			self.selected -= 1;
			if self.selected < self.offset {
				self.offset = self.selected;
			}
		}
	}

	fn down(&mut self, visible: usize) {
		if self.matches.is_empty() {
			return;
		}
		if self.selected + 1 == self.matches.len() {
			self.selected = 0;
			self.offset = 0;
		} else {
			self.selected += 1;
			if self.selected >= self.offset + visible {
				self.offset += 1;
			}
		}
	}

	fn left(&mut self) {
		self.cursor = previous_boundary(&self.input, self.cursor);
	}

	fn right(&mut self) {
		self.cursor = next_boundary(&self.input, self.cursor);
	}

	fn insert(&mut self, character: char) {
		self.input.insert(self.cursor, character);
		self.cursor += character.len_utf8();
		self.refresh();
	}

	fn backspace(&mut self) {
		let previous = previous_boundary(&self.input, self.cursor);
		if previous != self.cursor {
			self.input.drain(previous..self.cursor);
			self.cursor = previous;
			self.refresh();
		}
	}

	fn delete(&mut self) {
		let next = next_boundary(&self.input, self.cursor);
		if next != self.cursor {
			self.input.drain(self.cursor..next);
			self.refresh();
		}
	}

	fn clear(&mut self) {
		self.input.clear();
		self.cursor = 0;
		self.refresh();
	}

	fn choice(&self) -> Option<usize> {
		let (_, numbered) = query_and_choice(&self.input);
		if let Some(numbered) = numbered {
			return numbered
				.checked_sub(1)
				.and_then(|index| self.matches.get(index))
				.copied();
		}
		self.matches.get(self.selected).copied()
	}

	fn refresh(&mut self) {
		self.matches = self.ranked();
		self.selected = 0;
		self.offset = 0;
	}

	fn ranked(&self) -> Vec<usize> {
		let _measurement = self
			.telemetry
			.measure_items(Operation::SearchRank, self.search.len());
		let query = query_and_choice(&self.input).0;
		let mut seen = HashSet::new();
		self.preferred
			.iter()
			.copied()
			.filter(|index| *index < self.search.len())
			.chain(self.search.ranked(query))
			.filter(|index| seen.insert(*index))
			.collect()
	}

	fn ensure_visible(&mut self, visible: usize) {
		if self.selected < self.offset {
			self.offset = self.selected;
		} else if self.selected >= self.offset + visible {
			self.offset = self.selected + 1 - visible;
		}
	}
}

fn query_and_choice(input: &str) -> (&str, Option<usize>) {
	let input = input.trim_end();
	let start = input
		.rfind(char::is_whitespace)
		.map_or(0, |index| index + 1);
	let token = &input[start..];
	let Some(number) = token.strip_prefix('#') else {
		return (input, None);
	};
	if number.is_empty() {
		return (input[..start].trim_end(), None);
	}
	if number.bytes().all(|byte| byte.is_ascii_digit()) {
		return (
			input[..start].trim_end(),
			Some(number.parse().unwrap_or(usize::MAX)),
		);
	}
	(input, None)
}

fn previous_boundary(input: &str, cursor: usize) -> usize {
	input[..cursor]
		.char_indices()
		.next_back()
		.map_or(0, |(index, _)| index)
}

fn next_boundary(input: &str, cursor: usize) -> usize {
	input[cursor..]
		.chars()
		.next()
		.map_or(cursor, |character| cursor + character.len_utf8())
}

pub(crate) fn visible_rows(term: &Term) -> usize {
	usize::from(term.size().0)
		.saturating_sub(2)
		.clamp(1, MAX_VISIBLE)
}

pub(crate) struct Layout {
	width: usize,
	index_width: usize,
	rows: Vec<String>,
}

impl Layout {
	pub(crate) fn new<const N: usize>(
		term: &Term,
		rows: &[[String; N]],
	) -> Self {
		let mut layout = Self {
			width: 0,
			index_width: 0,
			rows: Vec::new(),
		};
		layout.replace(term, rows);
		layout
	}

	pub(crate) fn replace<const N: usize>(
		&mut self,
		term: &Term,
		rows: &[[String; N]],
	) {
		self.width = content_width(term);
		self.index_width = rows.len().max(1).to_string().len();
		self.rows =
			aligned_rows(rows, self.width.saturating_sub(self.index_width + 4));
	}
}

pub(crate) fn draw<const N: usize>(
	term: &Term,
	label: &str,
	rows: &[[String; N]],
	layout: &mut Layout,
	state: &mut State,
) -> io::Result<usize> {
	let width = content_width(term);
	if layout.width != width || layout.rows.len() != rows.len() {
		layout.replace(term, rows);
	}
	let visible = visible_rows(term);
	state.ensure_visible(visible);
	let end = (state.offset + visible).min(state.matches.len());
	let position = if state.matches.is_empty() {
		"0 of 0".into()
	} else {
		format!("{}–{end} of {}", state.offset + 1, state.matches.len())
	};
	term.write_line(&truncate_str(
		&format!("{label}  {position}"),
		width,
		"…",
	))?;

	let rendered = if state.matches.is_empty() {
		term.write_line("  No matches")?;
		1
	} else {
		for position in state.offset..end {
			let index = state.matches[position];
			let row = format!(
				"{:>index_width$}  {}",
				position + 1,
				layout.rows[index],
				index_width = layout.index_width
			);
			if position == state.selected {
				term.write_line(&format!(
					"{} {}",
					style("›").cyan().bold(),
					style(strip_ansi_codes(&row)).cyan().bold()
				))?;
			} else {
				term.write_line(&format!("  {row}"))?;
			}
		}
		end - state.offset
	};
	for _ in rendered..visible {
		term.write_line("")?;
	}

	let prefix = format!("{} Filter or #number: ", style("?").cyan().bold());
	let available = width.saturating_sub(measure_text_width(&prefix)).max(1);
	let (input, cursor) = input_window(&state.input, state.cursor, available);
	term.write_line(&format!("{prefix}{input}"))?;
	term.move_cursor_up(1)?;
	term.move_cursor_right(measure_text_width(&prefix) + cursor)?;
	term.flush()?;
	Ok(visible + 2)
}

fn content_width(term: &Term) -> usize {
	usize::from(term.size().1).saturating_sub(1).max(1)
}

fn input_window(input: &str, cursor: usize, width: usize) -> (&str, usize) {
	let mut start = 0;
	while measure_text_width(&input[start..cursor]) > width {
		start = next_boundary(input, start);
	}
	let mut end = input.len();
	while measure_text_width(&input[start..end]) > width && end > cursor {
		end = previous_boundary(input, end);
	}
	(
		&input[start..end],
		measure_text_width(&input[start..cursor]),
	)
}

pub(crate) fn clear(term: &Term, lines: usize) -> io::Result<()> {
	term.move_cursor_down(1)?;
	term.clear_last_lines(lines)
}

pub(crate) fn write_choice<const N: usize>(
	term: &Term,
	label: &str,
	rows: &[[String; N]],
	index: usize,
) -> io::Result<()> {
	let width = usize::from(term.size().1).saturating_sub(1).max(1);
	let row = aligned_rows(
		std::slice::from_ref(&rows[index]),
		width.saturating_sub(measure_text_width(label) + 5),
	)
	.pop()
	.unwrap_or_default();
	term.write_line(&format!(
		"{} {}  {row}",
		style("✓").green().bold(),
		style(label).bold()
	))?;
	term.flush()
}

pub(crate) fn term_error(error: io::Error) -> Error {
	Error::with_debug("Could not use the interactive terminal.", error)
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
