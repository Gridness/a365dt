use std::ffi::OsString;

use fluent_bundle::FluentValue;
use pretty_assertions::assert_eq;

use super::{
	EN_US, Language, Override, RU_RU, language_override, negotiate, tr_for_args,
};

#[test]
fn negotiates_first_supported_preferred_language() {
	assert_eq!(
		[
			negotiate(["de-DE", "ru-BY"]),
			negotiate(["eng-GB"]),
			negotiate(["uk-UA"]),
		],
		[Language::Russian, Language::English, Language::English]
	);
}

#[test]
fn parses_language_overrides_in_supported_positions() {
	for (arguments, expected) in [
		(
			&["a365dt", "--lang", "Russian", "doctor"][..],
			Override::Supported(Language::Russian),
		),
		(
			&["a365dt", "doctor", "--lang=eng-GB"][..],
			Override::Supported(Language::English),
		),
		(&["a365dt", "--", "--lang", "ru"][..], Override::Automatic),
	] {
		let arguments =
			arguments.iter().map(OsString::from).collect::<Vec<_>>();
		assert_eq!(language_override(&arguments), expected);
	}
}

#[test]
fn distinguishes_unsupported_and_invalid_overrides() {
	for (arguments, expected) in [
		(
			&["a365dt", "--lang", "de-DE"][..],
			Override::Unsupported("de-DE".into()),
		),
		(
			&["a365dt", "--lang", "russain"][..],
			Override::Invalid {
				value: "russain".into(),
				suggestion: Some("russian"),
			},
		),
		(&["a365dt", "--lang"][..], Override::Missing),
	] {
		let arguments =
			arguments.iter().map(OsString::from).collect::<Vec<_>>();
		assert_eq!(language_override(&arguments), expected);
	}
}

#[test]
fn formats_both_languages_without_isolation_marks() {
	let args = [("value", FluentValue::from("/tmp/anime"))];
	assert_eq!(
		[
			tr_for_args(Language::English, "test-value", &args),
			tr_for_args(Language::Russian, "test-value", &args),
		],
		["Value: /tmp/anime", "Значение: /tmp/anime"]
	);
}

#[test]
fn catalogs_have_matching_message_ids_and_variables_without_yo() {
	assert!(!RU_RU.contains(['ё', 'Ё']));
	assert_eq!(catalog_shape(EN_US), catalog_shape(RU_RU));
}

fn catalog_shape(source: &str) -> Vec<(String, Vec<String>)> {
	use fluent_syntax::ast::{
		Entry, Expression, InlineExpression, Pattern, PatternElement,
	};

	fn variables(pattern: &Pattern<&str>, output: &mut Vec<String>) {
		for element in &pattern.elements {
			let PatternElement::Placeable { expression } = element else {
				continue;
			};
			match expression {
				Expression::Inline(InlineExpression::VariableReference {
					id,
				}) => output.push(id.name.to_owned()),
				Expression::Select { selector, variants } => {
					if let InlineExpression::VariableReference { id } = selector
					{
						output.push(id.name.to_owned());
					}
					for variant in variants {
						variables(&variant.value, output);
					}
				}
				_ => {}
			}
		}
	}

	let resource = fluent_syntax::parser::parse(source).unwrap();
	let mut messages = resource
		.body
		.into_iter()
		.filter_map(|entry| {
			let Entry::Message(message) = entry else {
				return None;
			};
			let mut found = Vec::new();
			if let Some(value) = &message.value {
				variables(value, &mut found);
			}
			found.sort_unstable();
			found.dedup();
			Some((message.id.name.to_owned(), found))
		})
		.collect::<Vec<_>>();
	messages.sort_unstable();
	messages
}
