use std::{
	ffi::OsString,
	sync::{LazyLock, OnceLock},
};

use fluent_bundle::{
	FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle,
};
use unic_langid::LanguageIdentifier;

use crate::command_line::closest_match;

const EN_US: &str = include_str!("../locales/en-US.ftl");
const RU_RU: &str = include_str!("../locales/ru-RU.ftl");
const LANGUAGE_NAMES: &[&str] = &[
	"en", "eng", "english", "en-US", "ru", "rus", "russian", "ru-RU",
];

static CATALOG: LazyLock<Catalog> = LazyLock::new(Catalog::new);
static RUN_LANGUAGE: OnceLock<Language> = OnceLock::new();
static SYSTEM_LANGUAGE: LazyLock<Language> =
	LazyLock::new(|| negotiate(sys_locale::get_locales()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
	English,
	Russian,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Override {
	Automatic,
	Supported(Language),
	Unsupported(String),
	Invalid {
		value: String,
		suggestion: Option<&'static str>,
	},
	Missing,
}

pub fn init(language: Language) {
	RUN_LANGUAGE
		.set(language)
		.expect("run language must only be initialized once");
}

pub fn system_language() -> Language {
	*SYSTEM_LANGUAGE
}

pub fn negotiate(
	locales: impl IntoIterator<Item = impl AsRef<str>>,
) -> Language {
	locales
		.into_iter()
		.find_map(|locale| {
			let locale = locale.as_ref();
			let locale = locale
				.split(['.', '@'])
				.next()
				.unwrap_or(locale)
				.replace('_', "-");
			parse_identifier(&locale)
		})
		.unwrap_or(Language::English)
}

pub fn language_override(arguments: &[OsString]) -> Override {
	let mut value = None;
	let mut arguments = arguments.iter().skip(1);
	while let Some(argument) = arguments.next() {
		if argument == "--" {
			break;
		}
		let Some(argument) = argument.to_str() else {
			continue;
		};
		if argument == "--lang" {
			let Some(next) = arguments.next().and_then(|value| value.to_str())
			else {
				return Override::Missing;
			};
			if next.starts_with('-') {
				return Override::Missing;
			}
			value = Some(next.to_owned());
		} else if let Some(next) = argument.strip_prefix("--lang=") {
			if next.is_empty() {
				return Override::Missing;
			}
			value = Some(next.to_owned());
		}
	}
	value.map_or(Override::Automatic, parse_override)
}

fn parse_override(value: String) -> Override {
	let lowercase = value.to_ascii_lowercase();
	let language = match lowercase.as_str() {
		"english" => Some(Language::English),
		"russian" => Some(Language::Russian),
		_ => parse_identifier(&value),
	};
	if let Some(language) = language {
		return Override::Supported(language);
	}
	let tag_shaped =
		value.contains('-') || (2..=3).contains(&value.chars().count());
	if tag_shaped && value.parse::<LanguageIdentifier>().is_ok() {
		return Override::Unsupported(value);
	}
	Override::Invalid {
		suggestion: closest_match(&value, LANGUAGE_NAMES),
		value,
	}
}

fn parse_identifier(value: &str) -> Option<Language> {
	let identifier = value.parse::<LanguageIdentifier>().ok()?;
	match identifier.language.as_str() {
		"en" | "eng" => Some(Language::English),
		"ru" | "rus" => Some(Language::Russian),
		_ => None,
	}
}

pub fn tr(id: &str) -> String {
	tr_for(current_language(), id)
}

pub fn tr_args<'args>(
	id: &str,
	args: &[(&'args str, FluentValue<'args>)],
) -> String {
	tr_for_args(current_language(), id, args)
}

pub fn tr_for(language: Language, id: &str) -> String {
	tr_for_args(language, id, &[])
}

pub fn tr_for_args<'args>(
	language: Language,
	id: &str,
	args: &[(&'args str, FluentValue<'args>)],
) -> String {
	let mut fluent_args = FluentArgs::new();
	for (name, value) in args {
		fluent_args.set(*name, value.clone());
	}
	CATALOG.format(language, id, &fluent_args)
}

fn current_language() -> Language {
	RUN_LANGUAGE.get().copied().unwrap_or_else(system_language)
}

struct Catalog {
	english: FluentBundle<FluentResource>,
	russian: FluentBundle<FluentResource>,
}

impl Catalog {
	fn new() -> Self {
		Self {
			english: bundle("en-US", EN_US),
			russian: bundle("ru-RU", RU_RU),
		}
	}

	fn format(
		&self,
		language: Language,
		id: &str,
		args: &FluentArgs<'_>,
	) -> String {
		let selected = match language {
			Language::English => &self.english,
			Language::Russian => &self.russian,
		};
		format_message(selected, id, args)
			.or_else(|| format_message(&self.english, id, args))
			.unwrap_or_else(|| {
				panic!("missing or invalid English localization message: {id}")
			})
	}
}

fn bundle(locale: &str, source: &str) -> FluentBundle<FluentResource> {
	let language = locale
		.parse::<LanguageIdentifier>()
		.expect("embedded locale identifier must be valid");
	let resource = FluentResource::try_new(source.to_owned()).unwrap_or_else(
		|(_, errors)| {
			panic!("invalid embedded {locale} Fluent resource: {errors:?}")
		},
	);
	let mut bundle = FluentBundle::new_concurrent(vec![language]);
	bundle.set_use_isolating(false);
	bundle.add_resource(resource).unwrap_or_else(|errors| {
		panic!("invalid embedded {locale} Fluent messages: {errors:?}")
	});
	bundle
}

fn format_message(
	bundle: &FluentBundle<FluentResource>,
	id: &str,
	args: &FluentArgs<'_>,
) -> Option<String> {
	let message = bundle.get_message(id)?;
	let pattern = message.value()?;
	let mut errors = Vec::new();
	let value = bundle.format_pattern(pattern, Some(args), &mut errors);
	errors.is_empty().then(|| value.into_owned())
}

#[cfg(test)]
#[path = "l10n_tests.rs"]
mod tests;
