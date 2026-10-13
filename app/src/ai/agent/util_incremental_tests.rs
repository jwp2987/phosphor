//! Tests for [`IncrementalTextSections`]: converting a streamed answer chunk by chunk must give
//! exactly what converting the whole text gives, after every chunk (#849).

use super::{IncrementalTextSections, parse_markdown_into_text_and_code_sections};
use crate::ai::agent::AIAgentTextSection;
use crate::features::FeatureFlag;

/// Agent-style answers covering every kind of section and the line-level quirks of the splitter.
const CORPUS: &[&str] = &[
    "",
    "plain",
    "\n\n\nleading blank lines are dropped\n",
    "Intro paragraph.\n\nSecond paragraph with `inline code` and **bold**.\n",
    "# Heading\n\n- item one\n- item two\n  - nested\n\n1. first\n2. second\n",
    "Here is code:\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nAnd after.\n",
    "```\n\n\nleading blank code lines\n```\n",
    "```python path=/src/app.py start=10\ndef f():\n    return 1\n```\n",
    "```mermaid\ngraph TD\n  A --> B\n```\ntext after diagram\n",
    "```bash\nunterminated fence\nstill code\n",
    "```\n```\nempty code block\n",
    "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n\nOutro\n",
    "Intro\n| A | B |\n| --- | --- |\n| 1 | 2 |\nnot a row\n",
    "| A | B | C |\n| --- | --- |\nseparator consumed by a rejected table\n",
    "| only | a header |\njust text\n",
    "| A | B |\n| --- | --- |\n| `a|b` | 2 |\n```\ncode right after table\n```\n",
    "![diagram](https://example.com/a.png)\n",
    "before\n![one](a.png)\nbetween\n![two](b.png \"title\") ![three](c.png)\nafter\n",
    "![first](a.png)\n\n![second](b.png)\n",
    "text ![inline](a.png) stays text\n",
    "`code span\nacross lines`\n\n**bold across\nlines**\n",
    "line one\r\nline two\r\n\r\n```\r\ncode\r\n```\r\n",
    "lone\rcarriage return\n",
    "<!-- comment -->\nvisible\n<!-- open\nhidden?\n",
    "mix\n```\nc\n```\n| t | u |\n| - | - |\n| 1 | 2 |\n![i](i.png)\n- list\n# head\n",
    "unicode \u{1f600} text\n```\n\u{4e2d}\u{6587}\n```\n\u{00e9}\n",
];

/// `AIAgentTextSection`'s equality compares a plain-text section's per-line raw text and links,
/// not the parsed lines the renderer draws, so those are compared separately.
fn assert_same_sections(actual: &[AIAgentTextSection], expected: &[AIAgentTextSection], at: &str) {
    assert_eq!(actual, expected, "{at}");
    for (actual, expected) in actual.iter().zip(expected) {
        if let (
            AIAgentTextSection::PlainText { text: actual },
            AIAgentTextSection::PlainText { text: expected },
        ) = (actual, expected)
        {
            assert_eq!(
                actual.formatted_text_arc().as_deref(),
                expected.formatted_text_arc().as_deref(),
                "{at}"
            );
        }
    }
}

fn assert_streams_like_full_conversion(text: &str, boundaries: impl IntoIterator<Item = usize>) {
    let mut parser = IncrementalTextSections::default();
    let mut sections: Vec<AIAgentTextSection> = Vec::new();
    for end in boundaries {
        let prefix = &text[..end];
        parser.update(prefix, &mut sections);
        assert_same_sections(
            &sections,
            &parse_markdown_into_text_and_code_sections(prefix),
            &format!("streamed sections diverged at byte {end} of {text:?}"),
        );
    }
}

fn char_boundaries(text: &str) -> Vec<usize> {
    (0..=text.len())
        .filter(|i| text.is_char_boundary(*i))
        .collect()
}

fn random_boundaries(text: &str, seed: u64, max_chunk: usize) -> Vec<usize> {
    // A fixed-seed LCG keeps the test deterministic without a dependency.
    let mut state = seed;
    let mut boundaries = Vec::new();
    let mut end = 0;
    while end < text.len() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        end = (end + 1 + (state >> 33) as usize % max_chunk).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        boundaries.push(end);
    }
    boundaries
}

#[test]
fn streaming_byte_by_byte_matches_full_conversion_at_every_step() {
    for table_rendering in [true, false] {
        let _rendering =
            FeatureFlag::BlocklistMarkdownTableRendering.override_enabled(table_rendering);
        for markdown_tables in [true, false] {
            let _tables = FeatureFlag::MarkdownTables.override_enabled(markdown_tables);
            for text in CORPUS {
                assert_streams_like_full_conversion(text, char_boundaries(text));
            }
        }
    }
}

#[test]
fn streaming_the_corpus_in_random_chunks_matches_full_conversion() {
    let _rendering = FeatureFlag::BlocklistMarkdownTableRendering.override_enabled(true);
    let _tables = FeatureFlag::MarkdownTables.override_enabled(true);
    let text = CORPUS.join("\n");
    for seed in 0..10 {
        assert_streams_like_full_conversion(&text, random_boundaries(&text, seed, 97));
    }
}

#[test]
fn recovers_when_handed_sections_it_did_not_produce() {
    let text = "para\n\n```\ncode\n```\nmore text\nand more\n";
    let mut parser = IncrementalTextSections::default();
    let mut sections = Vec::new();
    parser.update(&text[..20], &mut sections);

    // Something else replaced the output in between; the next update must still be right.
    sections.clear();
    parser.update(text, &mut sections);
    assert_same_sections(
        &sections,
        &parse_markdown_into_text_and_code_sections(text),
        "after the output was cleared",
    );

    sections = vec![AIAgentTextSection::PlainText {
        text: "unrelated".to_string().into(),
    }];
    parser.update(text, &mut sections);
    assert_same_sections(
        &sections,
        &parse_markdown_into_text_and_code_sections(text),
        "after the output was replaced",
    );
}

#[test]
fn streaming_a_megabyte_in_4k_chunks_does_work_proportional_to_its_size() {
    // Before #849 every chunk re-converted the whole answer: 1 MB in 4 KB chunks handed ~128 MB
    // to the Markdown parser. The incremental parser must stay proportional to the answer.
    let _rendering = FeatureFlag::BlocklistMarkdownTableRendering.override_enabled(true);
    let _tables = FeatureFlag::MarkdownTables.override_enabled(true);
    let block = "Some prose with *emphasis*, `code`, and a [link](https://example.com).\n\
                 - a list item\n  - nested item\n\n\
                 ```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n\
                 | a | b |\n| --- | --- |\n| 1 | 2 |\n\n\
                 ![image](https://example.com/i.png)\n\
                 A closing paragraph that runs on for a while to make the lines realistic.\n\n";
    let mut text = String::new();
    while text.len() < 1 << 20 {
        text.push_str(block);
    }

    let mut parser = IncrementalTextSections::default();
    let mut sections = Vec::new();
    let mut end = 0;
    let mut chunks = 0;
    while end < text.len() {
        end = (end + 4096).min(text.len());
        chunks += 1;
        parser.update(&text[..end], &mut sections);
    }
    assert_same_sections(
        &sections,
        &parse_markdown_into_text_and_code_sections(&text),
        "after streaming",
    );

    let total_lines = text.lines().count();
    assert!(
        parser.markdown_bytes_parsed() <= 2 * text.len(),
        "parsed {} Markdown bytes to stream {} bytes",
        parser.markdown_bytes_parsed(),
        text.len()
    );
    assert!(
        parser.lines_split() <= total_lines + 8 * chunks,
        "split {} lines to stream {total_lines} lines in {chunks} chunks",
        parser.lines_split()
    );
}

#[test]
fn one_huge_paragraph_run_is_not_reparsed_per_chunk() {
    // The case that froze the app: prose with no code blocks is a single plain-text section,
    // which the full conversion re-parsed in its entirety on every chunk.
    let line = "A long answer line with **bold** text and `code` in it.\n";
    let text = line.repeat((1 << 20) / line.len());

    let mut parser = IncrementalTextSections::default();
    let mut sections = Vec::new();
    let mut end = 0;
    while end < text.len() {
        end = (end + 4096).min(text.len());
        parser.update(&text[..end], &mut sections);
    }
    assert_same_sections(
        &sections,
        &parse_markdown_into_text_and_code_sections(&text),
        "after streaming",
    );
    assert!(
        parser.markdown_bytes_parsed() <= 2 * text.len(),
        "parsed {} Markdown bytes to stream {} bytes",
        parser.markdown_bytes_parsed(),
        text.len()
    );
}
