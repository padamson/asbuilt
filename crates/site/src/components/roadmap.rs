use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::roadmap::{self, Horizon, Item, Roadmap as Data, Status};
use crate::version::{fetch_json, is_dev};

const ISSUES: &str = "https://github.com/padamson/asbuilt/issues";
const CHANGELOG: &str = "https://github.com/padamson/asbuilt/blob/main/CHANGELOG.md";
/// Later's line under its heading, where Now and Next show their theme.
const LATER: &str = "Under consideration, not yet planned";

/// Now, Next and Later, from `public/roadmap.json`. Now is the next release
/// and carries each item's status and a progress bar, Next the release
/// after it, and Later what is under consideration beyond that. What
/// already shipped is in the changelog.
///
/// The dev build's own copy is main's, so it shows it at once. A release
/// build waits for main's, so it never shows its own roadmap as if it were
/// current, and falls back to its own only when the fetch fails.
#[component]
pub fn Roadmap() -> impl IntoView {
    let dev = is_dev();
    let data = RwSignal::new(dev.then(Data::built));
    let fetched = RwSignal::new(false);
    if !dev {
        spawn_local(async move {
            match fetch_json::<Data>(&roadmap::current_url()).await {
                Some(current) => {
                    data.set(Some(current));
                    fetched.set(true);
                }
                None => data.set(Some(Data::built())),
            }
        });
    }
    let note = move || {
        roadmap::says_main(fetched.get(), dev).then(|| {
            view! {
                <span id="roadmap-current">"This is the current roadmap, from main. "</span>
            }
        })
    };

    view! {
        <section id="roadmap" class="mx-auto max-w-5xl px-6 py-12">
            <h2 class="text-2xl font-bold text-rust-300">"Roadmap"</h2>
            {move || {
                data.with(|data| {
                    data.as_ref()
                        .map(|data| {
                            view! {
                                <p class="mt-2 mb-6 max-w-3xl text-sm text-rust-50/70">
                                    {note}
                                    {format!(
                                        "Now is {}, Next is {}, and Later is under consideration beyond that. ",
                                        data.milestone,
                                        data.next_milestone,
                                    )}
                                    "What already shipped is in the "
                                    <a href=CHANGELOG class="underline hover:text-rust-300">
                                        "changelog"
                                    </a>
                                    "."
                                </p>
                                <div class="grid grid-cols-1 gap-5 md:grid-cols-3">
                                    {Horizon::ALL
                                        .into_iter()
                                        .map(|horizon| column(data, horizon))
                                        .collect_view()}
                                </div>
                            }
                        })
                })
            }}
        </section>
    }
}

/// One column's view, given only what it shows.
fn column(data: &Data, horizon: Horizon) -> impl IntoView + use<> {
    let heading = match horizon {
        Horizon::Now => format!("Now · {}", data.milestone),
        Horizon::Next => format!("Next · {}", data.next_milestone),
        other => other.label().to_string(),
    };
    let note = match horizon {
        Horizon::Later => Some(LATER.to_string()),
        _ => data.theme(horizon).map(String::from),
    };
    let progress = (horizon == Horizon::Now).then(|| {
        let (done, total) = data.progress();
        (done, total, data.milestone.clone())
    });
    let items: Vec<Item> = data.column(horizon).cloned().collect();
    view! { <Column horizon heading note progress items/> }
}

#[component]
fn Column(
    horizon: Horizon,
    heading: String,
    /// What the column's release is about (Later: that nothing is planned).
    note: Option<String>,
    progress: Option<(usize, usize, String)>,
    items: Vec<Item>,
) -> impl IntoView {
    let note = note.map(|note| {
        view! { <p data-roadmap-theme class="mt-1 text-xs text-rust-50/60">{note}</p> }
    });
    let progress = progress.map(|(done, total, milestone)| {
        let percent = (done * 100).checked_div(total).unwrap_or(0);
        view! {
            <div class="mt-2">
                <div
                    id="roadmap-progress"
                    role="progressbar"
                    aria-label=format!("{milestone} progress")
                    aria-valuemin="0"
                    aria-valuemax=total
                    aria-valuenow=done
                    class="h-1.5 overflow-hidden rounded-full bg-ink-900"
                >
                    <div class="h-full bg-rust-500" style=format!("width: {percent}%")></div>
                </div>
                <p class="mt-1 text-xs text-rust-50/60">{format!("{done} of {total} done")}</p>
            </div>
        }
    });

    view! {
        <div id=horizon.id() class="flex flex-col rounded-xl border border-rust-700/30 bg-ink-800 p-5">
            <h3 class="text-lg font-semibold text-rust-300">{heading}</h3>
            {note}
            {progress}
            <ul class="mt-4 flex flex-col gap-4">
                {items.into_iter().map(|item| view! { <RoadmapItem item/> }).collect_view()}
            </ul>
        </div>
    }
}

#[component]
fn RoadmapItem(item: Item) -> impl IntoView {
    let status = (item.horizon == Horizon::Now)
        .then(|| item.status.label())
        .flatten()
        .map(|label| {
            let tone = match item.status {
                Status::Done => "border-rust-500/50 bg-rust-500/15 text-rust-100",
                Status::InProgress => "border-rust-300/50 bg-rust-300/10 text-rust-300",
                Status::Planned | Status::Unknown => "border-rust-50/20 text-rust-50/60",
            };
            view! {
                <span
                    data-status=label
                    class=format!(
                        "shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider {tone}",
                    )
                >
                    {label}
                </span>
            }
        });
    let issue = item.issue.map(|number| {
        view! {
            <a href=format!("{ISSUES}/{number}") class="text-xs text-rust-300 underline hover:text-rust-500">
                {format!("#{number}")}
            </a>
        }
    });

    view! {
        <li id=item.id>
            <div class="flex items-start justify-between gap-2">
                <h4 class="font-semibold text-rust-50">{item.title}</h4>
                {status}
            </div>
            <p class="mt-1 text-sm text-rust-50/70">{item.blurb} " " {issue}</p>
        </li>
    }
}
