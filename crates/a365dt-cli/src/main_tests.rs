use std::sync::Mutex;

use pretty_assertions::assert_eq;
use tokio::sync::watch;

use super::cancel_download;

#[test]
fn routes_interrupts_to_active_downloads() {
	let (cancel, mut cancellation) = watch::channel(false);
	let active_download = Mutex::new(Some(cancel));

	assert_eq!(
		(
			cancel_download(&active_download),
			*cancellation.borrow_and_update(),
		),
		(true, true)
	);
}
