use image::{DynamicImage, Rgba, RgbaImage};
use pretty_assertions::assert_eq;

use super::{ItermTransport, write_iterm, write_kitty};

#[test]
fn writes_iterm_image_without_a_kitty_probe() {
	let mut output = Vec::new();

	write_iterm(&mut output, b"poster", ItermTransport::Direct).unwrap();

	assert_eq!(
		String::from_utf8(output).unwrap(),
		"\u{1b}]1337;File=inline=1;size=6;height=16;preserveAspectRatio=1:cG9zdGVy\u{7}\n"
	);
}

#[test]
fn writes_kitty_image_without_querying_the_terminal() {
	let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
		1,
		1,
		Rgba([1, 2, 3, 4]),
	));
	let mut output = Vec::new();

	write_kitty(&mut output, &image).unwrap();

	assert_eq!(
		String::from_utf8(output).unwrap(),
		"\u{1b}_Ga=T,f=32,s=1,v=1,r=16,q=2,m=0;AQIDBA==\u{1b}\\\n"
	);
}
