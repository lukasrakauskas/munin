//! Lossless source typography and native Markdown / Mermaid reading view.
use rune::{El, Span, TextStyle};
use rune_kit::{Kit, Markdown, MarkdownStyle};

pub fn reading(source: &str, k: &Kit, width: f32) -> El {
    let style = MarkdownStyle::from_kit(k)
        .body_font(TextStyle::new(15.0, 400.0).lh(25.0))
        .heading_font(1, TextStyle::new(30.0, 650.0).lh(38.0))
        .heading_font(2, TextStyle::new(24.0, 600.0).lh(32.0))
        .heading_font(3, TextStyle::new(20.0, 600.0).lh(28.0))
        .strong_weight(700.0)
        .text_width(width.max(1.0))
        .table_full_width(true)
        .diagram_max_width(width.max(1.0))
        .diagram(k.c.layer_01, k.c.border_focus, k.c.text_accent);
    Markdown::new(source)
        .style(style)
        .scroll_wide(true)
        .tasks_read_only()
        .selectable(false)
        .on_link_tx(|s: &mut crate::state::State, url, tx| {
            if (url.starts_with("https://")
                || url.starts_with("http://")
                || url.starts_with("mailto:"))
                && let Err(e) = tx.open_url(url)
            {
                s.error = Some(e.to_string());
            }
        })
        .build(k)
        .key("rendered-note")
        .w_full()
        .min_w(0.0)
}

pub fn source_spans(source: &str, k: &Kit) -> Vec<Span> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let mut spans = rune_kit::syntax::SyntaxTheme::from_kit(k)
        .spans(&rune_kit::syntax::highlight(Some("markdown"), source), 0);
    for (event, range) in Parser::new_ext(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    )
    .into_offset_iter()
    {
        let span = Span::new(range.start, range.end);
        let styled = match event {
            Event::Start(Tag::Strong) => span.weight(700.0),
            Event::Start(Tag::Emphasis) => span.italic(),
            Event::Start(Tag::Heading { level, .. }) => {
                let size = [28.0, 24.0, 20.0, 18.0, 16.0, 15.0][level as usize - 1];
                span.size(size).weight(650.0).color(k.c.text_base)
            }
            Event::Code(_) => span.mono().bg(k.alpha(k.c.text_base, 0.06), 2.0),
            Event::Start(Tag::Link { .. }) => {
                span.color(k.c.text_accent).underline(k.c.text_accent)
            }
            _ => continue,
        };
        spans.push(styled);
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at<'a>(spans: &'a [Span], source: &str, needle: &str) -> Vec<&'a Span> {
        let index = source.find(needle).unwrap() as u32;
        spans
            .iter()
            .filter(|s| s.start <= index && s.end >= index + needle.len() as u32)
            .collect()
    }
    #[test]
    fn source_styles_nested_unicode_emphasis_without_changing_offsets() {
        let source = "Žąsis **bold *italic 🐦*** and `**literal**`";
        let spans = source_spans(source, &Kit::neutral(false));
        assert!(
            at(&spans, source, "bold")
                .iter()
                .any(|s| s.weight.is_some_and(|w| w >= 600.0))
        );
        assert!(at(&spans, source, "italic 🐦").iter().any(|s| s.italic));
        assert!(
            !at(&spans, source, "literal")
                .iter()
                .any(|s| s.weight.is_some() || s.italic)
        );
        assert!(
            spans
                .iter()
                .all(|s| source.is_char_boundary(s.start as usize)
                    && source.is_char_boundary(s.end as usize))
        );
    }
    #[test]
    fn source_headings_have_hierarchy_but_code_stays_literal() {
        let source = "# First\n\n### Third\n\n```md\n**not bold**\n# not a heading\n```\n";
        let spans = source_spans(source, &Kit::neutral(true));
        assert!(
            at(&spans, source, "First")
                .iter()
                .any(|s| s.size.is_some_and(|v| v >= 26.0))
        );
        assert!(
            at(&spans, source, "Third")
                .iter()
                .any(|s| s.size.is_some_and(|v| (18.0..26.0).contains(&v)))
        );
        assert!(
            !at(&spans, source, "not bold")
                .iter()
                .any(|s| s.weight.is_some())
        );
        assert!(
            !at(&spans, source, "not a heading")
                .iter()
                .any(|s| s.size.is_some())
        );
    }
}
