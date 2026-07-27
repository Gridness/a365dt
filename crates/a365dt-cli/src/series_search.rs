use std::{
	collections::{BTreeMap, HashSet},
	io,
	time::Duration,
};

use console::{Key, Term};
use tokio::{
	sync::mpsc,
	task::{JoinError, JoinHandle, JoinSet},
	time::sleep,
};

use crate::{
	api::{
		Anime365, Result as ApiResult, SERIES_PAGE_SIZE, Series,
		series_id_from_url,
	},
	error::Error,
	search::Search,
	select,
	series_cache::{self, Cache},
	ui::{self, selector},
};

const REFRESH_CONCURRENCY: usize = 4;
const MAX_CATALOGUE_SIZE: usize = 100_000;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);
const SEARCH_LABEL: &str = "Search title or paste Anime365 URL";

pub async fn choose(api: &Anime365, prefill: String) -> Result<Series, Error> {
	if prefill.starts_with("http://") || prefill.starts_with("https://") {
		return load_url(api, &prefill).await;
	}
	let cache = tokio::task::spawn_blocking(series_cache::load)
		.await
		.unwrap_or_default();
	if selector::interactive_terminal() {
		choose_interactive(api, cache, prefill).await
	} else {
		choose_line(api, cache, prefill).await
	}
}

async fn choose_line(
	api: &Anime365,
	mut cache: Cache,
	mut query: String,
) -> Result<Series, Error> {
	if query.is_empty() {
		query = ui::prompt("Search title or Anime365 catalogue URL:")?;
	}
	if query.starts_with("http://") || query.starts_with("https://") {
		return load_url(api, &query).await;
	}
	let mut matches = ranked_series(&cache.series, &query, 10);
	if matches.is_empty() {
		let candidates = api.search(api_query(&query)).await?;
		upsert(&mut cache.series, candidates.clone());
		store(cache.clone()).await;
		matches = ranked_series(&candidates, &query, 10);
	}
	let selected = select::choose_series(&matches)?;
	match api.series(selected.id).await? {
		Some(series) => Ok(series),
		None => {
			cache.series.retain(|series| series.id != selected.id);
			store(cache).await;
			Err("That cached Anime365 series no longer exists.".into())
		}
	}
}

async fn choose_interactive(
	api: &Anime365,
	mut cache: Cache,
	prefill: String,
) -> Result<Series, Error> {
	let term = Term::buffered_stdout();
	let mut rows = select::series_rows(&cache.series);
	let mut state = selector::State::from_rows(&rows, prefill);
	let mut layout = selector::Layout::new(&term, &rows);
	let mut lines =
		selector::draw(&term, SEARCH_LABEL, &rows, &mut layout, &mut state)
			.map_err(selector::term_error)?;
	let (updates_tx, mut updates) = mpsc::unbounded_channel();
	let (cache_tx, cache_rx) = mpsc::unbounded_channel();
	drop(tokio::spawn(write_cache(cache_rx)));
	if !cache.is_fresh() {
		drop(tokio::spawn(refresh(api.clone(), updates_tx.clone())));
	}
	let mut search_task = schedule_search(api, &updates_tx, &state);
	let mut key_task = read_key(&term);
	let mut query_results = HashSet::new();

	loop {
		let event = tokio::select! {
			key = &mut key_task => Event::Key(key),
			update = updates.recv() => Event::Update(update),
		};
		match event {
			Event::Key(key) => {
				let key = match resolve_key(key) {
					Ok(key) => key,
					Err(error) => {
						selector::clear(&term, lines)
							.map_err(selector::term_error)?;
						term.flush().map_err(selector::term_error)?;
						return Err(error);
					}
				};
				if matches!(key, Key::Enter)
					&& (state.query().starts_with("http://")
						|| state.query().starts_with("https://"))
				{
					selector::clear(&term, lines)
						.map_err(selector::term_error)?;
					term.flush().map_err(selector::term_error)?;
					return load_url(api, state.query()).await;
				}
				let visible = selector::visible_rows(&term);
				match state.handle(key, visible) {
					selector::Action::Selected(index) => {
						let selected = cache.series[index].clone();
						selector::clear(&term, lines)
							.map_err(selector::term_error)?;
						term.flush().map_err(selector::term_error)?;
						let spinner = ui::spinner("Loading title…");
						let series = api.series(selected.id).await;
						spinner.finish_and_clear();
						match series? {
							Some(series) => {
								selector::write_choice(
									&term,
									SEARCH_LABEL,
									&rows,
									index,
								)
								.map_err(selector::term_error)?;
								return Ok(series);
							}
							None => {
								cache.series.remove(index);
								let _ = cache_tx.send(cache.clone());
								rows = select::series_rows(&cache.series);
								layout.replace(&term, &rows);
								state.replace(&rows);
								ui::warning(
									"That cached title no longer exists; removed it.",
								);
								key_task = read_key(&term);
								lines = selector::draw(
									&term,
									SEARCH_LABEL,
									&rows,
									&mut layout,
									&mut state,
								)
								.map_err(selector::term_error)?;
								continue;
							}
						}
					}
					selector::Action::Changed => {
						if let Some(task) = search_task.take() {
							task.abort();
						}
						search_task = schedule_search(api, &updates_tx, &state);
					}
					selector::Action::Cancelled => {
						selector::clear(&term, lines)
							.map_err(selector::term_error)?;
						term.flush().map_err(selector::term_error)?;
						return Err("Cancelled.".into());
					}
					selector::Action::Continue => {}
				}
				key_task = read_key(&term);
			}
			Event::Update(Some(Update::Search(query, result)))
				if query == state.query() =>
			{
				if let Ok(series) = result
					&& !series.is_empty()
				{
					query_results.extend(series.iter().map(|series| series.id));
					upsert(&mut cache.series, series);
					let _ = cache_tx.send(cache.clone());
					rows = select::series_rows(&cache.series);
					layout.replace(&term, &rows);
					state.replace(&rows);
				}
			}
			Event::Update(Some(Update::Page(offset, series))) => {
				let query = state.query().trim();
				if (offset == 0 && cache.series.is_empty())
					|| (!query.is_empty()
						&& !ranked_series(&series, query, 1).is_empty())
				{
					upsert(&mut cache.series, series);
					rows = select::series_rows(&cache.series);
					layout.replace(&term, &rows);
					state.replace(&rows);
				}
			}
			Event::Update(Some(Update::Refreshed(mut refreshed))) => {
				let selected =
					state.selected_row().map(|index| cache.series[index].id);
				let original_len = refreshed.series.len();
				let mut ids = refreshed
					.series
					.iter()
					.map(|series| series.id)
					.collect::<HashSet<_>>();
				refreshed.series.extend(
					cache
						.series
						.iter()
						.filter(|series| {
							query_results.contains(&series.id)
								&& ids.insert(series.id)
						})
						.cloned(),
				);
				cache = refreshed;
				if cache.series.len() != original_len {
					let _ = cache_tx.send(cache.clone());
				}
				rows = select::series_rows(&cache.series);
				layout.replace(&term, &rows);
				state.replace(&rows);
				if let Some(selected) = selected {
					if let Some(row) = cache
						.series
						.iter()
						.position(|series| series.id == selected)
					{
						state.select_row(row);
					} else {
						state.select_first();
					}
				}
			}
			Event::Update(Some(Update::Search(_, _))) | Event::Update(None) => {
			}
		}
		selector::clear(&term, lines).map_err(selector::term_error)?;
		lines =
			selector::draw(&term, SEARCH_LABEL, &rows, &mut layout, &mut state)
				.map_err(selector::term_error)?;
	}
}

enum Event {
	Key(Result<io::Result<Key>, JoinError>),
	Update(Option<Update>),
}

enum Update {
	Search(String, ApiResult<Vec<Series>>),
	Page(usize, Vec<Series>),
	Refreshed(Cache),
}

fn schedule_search(
	api: &Anime365,
	updates: &mpsc::UnboundedSender<Update>,
	state: &selector::State,
) -> Option<JoinHandle<()>> {
	let query = state.query().trim();
	if query.is_empty() || state.has_matches() {
		return None;
	}
	let query = query.to_owned();
	let api_query = api_query(&query).to_owned();
	let api = api.clone();
	let updates = updates.clone();
	Some(tokio::spawn(async move {
		sleep(SEARCH_DEBOUNCE).await;
		let result = api.search(&api_query).await;
		let _ = updates.send(Update::Search(query, result));
	}))
}

async fn refresh(api: Anime365, updates: mpsc::UnboundedSender<Update>) {
	let mut active = JoinSet::new();
	let mut next_offset = 0;
	for _ in 0..REFRESH_CONCURRENCY {
		spawn_page(&mut active, &api, next_offset);
		next_offset += SERIES_PAGE_SIZE;
	}
	let mut pages = BTreeMap::new();
	let mut reached_end = false;
	while let Some(joined) = active.join_next().await {
		let Ok((offset, Ok(page))) = joined else {
			return;
		};
		let full = page.len() == SERIES_PAGE_SIZE;
		let _ = updates.send(Update::Page(offset, page.clone()));
		pages.insert(offset, page);
		if full && !reached_end {
			if next_offset >= MAX_CATALOGUE_SIZE {
				return;
			}
			spawn_page(&mut active, &api, next_offset);
			next_offset += SERIES_PAGE_SIZE;
		} else {
			reached_end = true;
		}
	}
	let mut cache = Cache {
		refreshed_at: 0,
		series: pages.into_values().flatten().collect(),
	};
	let mut ids = HashSet::new();
	cache.series.retain(|series| ids.insert(series.id));
	cache.mark_refreshed();
	store(cache.clone()).await;
	let _ = updates.send(Update::Refreshed(cache));
}

fn spawn_page(
	active: &mut JoinSet<(usize, ApiResult<Vec<Series>>)>,
	api: &Anime365,
	offset: usize,
) {
	let api = api.clone();
	active.spawn(async move { (offset, api.series_page(offset).await) });
}

fn read_key(term: &Term) -> JoinHandle<io::Result<Key>> {
	let term = term.clone();
	tokio::task::spawn_blocking(move || term.read_key())
}

fn resolve_key(key: Result<io::Result<Key>, JoinError>) -> Result<Key, Error> {
	key.map_err(|error| {
		Error::with_debug("The terminal input task stopped.", error)
	})?
	.map_err(selector::term_error)
}

fn api_query(query: &str) -> &str {
	query
		.split_whitespace()
		.max_by_key(|word| word.chars().count())
		.unwrap_or(query)
}

async fn load_url(api: &Anime365, input: &str) -> Result<Series, Error> {
	let id = series_id_from_url(input).ok_or_else(|| {
		"Enter an official Anime365 series catalogue URL.".to_owned()
	})?;
	api.series(id)
		.await?
		.ok_or_else(|| "That Anime365 series no longer exists.".into())
}

fn ranked_series(series: &[Series], query: &str, limit: usize) -> Vec<Series> {
	let rows = select::series_rows(series);
	Search::new(&rows)
		.ranked(query)
		.into_iter()
		.take(limit)
		.map(|index| series[index].clone())
		.collect()
}

fn upsert(current: &mut Vec<Series>, incoming: Vec<Series>) {
	let mut positions = current
		.iter()
		.enumerate()
		.map(|(index, series)| (series.id, index))
		.collect::<std::collections::HashMap<_, _>>();
	for series in incoming {
		if let Some(index) = positions.get(&series.id).copied() {
			current[index] = series;
		} else {
			positions.insert(series.id, current.len());
			current.push(series);
		}
	}
}

async fn write_cache(mut caches: mpsc::UnboundedReceiver<Cache>) {
	while let Some(mut cache) = caches.recv().await {
		while let Ok(newer) = caches.try_recv() {
			cache = newer;
		}
		store(cache).await;
	}
}

async fn store(cache: Cache) {
	let _ =
		tokio::task::spawn_blocking(move || series_cache::store(&cache)).await;
}

#[cfg(test)]
#[path = "series_search_tests.rs"]
mod tests;
