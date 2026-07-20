use pretty_assertions::assert_eq;

use super::{RangePlan, plan_range};
use crate::api::Episode;

#[test]
fn plans_missing_and_fractional_episodes() {
	let episode = |id, number: &str| Episode {
		id,
		episode_int: number.into(),
		episode_full: format!("Episode {number}"),
	};
	let episodes = vec![episode(1, "1"), episode(2, "2.5"), episode(3, "4")];

	assert_eq!(
		plan_range(&episodes, "1-4"),
		Ok(RangePlan {
			whole: vec![episode(1, "1"), episode(3, "4")],
			fractional: vec![episode(2, "2.5")],
			missing: vec![2, 3],
		})
	);
}
