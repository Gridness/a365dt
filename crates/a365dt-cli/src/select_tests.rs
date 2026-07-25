use pretty_assertions::assert_eq;

use super::{RangePlan, plan_range};
use crate::api::Episode;

#[test]
fn plans_overlapping_ranges_with_fractional_episodes() {
	let episode = |id, number: &str| Episode {
		id,
		episode_int: number.into(),
		episode_full: format!("Episode {number}"),
	};
	let episodes = vec![
		episode(1, "1"),
		episode(2, "2.5"),
		episode(3, "4"),
		episode(4, "4.5"),
		episode(5, "6.5"),
		episode(6, "7"),
		episode(7, "7.5"),
	];

	assert_eq!(
		plan_range(&episodes, "1-3,2-4,6-7.5"),
		Ok(RangePlan {
			whole: vec![episode(1, "1"), episode(3, "4"), episode(6, "7")],
			fractional: vec![
				episode(2, "2.5"),
				episode(5, "6.5"),
				episode(7, "7.5"),
			],
			missing: vec![2, 3, 6],
		})
	);
}

#[test]
fn rejects_overlapping_ranges_wider_than_limit() {
	assert_eq!(
		plan_range(&[], "0-10000,9999-19999"),
		Err(
			"Enter ascending ranges no wider than 10,000 episodes after merging overlaps."
				.into()
		)
	);
}
