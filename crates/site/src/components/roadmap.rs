use leptos::prelude::*;
use leptos::task::spawn_local;

use super::icons::{self, Icon};
use crate::roadmap::{self, Criterion, Horizon, Item, Roadmap as Data, Status};
use crate::version::{fetch_json, is_dev};

const ISSUES: &str = "https://github.com/padamson/asbuilt/issues";
const CHANGELOG: &str = "https://github.com/padamson/asbuilt/blob/main/CHANGELOG.md";
/// Where a criterion's proving test is linked to, by its path from the root.
const SOURCE: &str = "https://github.com/padamson/asbuilt/blob/main";
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
    let criteria = (!item.acceptance.is_empty()).then(|| {
        view! {
            <Criteria
                id=format!("{}-criteria", item.id)
                title=item.title.clone()
                done=item.status == Status::Done
                criteria=item.acceptance
            />
        }
    });

    view! {
        <li id=item.id>
            <div class="flex items-start justify-between gap-2">
                <h4 class="font-semibold text-rust-50">{item.title}</h4>
                {status}
            </div>
            <p class="mt-1 text-sm text-rust-50/70">{item.blurb} " " {issue}</p>
            {criteria}
        </li>
    }
}

/// The criteria button's label: how many there are, which the popover
/// lists in full.
fn count_label(count: usize) -> String {
    if count == 1 {
        "1 criterion".to_string()
    } else {
        format!("{count} criteria")
    }
}

/// The item's acceptance criteria, behind a button, as a native popover:
/// the browser handles the top layer, Escape and a click outside. On a done
/// item each criterion links to its proving test, which a host test holds
/// to being in the tree. A planned item's tests may still be renamed or
/// split, so the page shows only its criteria.
#[component]
fn Criteria(id: String, title: String, done: bool, criteria: Vec<Criterion>) -> impl IntoView {
    let label = count_label(criteria.len());
    let mark = if done { icons::TICK } else { icons::BOX };
    let button_label = format!("{label} for {title}");
    let dialog_label = format!("Criteria for {title}");
    view! {
        <button
            type="button"
            popovertarget=id.clone()
            aria-label=button_label
            data-criteria-button
            class="mt-2 inline-flex items-center gap-1.5 rounded-md border border-rust-700/40 px-2.5 py-1.5 text-xs font-semibold text-rust-300 transition hover:border-rust-300"
        >
            <Icon path=mark class="h-3.5 w-3.5"/>
            {label}
        </button>
        <div
            id=id
            popover="auto"
            role="dialog"
            aria-label=dialog_label
            class="m-auto w-[min(32rem,calc(100vw-2rem))] rounded-xl border border-rust-700/40 bg-ink-800 p-6 text-left text-rust-50 shadow-2xl"
        >
            <div class="flex items-start gap-3">
                <div class="flex-1">
                    <p class="text-xs font-bold uppercase tracking-wider text-rust-300">"Done when"</p>
                    <h5 class="mt-1 text-lg font-bold">{title}</h5>
                </div>
                <button
                    type="button"
                    popovertarget=id.clone()
                    popovertargetaction="hide"
                    aria-label="Close criteria"
                    class="-mt-1 -mr-2 rounded p-2 text-rust-50/70 hover:text-rust-50"
                >
                    <Icon path=icons::CLOSE/>
                </button>
            </div>
            <ol class="mt-4 flex flex-col gap-3">
                {criteria
                    .into_iter()
                    .map(|c| {
                        // Main's file reaches an older snapshot unchecked by its
                        // host tests, so a test with no location is shown, not
                        // linked.
                        let href = c.test_location().map(|(path, _)| format!("{SOURCE}/{path}"));
                        let Criterion { criterion, test } = c;
                        let test = done.then(|| {
                            let shown = match href {
                                Some(href) => view! {
                                    <a href=href class="text-rust-50/60 underline hover:text-rust-300">{test}</a>
                                }
                                .into_any(),
                                None => view! { <span class="text-rust-50/60">{test}</span> }.into_any(),
                            };
                            view! { <code class="mt-0.5 block text-xs break-all">{shown}</code> }
                        });
                        view! {
                            <li data-criterion class="flex gap-3">
                                <Icon path=mark class="mt-0.5 h-4 w-4 shrink-0 text-rust-300"/>
                                <div class="min-w-0">
                                    <p class="text-sm">{criterion}</p>
                                    {test}
                                </div>
                            </li>
                        }
                    })
                    .collect_view()}
            </ol>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_criterion_is_singular() {
        assert_eq!(count_label(1), "1 criterion");
    }

    #[test]
    fn several_criteria_are_plural() {
        assert_eq!(count_label(4), "4 criteria");
    }
}
