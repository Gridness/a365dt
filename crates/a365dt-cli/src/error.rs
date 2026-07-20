use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
	message: String,
	detail: Option<String>,
}

impl Error {
	pub fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
			detail: None,
		}
	}

	pub fn with_debug(
		message: impl Into<String>,
		detail: impl fmt::Display,
	) -> Self {
		Self {
			message: message.into(),
			detail: Some(detail.to_string()),
		}
	}

	pub fn message(&self) -> &str {
		&self.message
	}

	pub fn context(mut self, context: &str) -> Self {
		self.message = format!("{context}: {}", self.message);
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
