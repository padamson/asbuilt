use leptos::prelude::*;

use crate::roadmap::{self, Horizon, ITEMS, Item, MILESTONE, NEXT_MILESTONE, Status};

const ISSUES: &str = "https://github.com/padamson/asbuilt/issues";
const CHANGELOG: &str = "https://github.com/padamson/asbuilt/blob/main/CHANGELOG.md";

/// Now, Next and Later, from `crate::roadmap`. Now is the next release and
/// carries each item's status and a progress bar, Next the release after
/// it, and Later what is under consideration beyond that. What already
/// shipped is in the changelog.
#[component]
pub fn Roadmap() -> impl IntoView {
    view! {
        <section id="roadmap" class="mx-auto max-w-5xl px-6 py-12">
            <h2 class="text-2xl font-bold text-rust-300">"Roadmap"</h2>
            <p class="mt-2 mb-6 max-w-3xl text-sm text-rust-50/70">
                {format!("Now is {MILESTONE}, Next is {NEXT_MILESTONE}, and Later is under consideration beyond that. ")}
                "What already shipped is in the " <a href=CHANGELOG class="underline hover:text-rust-300">
                    "changelog"
                </a> "."
            </p>
            <div class="grid grid-cols-1 gap-5 md:grid-cols-3">
                {Horizon::ALL.into_iter().map(|horizon| view! { <Column horizon/> }).collect_view()}
            </div>
        </section>
    }
}

#[component]
fn Column(horizon: Horizon) -> impl IntoView {
    let heading = match horizon {
        Horizon::Now => format!("Now · {MILESTONE}"),
        Horizon::Next => format!("Next · {NEXT_MILESTONE}"),
        Horizon::Later => Horizon::Later.label().to_string(),
    };
    let note = (horizon == Horizon::Later).then(|| {
        view! {
            <p class="mt-1 text-xs text-rust-50/60">"Under consideration, not yet planned"</p>
        }
    });
    let progress = (horizon == Horizon::Now).then(|| {
        let (done, total) = roadmap::progress(ITEMS);
        let percent = (done * 100).checked_div(total).unwrap_or(0);
        view! {
            <div class="mt-2">
                <div
                    id="roadmap-progress"
                    role="progressbar"
                    aria-label=format!("{MILESTONE} progress")
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
                {roadmap::column(ITEMS, horizon)
                    .map(|item| view! { <RoadmapItem item=*item/> })
                    .collect_view()}
            </ul>
        </div>
    }
}

#[component]
fn RoadmapItem(item: Item) -> impl IntoView {
    let status = (item.horizon == Horizon::Now).then(|| {
        let tone = match item.status {
            Status::Done => "border-rust-500/50 bg-rust-500/15 text-rust-100",
            Status::InProgress => "border-rust-300/50 bg-rust-300/10 text-rust-300",
            Status::Planned => "border-rust-50/20 text-rust-50/60",
        };
        view! {
            <span
                data-status=item.status.label()
                class=format!(
                    "shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider {tone}",
                )
            >
                {item.status.label()}
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
