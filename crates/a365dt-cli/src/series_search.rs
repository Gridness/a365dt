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
	let local_available = !suggestions(&cache, &query, &[], 10).is_empty();
	let mut exact_ids = Vec::new();
	match remote_search(api, &query).await {
		Ok(results) => {
			exact_ids.extend(results.exact.iter().map(|series| series.id));
			let mut incoming = results.exact;
			incoming.extend(results.fallback);
			upsert(&mut cache.series, incoming);
			store(cache.clone()).await;
		}
		Err(error) if !local_available => return Err(error),
		Err(_) => {}
	}
	let matches = suggestions(&cache, &query, &exact_ids, 10);
	let selected = select::choose_series(&matches)?;
	match api.series(selected.id).await? {
		Some(series) => {
			if exact_ids.contains(&selected.id) {
				cache.remember_alias(&query, selected.id);
				store(cache).await;
			}
			Ok(series)
		}
		None => {
			cache.remove_series(selected.id);
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
	let mut server_matches = Vec::new();
	prioritize(&mut state, &cache, &server_matches);
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
						let query = state.query().to_owned();
						let confirmed = server_matches.contains(&selected.id);
						selector::clear(&term, lines)
							.map_err(selector::term_error)?;
						term.flush().map_err(selector::term_error)?;
						let spinner = ui::spinner("Loading title…");
						let series = api.series(selected.id).await;
						spinner.finish_and_clear();
						match series? {
							Some(series) => {
								if confirmed {
									cache.remember_alias(&query, selected.id);
									let _ = cache_tx.send(cache.clone());
								}
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
								cache.remove_series(selected.id);
								let _ = cache_tx.send(cache.clone());
								rows = select::series_rows(&cache.series);
								layout.replace(&term, &rows);
								state.replace(&rows);
								prioritize(&mut state, &cache, &server_matches);
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
						server_matches.clear();
						prioritize(&mut state, &cache, &server_matches);
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
				if query == state.query().trim() =>
			{
				match result {
					Ok(results) => {
						server_matches = results
							.exact
							.iter()
							.map(|series| series.id)
							.collect();
						let mut incoming = results.exact;
						incoming.extend(results.fallback);
						query_results
							.extend(incoming.iter().map(|series| series.id));
						if !incoming.is_empty() {
							upsert(&mut cache.series, incoming);
							let _ = cache_tx.send(cache.clone());
							rows = select::series_rows(&cache.series);
							layout.replace(&term, &rows);
							state.replace(&rows);
						}
						prioritize(&mut state, &cache, &server_matches);
						state.select_first();
					}
					Err(error) if !state.has_matches() => {
						selector::clear(&term, lines)
							.map_err(selector::term_error)?;
						term.flush().map_err(selector::term_error)?;
						ui::warning(error);
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
					Err(_) => {}
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
					prioritize(&mut state, &cache, &server_matches);
				}
			}
			Event::Update(Some(Update::Refreshed(mut refreshed))) => {
				let selected =
					state.selected_row().map(|index| cache.series[index].id);
				let mut preserved_ids = query_results.clone();
				preserved_ids.extend(cache.aliases.values().copied());
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
							preserved_ids.contains(&series.id)
								&& ids.insert(series.id)
						})
						.cloned(),
				);
				refreshed.aliases.clone_from(&cache.aliases);
				cache = refreshed;
				let _ = cache_tx.send(cache.clone());
				rows = select::series_rows(&cache.series);
				layout.replace(&term, &rows);
				state.replace(&rows);
				prioritize(&mut state, &cache, &server_matches);
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
	Search(String, ApiResult<RemoteResults>),
	Page(usize, Vec<Series>),
	Refreshed(Cache),
}

struct RemoteResults {
	exact: Vec<Series>,
	fallback: Vec<Series>,
}

fn schedule_search(
	api: &Anime365,
	updates: &mpsc::UnboundedSender<Update>,
	state: &selector::State,
) -> Option<JoinHandle<()>> {
	let query = state.query().trim();
	if query.is_empty() {
		return None;
	}
	let query = query.to_owned();
	let api = api.clone();
	let updates = updates.clone();
	Some(tokio::spawn(async move {
		sleep(SEARCH_DEBOUNCE).await;
		let result = remote_search(&api, &query).await;
		let _ = updates.send(Update::Search(query, result));
	}))
}

async fn remote_search(
	api: &Anime365,
	query: &str,
) -> ApiResult<RemoteResults> {
	let exact = api.search(query).await?;
	if !exact.is_empty() {
		return Ok(RemoteResults {
			exact,
			fallback: Vec::new(),
		});
	}
	let fallback_query = api_query(query);
	let fallback = if fallback_query == query {
		Vec::new()
	} else {
		ranked_series(&api.search(fallback_query).await?, query, 10)
	};
	Ok(RemoteResults { exact, fallback })
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
		aliases: BTreeMap::new(),
	};
	let mut ids = HashSet::new();
	cache.series.retain(|series| ids.insert(series.id));
	cache.mark_refreshed();
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
	tokio::task::spawn_blocking(move || term.read_key_raw())
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

fn suggestions(
	cache: &Cache,
	query: &str,
	server_matches: &[u64],
	limit: usize,
) -> Vec<Series> {
	let rows = select::series_rows(&cache.series);
	let search = Search::new(&rows);
	let mut seen = HashSet::new();
	preferred_rows(cache, query, server_matches)
		.into_iter()
		.chain(search.ranked(query))
		.filter(|index| seen.insert(*index))
		.take(limit)
		.map(|index| cache.series[index].clone())
		.collect()
}

fn prioritize(
	state: &mut selector::State,
	cache: &Cache,
	server_matches: &[u64],
) {
	state.prefer(preferred_rows(cache, state.query(), server_matches));
}

fn preferred_rows(
	cache: &Cache,
	query: &str,
	server_matches: &[u64],
) -> Vec<usize> {
	let mut ids = cache.alias(query).into_iter().collect::<Vec<_>>();
	ids.extend_from_slice(server_matches);
	let mut seen = HashSet::new();
	// ponytail: at most 11 priorities; add an ID index if that limit grows.
	ids.into_iter()
		.filter(|id| seen.insert(*id))
		.filter_map(|id| cache.series.iter().position(|series| series.id == id))
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
