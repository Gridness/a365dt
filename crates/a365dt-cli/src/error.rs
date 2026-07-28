use std::fmt;

use crate::l10n::{tr, tr_args};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
	message: String,
	detail: Option<String>,
	cancelled: bool,
	interrupted: bool,
}

impl Error {
	pub fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
			detail: None,
			cancelled: false,
			interrupted: false,
		}
	}

	pub fn with_debug(
		message: impl Into<String>,
		detail: impl fmt::Display,
	) -> Self {
		Self {
			message: message.into(),
			detail: Some(detail.to_string()),
			cancelled: false,
			interrupted: false,
		}
	}

	pub fn cancelled() -> Self {
		Self {
			message: tr("cancelled"),
			detail: None,
			cancelled: true,
			interrupted: false,
		}
	}

	pub fn interrupted() -> Self {
		Self {
			message: tr("interrupted"),
			detail: None,
			cancelled: false,
			interrupted: true,
		}
	}

	pub fn is_cancelled(&self) -> bool {
		self.cancelled
	}

	pub fn is_interrupted(&self) -> bool {
		self.interrupted
	}

	pub fn context(mut self, context: &str) -> Self {
		self.message = tr_args(
			"error-context",
			&[
				("context", context.into()),
				("message", self.message.into()),
			],
		);
		self
	}

	pub fn render(&self, debug: bool) -> String {
		match (debug, &self.detail) {
			(true, Some(detail)) => detail.clone(),
			(false, _) | (true, None) => self.message.clone(),
		}
	}
}

impl fmt::Display for Error {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(&self.message)
	}
}

impl From<String> for Error {
	fn from(message: String) -> Self {
		Self::new(message)
	}
}

impl From<&str> for Error {
	fn from(message: &str) -> Self {
		Self::new(message)
	}
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
