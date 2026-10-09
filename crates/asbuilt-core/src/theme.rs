//! Diagram colors for the documentation pages. Per element kind, a light
//! and a dark fill come from `[theme]` (LikeC4's default blue for a kind
//! with none), and each fill's text, secondary text and outline are
//! derived from it. `theme.css` sets them as `--c4-<kind>-*` variables for
//! the system's light and dark schemes and for a visitor's explicit
//! choice (`data-theme` on the root), then applies them to the nodes of an
//! inlined view. Group boxes, edges and edge labels take their colors
//! from the page tokens (`--bg`, `--fg`, `--muted`), so a host stylesheet
//! that restates those restyles the diagrams with the page.
//!
//! Pure: colors in, CSS out.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::config::ThemeColor;

/// The fill `likec4 gen dot` gives an element kind with no style.
pub const LIKEC4_DEFAULT_FILL: &str = "#3b82f6";

/// How far a group box's fill and outline move from the page background
/// toward its kind's color, light scheme then dark.
const GROUP_MIX: [(&str, &str); 2] = [("7%", "20%"), ("22%", "40%")];

/// A node's colors for one scheme, each `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeColors {
    pub fill: String,
    pub stroke: String,
    pub text: String,
    pub muted: String,
}

type Rgb = [f64; 3];

fn parse(hex: &str) -> Option<Rgb> {
    let digits = hex.strip_prefix('#').filter(|d| d.len() == 6)?;
    let channel = |i: usize| {
        u8::from_str_radix(&digits[i..i + 2], 16)
            .ok()
            .map(f64::from)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn to_hex(color: Rgb) -> String {
    let [r, g, b] = color.map(|c| c.round().clamp(0.0, 255.0) as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `to` mixed into `from` by `t`, channel by channel.
fn mix(from: Rgb, to: Rgb, t: f64) -> Rgb {
    [0, 1, 2].map(|i| from[i] + (to[i] - from[i]) * t)
}

/// Relative luminance, as WCAG defines it.
fn luminance(color: Rgb) -> f64 {
    let linear = |c: f64| {
        let c = c / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
}

/// Whether a fill of this luminance takes near-black text. The threshold
/// sits above the midpoint so saturated mid-tones (LikeC4's blue, a rust
/// red) keep white text; a fill exactly at it keeps light text too.
fn wants_dark_text(luminance: f64) -> bool {
    luminance > 0.45
}

/// A fill's colors: near-black text on a light fill and near-white on a
/// dark or saturated one (the threshold keeps white text on LikeC4's blue
/// and on a rust red), secondary text most of the way from the fill to the
/// text, and a hairline outline a step from the fill toward the text. A
/// fill that is not `#rrggbb` is LikeC4's default blue.
///
/// ```
/// let white = asbuilt_core::theme::node_colors("#ffffff");
/// assert_eq!(white.text, "#242424");
/// ```
pub fn node_colors(fill: &str) -> NodeColors {
    let default = parse(LIKEC4_DEFAULT_FILL).unwrap_or([59.0, 130.0, 246.0]);
    let base = parse(fill).unwrap_or(default);
    let light_fill = wants_dark_text(luminance(base));
    let (ink, strength) = if light_fill {
        ([0.0; 3], 0.86)
    } else {
        ([255.0; 3], 0.94)
    };
    let text = mix(base, ink, strength);
    NodeColors {
        fill: to_hex(base),
        stroke: to_hex(mix(base, text, 0.18)),
        text: to_hex(text),
        muted: to_hex(mix(base, text, 0.8)),
    }
}

/// A kind usable in a class name and a custom property name.
fn is_css_ident(kind: &str) -> bool {
    !kind.is_empty()
        && kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn kind_variables(out: &mut String, indent: &str, kind: &str, colors: &NodeColors) {
    let _ = writeln!(out, "{indent}--c4-{kind}-fill: {};", colors.fill);
    let _ = writeln!(out, "{indent}--c4-{kind}-stroke: {};", colors.stroke);
    let _ = writeln!(out, "{indent}--c4-{kind}-text: {};", colors.text);
    let _ = writeln!(out, "{indent}--c4-{kind}-muted: {};", colors.muted);
}

fn dark_variables(out: &mut String, indent: &str, kinds: &[(&String, NodeColors, NodeColors)]) {
    let (fill, stroke) = GROUP_MIX[1];
    let _ = writeln!(out, "{indent}--c4-group-fill-mix: {fill};");
    let _ = writeln!(out, "{indent}--c4-group-stroke-mix: {stroke};");
    for (kind, _, dark) in kinds {
        kind_variables(out, indent, kind, dark);
    }
}

/// `theme.css` for the element kinds a model has, colored from `[theme]`.
pub fn stylesheet(kinds: &BTreeSet<String>, theme: &BTreeMap<String, ThemeColor>) -> String {
    let colored: Vec<(&String, NodeColors, NodeColors)> = kinds
        .iter()
        .filter(|kind| is_css_ident(kind))
        .map(|kind| {
            let light = theme
                .get(kind)
                .map_or(LIKEC4_DEFAULT_FILL, |c| c.light.as_str());
            let dark = theme
                .get(kind)
                .and_then(|c| c.dark.as_deref())
                .unwrap_or(light);
            (kind, node_colors(light), node_colors(dark))
        })
        .collect();

    let mut out = String::from(
        "/* Diagram colors for the asbuilt docs pages, generated from [theme].\n   Group boxes, edges and labels follow the page tokens, and so does\n   each diagram's legend, styled here so a figure embedded with this\n   file alone has it; a host stylesheet linked after this one can\n   restate any variable. */\n",
    );
    out.push_str(":root {\n");
    out.push_str("  --c4-edge: color-mix(in srgb, var(--fg) 45%, var(--bg));\n");
    out.push_str("  --c4-edge-text: var(--muted);\n");
    out.push_str("  --c4-label-bg: color-mix(in srgb, var(--bg) 88%, transparent);\n");
    let (fill, stroke) = GROUP_MIX[0];
    let _ = writeln!(out, "  --c4-group-fill-mix: {fill};");
    let _ = writeln!(out, "  --c4-group-stroke-mix: {stroke};");
    for (kind, light, _) in &colored {
        kind_variables(&mut out, "  ", kind, light);
    }
    out.push_str(
        "}\n@media (prefers-color-scheme: dark) {\n  :root:not([data-theme=\"light\"]) {\n",
    );
    dark_variables(&mut out, "    ", &colored);
    out.push_str("  }\n}\n:root[data-theme=\"dark\"] {\n");
    dark_variables(&mut out, "  ", &colored);
    out.push_str("}\n");
    out.push_str(
        "svg.c4 { display: block; width: 100%; height: auto; }\n\
         svg.c4 .cluster text { fill: var(--muted); fill-opacity: 1; }\n\
         svg.c4 .edge path { stroke: var(--c4-edge); }\n\
         svg.c4 .edge :is(polygon, ellipse):not(.c4-label-bg):not([fill=\"none\"]) { fill: var(--c4-edge); stroke: var(--c4-edge); }\n\
         svg.c4 .edge :is(polygon, ellipse)[fill=\"none\"] { stroke: var(--c4-edge); }\n\
         svg.c4 .edge .c4-label-bg { fill: var(--c4-label-bg); fill-opacity: 1; }\n\
         svg.c4 .edge text { fill: var(--c4-edge-text); }\n\
         .legend ul { list-style: none; display: flex; flex-wrap: wrap; gap: 0.25rem 0.9rem; margin: 0.4rem 0 0; padding: 0; font-size: 0.8em; color: var(--muted); }\n\
         .legend li { display: inline-flex; align-items: center; gap: 0.35em; }\n\
         .legend-swatch { display: inline-block; width: 0.9em; height: 0.9em; border: 1px solid var(--rule); border-radius: 2px; }\n\
         .legend-line { width: 2.25em; height: 0.75em; overflow: visible; }\n\
         .legend-line line { stroke: var(--c4-edge); stroke-width: 1.5; stroke-linecap: round; }\n\
         .legend-line :is(polygon, circle):not([fill=\"none\"]) { fill: var(--c4-edge); stroke: var(--c4-edge); }\n\
         .legend-line :is(polygon, circle)[fill=\"none\"] { stroke: var(--c4-edge); stroke-width: 1.2; }\n",
    );
    for (kind, _, _) in &colored {
        let _ = writeln!(
            out,
            "svg.c4 .node.c4-k-{kind} > :is(polygon, path, ellipse, polyline) {{ fill: var(--c4-{kind}-fill); stroke: var(--c4-{kind}-stroke); stroke-width: 1px; vector-effect: non-scaling-stroke; }}"
        );
        let _ = writeln!(
            out,
            "svg.c4 .node.c4-k-{kind} text {{ fill: var(--c4-{kind}-text); }}"
        );
        let _ = writeln!(
            out,
            "svg.c4 .node.c4-k-{kind} text.c4-muted {{ fill: var(--c4-{kind}-muted); }}"
        );
        let _ = writeln!(
            out,
            "svg.c4 .cluster.c4-k-{kind} > :is(polygon, path) {{ fill: color-mix(in srgb, var(--c4-{kind}-fill) var(--c4-group-fill-mix), var(--bg)); stroke: color-mix(in srgb, var(--c4-{kind}-fill) var(--c4-group-stroke-mix), var(--bg)); }}"
        );
        let _ = writeln!(
            out,
            ".legend-swatch.c4-k-{kind} {{ background: var(--c4-{kind}-fill); border-color: var(--c4-{kind}-stroke); }}"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(fill: &str, stroke: &str, text: &str, muted: &str) -> NodeColors {
        NodeColors {
            fill: fill.into(),
            stroke: stroke.into(),
            text: text.into(),
            muted: muted.into(),
        }
    }

    #[test]
    fn a_light_fill_gets_near_black_text_and_a_grey_hairline() {
        assert_eq!(
            node_colors("#ffffff"),
            colors("#ffffff", "#d8d8d8", "#242424", "#505050")
        );
    }

    #[test]
    fn a_saturated_fill_keeps_near_white_text() {
        assert_eq!(
            node_colors("#ce422b"),
            colors("#ce422b", "#d6624f", "#fcf4f2", "#f3d0ca")
        );
    }

    #[test]
    fn a_dark_fill_gets_near_white_text() {
        assert_eq!(
            node_colors("#3d2c22"),
            colors("#3d2c22", "#5e5047", "#f3f2f2", "#cfcbc8")
        );
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} vs {expected}");
    }

    #[test]
    fn luminance_weights_red_green_and_blue_as_wcag_does() {
        close(luminance([255.0, 0.0, 0.0]), 0.2126);
        close(luminance([0.0, 255.0, 0.0]), 0.7152);
        close(luminance([0.0, 0.0, 255.0]), 0.0722);
    }

    #[test]
    fn luminance_follows_the_srgb_curve_above_and_below_its_linear_segment() {
        close(luminance([128.0, 128.0, 128.0]), 0.21586050011389923);
        close(luminance([5.0, 5.0, 5.0]), 0.0015176349177441874);
    }

    #[test]
    fn a_fill_exactly_at_the_threshold_keeps_light_text() {
        assert!(!wants_dark_text(0.45));
        assert!(wants_dark_text(0.4501));
    }

    #[test]
    fn a_fill_that_is_not_hex_is_likec4_s_blue() {
        assert_eq!(node_colors("blue"), node_colors(LIKEC4_DEFAULT_FILL));
    }

    fn kinds(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn block<'a>(css: &'a str, opener: &str) -> &'a str {
        let start = css
            .find(opener)
            .unwrap_or_else(|| panic!("no {opener} in {css}"));
        let rest = &css[start..];
        &rest[..rest.find('}').unwrap()]
    }

    #[test]
    fn a_themed_kind_gets_its_light_and_dark_fills_in_every_scheme_block() {
        let theme = BTreeMap::from([(
            "container".to_string(),
            ThemeColor {
                light: "#ce422b".into(),
                dark: Some("#8f2d19".into()),
            },
        )]);
        let css = stylesheet(&kinds(&["container"]), &theme);
        assert!(
            block(&css, ":root {").contains("--c4-container-fill: #ce422b;"),
            "{css}"
        );
        assert!(
            block(&css, ":root:not([data-theme=\"light\"])")
                .contains("--c4-container-fill: #8f2d19;"),
            "{css}"
        );
        assert!(
            block(&css, ":root[data-theme=\"dark\"]").contains("--c4-container-fill: #8f2d19;"),
            "{css}"
        );
    }

    #[test]
    fn a_kind_without_a_dark_color_keeps_its_light_one_in_the_dark() {
        let theme = BTreeMap::from([(
            "process".to_string(),
            ThemeColor {
                light: "#e9d4c8".into(),
                dark: None,
            },
        )]);
        let css = stylesheet(&kinds(&["process"]), &theme);
        assert!(
            block(&css, ":root[data-theme=\"dark\"]").contains("--c4-process-fill: #e9d4c8;"),
            "{css}"
        );
    }

    #[test]
    fn an_unthemed_kind_is_likec4_s_blue_in_both_schemes() {
        let css = stylesheet(&kinds(&["component"]), &BTreeMap::new());
        assert!(
            block(&css, ":root {").contains("--c4-component-fill: #3b82f6;"),
            "{css}"
        );
        assert!(
            block(&css, ":root[data-theme=\"dark\"]").contains("--c4-component-fill: #3b82f6;"),
            "{css}"
        );
    }

    #[test]
    fn every_kind_the_model_has_gets_node_and_group_box_rules_and_no_other_kind_does() {
        let theme = BTreeMap::from([(
            "browser".to_string(),
            ThemeColor {
                light: "#000000".into(),
                dark: None,
            },
        )]);
        let css = stylesheet(&kinds(&["component", "container"]), &theme);
        for kind in ["component", "container"] {
            assert!(
                css.contains(&format!("svg.c4 .node.c4-k-{kind} > ")),
                "{css}"
            );
            assert!(
                css.contains(&format!("svg.c4 .node.c4-k-{kind} text.c4-muted")),
                "{css}"
            );
            assert!(
                css.contains(&format!("svg.c4 .cluster.c4-k-{kind} > ")),
                "{css}"
            );
        }
        assert!(!css.contains("browser"), "{css}");
    }

    #[test]
    fn a_kind_that_is_not_a_css_identifier_is_skipped() {
        let css = stylesheet(
            &kinds(&["a b", "ok", "web-app", "db_store"]),
            &BTreeMap::new(),
        );
        assert!(!css.contains("a b"), "{css}");
        for kind in ["ok", "web-app", "db_store"] {
            assert!(css.contains(&format!("--c4-{kind}-fill")), "{css}");
        }
    }

    #[test]
    fn group_boxes_mix_toward_the_page_more_in_the_dark() {
        let css = stylesheet(&kinds(&["container"]), &BTreeMap::new());
        assert!(
            block(&css, ":root {").contains("--c4-group-fill-mix: 7%;"),
            "{css}"
        );
        assert!(
            block(&css, ":root[data-theme=\"dark\"]").contains("--c4-group-fill-mix: 22%;"),
            "{css}"
        );
        assert!(
            css.contains("fill: color-mix(in srgb, var(--c4-container-fill) var(--c4-group-fill-mix), var(--bg));"),
            "{css}"
        );
    }

    #[test]
    fn edges_and_labels_follow_the_page_tokens() {
        let css = stylesheet(&kinds(&["container"]), &BTreeMap::new());
        let root = block(&css, ":root {");
        assert!(
            root.contains("--c4-edge: color-mix(in srgb, var(--fg) 45%, var(--bg));"),
            "{css}"
        );
        assert!(root.contains("--c4-edge-text: var(--muted);"), "{css}");
        assert!(
            css.contains(
                "svg.c4 .edge .c4-label-bg { fill: var(--c4-label-bg); fill-opacity: 1; }"
            ),
            "{css}"
        );
    }

    #[test]
    fn a_filled_arrowhead_takes_the_edge_color() {
        let css = stylesheet(&kinds(&["container"]), &BTreeMap::new());
        assert!(
            css.contains("svg.c4 .edge :is(polygon, ellipse):not(.c4-label-bg):not([fill=\"none\"]) { fill: var(--c4-edge); stroke: var(--c4-edge); }"),
            "{css}"
        );
    }

    #[test]
    fn a_hollow_arrowhead_keeps_its_hollow_and_takes_the_edge_color_as_its_outline() {
        let css = stylesheet(&kinds(&["container"]), &BTreeMap::new());
        assert!(
            css.contains(
                "svg.c4 .edge :is(polygon, ellipse)[fill=\"none\"] { stroke: var(--c4-edge); }"
            ),
            "{css}"
        );
    }

    #[test]
    fn a_legend_swatch_takes_its_kind_s_colors() {
        let css = stylesheet(&kinds(&["container"]), &BTreeMap::new());
        assert!(
            css.contains(".legend-swatch.c4-k-container { background: var(--c4-container-fill); border-color: var(--c4-container-stroke); }"),
            "{css}"
        );
    }

    #[test]
    fn the_legend_is_styled_with_the_diagram_colors() {
        let css = stylesheet(&kinds(&["container"]), &BTreeMap::new());
        assert!(
            css.contains(".legend-line line { stroke: var(--c4-edge); stroke-width: 1.5; stroke-linecap: round; }"),
            "{css}"
        );
    }
}
