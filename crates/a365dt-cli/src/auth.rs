use std::process::{Command, Stdio};

use fluent_bundle::FluentValue;

#[cfg(target_os = "macos")]
use security_framework::{
	item::{ItemClass, ItemSearchOptions, SearchResult},
	passwords::{delete_generic_password, set_generic_password},
};

#[cfg(target_os = "macos")]
use crate::app_files;
use crate::{
	error::Error,
	l10n::{tr, tr_args},
	ui,
};

const ACCESS_TOKEN_URL: &str =
	"https://anime365.ru/api/accessToken?app=app-70510a2eebd4c6a4aa6e4a0e";
#[cfg(target_os = "macos")]
const KEYCHAIN_ITEM: &str = "anime365-access-token";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = app_files::APPLICATION_ID;
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

pub(crate) enum AccessToken {
	Environment(String),
	#[cfg(target_os = "macos")]
	Keychain(String),
	Browser(String),
}

impl AccessToken {
	pub(crate) fn value(&self) -> &str {
		match self {
			Self::Environment(token) | Self::Browser(token) => token,
			#[cfg(target_os = "macos")]
			Self::Keychain(token) => token,
		}
	}
}

pub(crate) fn access_token() -> Result<AccessToken, Error> {
	if let Ok(token) = std::env::var("ANIME365_ACCESS_TOKEN")
		&& !token.trim().is_empty()
	{
		return Ok(AccessToken::Environment(token.trim().to_owned()));
	}
	#[cfg(target_os = "macos")]
	if let Some(token) = keychain_token() {
		ui::note(tr("auth-keychain-token"));
		return Ok(AccessToken::Keychain(token));
	}
	browser_access_token()
}

fn browser_access_token() -> Result<AccessToken, Error> {
	if !ui::can_prompt() {
		return Err(Error::new(tr("auth-token-unavailable")));
	}
	ui::note(tr_args(
		"auth-opening",
		&[("url", FluentValue::from(ACCESS_TOKEN_URL))],
	));
	if !open_browser(ACCESS_TOKEN_URL) {
		ui::warning(tr("auth-browser-error"));
	}
	ui::note(tr("auth-sign-in"));
	let token = ui::secret(&tr("auth-token-prompt"))?;
	if token.is_empty() {
		return Err(Error::new(tr("auth-token-empty")));
	}
	Ok(AccessToken::Browser(token))
}

#[cfg(target_os = "macos")]
fn keychain_token() -> Option<String> {
	let mut search = ItemSearchOptions::new();
	let result = search
		.class(ItemClass::generic_password())
		.service(KEYCHAIN_ITEM)
		.account(KEYCHAIN_ACCOUNT)
		.load_data(true)
		.search();
	match result {
		Ok(results) => results.into_iter().find_map(|result| match result {
			SearchResult::Data(token) => match String::from_utf8(token) {
				Ok(token) if !token.trim().is_empty() => {
					Some(token.trim().to_owned())
				}
				Ok(_) => None,
				Err(error) => {
					ui::warning(tr_args(
						"auth-keychain-read-error-detail",
						&[("error", FluentValue::from(error.to_string()))],
					));
					None
				}
			},
			SearchResult::Ref(_)
			| SearchResult::Dict(_)
			| SearchResult::Other => None,
		}),
		Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => None,
		Err(error) => {
			ui::warning(tr_args(
				"auth-keychain-read-error-detail",
				&[("error", FluentValue::from(error.to_string()))],
			));
			None
		}
	}
}

#[cfg(target_os = "macos")]
pub(crate) fn store_if_requested(
	access_token: &AccessToken,
) -> Result<(), Error> {
	let AccessToken::Browser(token) = access_token else {
		return Ok(());
	};
	if !ui::confirm(&tr("auth-keychain-save-confirm"), true)? {
		return Ok(());
	}
	set_generic_password(KEYCHAIN_ITEM, KEYCHAIN_ACCOUNT, token.as_bytes())
		.map_err(|error| {
			Error::with_debug(tr("auth-keychain-save-error"), error)
		})?;
	ui::success(tr("auth-keychain-save-success"));
	Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn store_if_requested(
	_access_token: &AccessToken,
) -> Result<(), Error> {
	Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn remove_stored_token() -> Result<(), Error> {
	match delete_generic_password(KEYCHAIN_ITEM, KEYCHAIN_ACCOUNT) {
		Ok(()) => Ok(()),
		Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
		Err(error) => {
			Err(Error::with_debug(tr("auth-keychain-remove-error"), error))
		}
	}
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn remove_stored_token() -> Result<(), Error> {
	Ok(())
}

#[cfg(target_os = "macos")]
fn open_browser(url: &str) -> bool {
	spawn_browser(Command::new("open").arg(url))
}

#[cfg(target_os = "windows")]
fn open_browser(url: &str) -> bool {
	spawn_browser(Command::new("cmd").args(["/C", "start", "", url]))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_browser(url: &str) -> bool {
	spawn_browser(Command::new("xdg-open").arg(url))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn open_browser(_url: &str) -> bool {
	false
}

#[cfg(any(unix, target_os = "windows"))]
fn spawn_browser(command: &mut Command) -> bool {
	command
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.is_ok()
}
