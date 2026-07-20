use pretty_assertions::assert_eq;

use super::aligned_rows;

#[test]
fn aligns_borderless_columns() {
	let rows = [
		["\x1b[31mOne\x1b[0m".into(), "2024".into(), "TV".into()],
		["Twenty".into(), "?".into(), "Movie".into()],
	];

	assert_eq!(
		aligned_rows(&rows),
		vec!["\x1b[31mOne\x1b[0m     2024  TV", "Twenty  ?     Movie"]
	);
}
