//! Markdown conversion for compose body rendering.
//!
//! Compose bodies are stored as Markdown source. At send time, the message
//! builder produces a multipart/alternative MIME body with two parts:
//! - `text/plain` rendered via [`markdown_to_plaintext`]
//! - `text/html` rendered via [`markdown_to_html`]
//!
//! Recipients on plain-text mail clients see a readable plain version.
//! Recipients on HTML mail clients see the formatted version.
//!
//! ## Security
//!
//! [`markdown_to_html`] starts from [`pulldown_cmark::Options::empty`] and
//! enables only tables, strikethrough, and tasklists — all other extensions
//! are off. Critically, raw HTML literals in the Markdown source (e.g.
//! `<script>`) are then stripped by filtering out [`pulldown_cmark::Event::Html`]
//! and [`pulldown_cmark::Event::InlineHtml`] events before they reach the
//! HTML renderer.
//!
//! Note: pulldown-cmark 0.12 has NO `ENABLE_HTML` option — its parser always
//! emits `Event::Html` for raw HTML in the source, and the default html
//! renderer always passes those bytes through verbatim. The only safe way to
//! drop raw HTML at the markdown layer is to remove those events from the
//! event stream, which is what we do.
//!
//! The compose preview pipeline ALSO runs the output through
//! `inboxly-ui::sanitize::sanitize_html` (M34's existing sanitiser) as
//! defence in depth. The two layers protect against different threats:
//! the event filter here blocks Markdown-source XSS, and `sanitize_html`
//! blocks rendered-HTML XSS that could come from other sources (e.g. paste
//! from external HTML).

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Render Markdown source to HTML.
///
/// Enables tables, strikethrough, and tasklists. Filters out raw HTML
/// events so `<script>` in the source is dropped. Does NOT enable footnotes
/// (out of scope for compose). Does NOT enable smart punctuation (would
/// produce surprising character substitutions in technical messages).
#[must_use]
pub fn markdown_to_html(md: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(md, options).filter(|event| {
        // Drop raw HTML events at the parse layer — pulldown-cmark 0.12 has
        // no ENABLE_HTML option, so the only safe block is event filtering.
        !matches!(event, Event::Html(_) | Event::InlineHtml(_))
    });
    let mut html = String::with_capacity(md.len() + md.len() / 4);
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// Render Markdown source to readable plain text.
///
/// Designed for the multipart/alternative `text/plain` body part. The output
/// is meant for human readers on plain-text mail clients, not for machine
/// re-parsing.
///
/// Format choices:
/// - Paragraphs are separated by a single blank line.
/// - List items are prefixed with `- ` (unordered) or `1. `, `2. ` (ordered).
/// - Links render as `text (https://url)`.
/// - Inline code is wrapped in backticks: `` `code` ``.
/// - Code blocks are fenced with triple backticks (preserving the language tag if any).
/// - Headings render as `# title`, `## title`, etc., matching their level.
/// - Block quotes are prefixed with `> `.
/// - Bold and italic markers are stripped (plain text has no equivalent).
/// - HTML literals (which `Options::empty()` already drops at the HTML
///   layer) are also skipped here.
#[must_use]
pub fn markdown_to_plaintext(md: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(md, options);

    let mut out = String::with_capacity(md.len());
    // Some(n) = ordered counter, None = unordered.
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut link_url: Option<String> = None;
    let mut needs_blank_line_before_next_block = false;

    fn ensure_paragraph_break(out: &mut String, needs: &mut bool) {
        if *needs {
            if !out.is_empty() && !out.ends_with("\n\n") {
                if out.ends_with('\n') {
                    out.push('\n');
                } else {
                    out.push_str("\n\n");
                }
            }
            *needs = false;
        }
    }

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    ensure_paragraph_break(&mut out, &mut needs_blank_line_before_next_block);
                }
                Tag::Heading { level, .. } => {
                    ensure_paragraph_break(&mut out, &mut needs_blank_line_before_next_block);
                    let hashes = match level {
                        HeadingLevel::H1 => "#",
                        HeadingLevel::H2 => "##",
                        HeadingLevel::H3 => "###",
                        HeadingLevel::H4 => "####",
                        HeadingLevel::H5 => "#####",
                        HeadingLevel::H6 => "######",
                    };
                    out.push_str(hashes);
                    out.push(' ');
                }
                Tag::BlockQuote(_) => {
                    ensure_paragraph_break(&mut out, &mut needs_blank_line_before_next_block);
                    out.push_str("> ");
                }
                Tag::CodeBlock(kind) => {
                    ensure_paragraph_break(&mut out, &mut needs_blank_line_before_next_block);
                    out.push_str("```");
                    if let CodeBlockKind::Fenced(lang) = kind {
                        out.push_str(&lang);
                    }
                    out.push('\n');
                }
                Tag::List(start) => {
                    ensure_paragraph_break(&mut out, &mut needs_blank_line_before_next_block);
                    list_stack.push(start);
                }
                Tag::Item => {
                    if let Some(counter) = list_stack.last_mut() {
                        match counter {
                            Some(n) => {
                                out.push_str(&format!("{n}. "));
                                *n = n.saturating_add(1);
                            }
                            None => out.push_str("- "),
                        }
                    }
                }
                Tag::Link { dest_url, .. } => {
                    link_url = Some(dest_url.into_string());
                }
                Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Image { .. } => {}
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::BlockQuote(_) => {
                    out.push('\n');
                    needs_blank_line_before_next_block = true;
                }
                TagEnd::CodeBlock => {
                    out.push_str("```\n");
                    needs_blank_line_before_next_block = true;
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    needs_blank_line_before_next_block = true;
                }
                TagEnd::Item => {
                    out.push('\n');
                }
                TagEnd::Link => {
                    if let Some(url) = link_url.take() {
                        out.push_str(" (");
                        out.push_str(&url);
                        out.push(')');
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                out.push_str(&text);
            }
            Event::Code(text) => {
                out.push('`');
                out.push_str(&text);
                out.push('`');
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                // Ignored — Options::empty() blocks raw HTML at parse,
                // but defence-in-depth: we don't echo HTML even if it leaks.
            }
            Event::SoftBreak => {
                out.push(' ');
            }
            Event::HardBreak => {
                out.push('\n');
            }
            Event::Rule => {
                ensure_paragraph_break(&mut out, &mut needs_blank_line_before_next_block);
                out.push_str("---\n");
                needs_blank_line_before_next_block = true;
            }
            Event::TaskListMarker(checked) => {
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            _ => {}
        }
    }

    // Trim a trailing blank line if present.
    while out.ends_with("\n\n") {
        out.pop();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{markdown_to_html, markdown_to_plaintext};

    // ===== markdown_to_html (10 tests) =====

    #[test]
    fn html_bold() {
        let html = markdown_to_html("**bold**");
        assert!(html.contains("<strong>bold</strong>"), "got: {html}");
    }

    #[test]
    fn html_unordered_list() {
        let html = markdown_to_html("- one\n- two\n- three");
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("<li>three</li>"));
    }

    #[test]
    fn html_ordered_list() {
        let html = markdown_to_html("1. first\n2. second");
        assert!(html.contains("<ol"));
        assert!(html.contains("<li>first</li>"));
    }

    #[test]
    fn html_link() {
        let html = markdown_to_html("[Inboxly](https://inboxly.example)");
        assert!(html.contains("href=\"https://inboxly.example\""));
        assert!(html.contains(">Inboxly</a>"));
    }

    #[test]
    fn html_inline_code() {
        let html = markdown_to_html("use `cargo build`");
        assert!(html.contains("<code>cargo build</code>"));
    }

    #[test]
    fn html_fenced_code_block() {
        let html = markdown_to_html("```rust\nlet x = 1;\n```");
        assert!(html.contains("<pre>"));
        assert!(html.contains("let x = 1;"));
    }

    #[test]
    fn html_heading() {
        let html = markdown_to_html("# Title\n\n## Sub");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<h2>Sub</h2>"));
    }

    #[test]
    fn html_table() {
        let html = markdown_to_html("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>a</th>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn html_strikethrough() {
        let html = markdown_to_html("~~struck~~");
        assert!(html.contains("<del>struck</del>"));
    }

    #[test]
    fn html_raw_html_dropped() {
        // Issue 2.2 invariant: Options::empty() must drop raw HTML in the source.
        // The <script> tag must NOT appear unescaped in the rendered output.
        let html = markdown_to_html("Hello <script>alert(1)</script> world");
        assert!(!html.contains("<script>"), "raw script leaked: {html}");
        // The literal text might survive as escaped text — that's OK,
        // the dangerous part is the unescaped tag execution.
    }

    // ===== markdown_to_plaintext (5 tests) =====

    #[test]
    fn plain_paragraphs_separated_by_blank_line() {
        let plain = markdown_to_plaintext("First paragraph.\n\nSecond paragraph.");
        assert!(
            plain.contains("First paragraph.\n\nSecond paragraph."),
            "got: {plain:?}"
        );
    }

    #[test]
    fn plain_unordered_list_dash_prefix() {
        let plain = markdown_to_plaintext("- one\n- two\n- three");
        assert!(plain.contains("- one\n"), "got: {plain:?}");
        assert!(plain.contains("- two\n"), "got: {plain:?}");
        assert!(plain.contains("- three\n"), "got: {plain:?}");
    }

    #[test]
    fn plain_link_renders_text_and_url() {
        let plain = markdown_to_plaintext("Visit [Inboxly](https://inboxly.example) today");
        assert!(
            plain.contains("Inboxly (https://inboxly.example)"),
            "got: {plain:?}"
        );
    }

    #[test]
    fn plain_code_block_fenced() {
        let plain = markdown_to_plaintext("```\nlet x = 1;\n```");
        assert!(plain.contains("```"), "got: {plain:?}");
        assert!(plain.contains("let x = 1;"), "got: {plain:?}");
    }

    #[test]
    fn plain_heading_hash_prefix() {
        let plain = markdown_to_plaintext("## Section title");
        assert!(plain.contains("## Section title"), "got: {plain:?}");
    }
}
