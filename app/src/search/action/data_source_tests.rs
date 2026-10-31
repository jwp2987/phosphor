use super::*;
use warpui::keymap::BindingDescription;

fn binding(name: &str, description: &str) -> Arc<CommandBinding> {
    Arc::new(CommandBinding {
        name: name.to_owned(),
        description: BindingDescription::new_preserve_case(description),
        trigger: None,
        action: None,
        group: None,
        id: BindingId::new(),
    })
}

fn searcher_with(bindings: &[Arc<CommandBinding>]) -> FuzzyActionSearcher {
    FuzzyActionSearcher {
        all_bindings: bindings.iter().map(|b| (b.id, b.clone())).collect(),
        searchable: Default::default(),
    }
}

/// The searchable string is exactly what the per-query code used to compute inline.
#[test]
fn searchable_binding_matches_previous_inline_computation() {
    let b = binding("workspace:show_theme_chooser", "Open Theme Chooser ÄÖ");
    let searchable = SearchableBinding::new(&b);

    let mut expected = b
        .description
        .in_context(DescriptionContext::Default)
        .to_lowercase();
    expected.push(' ');
    expected.push_str("workspace show theme chooser");
    assert_eq!(searchable.text, expected);
    assert_eq!(
        searchable.description_char_len,
        "Open Theme Chooser ÄÖ".chars().count()
    );

    // No name tokens: no separator is appended.
    let unnamed = SearchableBinding::new(&binding("", "Split Pane"));
    assert_eq!(unnamed.text, "split pane");
}

/// Results are identical whether or not the searchable strings were precomputed by
/// `build_index` (the fallback path computes them per query, as the old code did).
#[test]
fn search_results_identical_with_and_without_precomputed_strings() {
    let bindings = [
        binding("workspace:show_theme_chooser", "Open Theme Chooser"),
        binding("terminal:split_right", "Split Pane Right"),
        binding("terminal:clear_blocks", "Clear Blocks"),
        binding("workspace:search_drive", "Search Drive"),
    ];
    let cold = searcher_with(&bindings);
    let mut warm = searcher_with(&bindings);
    warm.build_index();
    assert_eq!(warm.searchable.len(), bindings.len());

    for query in [
        "theme",
        "THEME",
        "spr",
        "clear",
        "workspace show",
        "zzz",
        "",
    ] {
        let scores = |searcher: &FuzzyActionSearcher| {
            let mut scores: Vec<_> = searcher
                .search(query)
                .unwrap()
                .iter()
                .map(|result| result.score())
                .collect();
            scores.sort();
            scores
        };
        assert_eq!(scores(&cold), scores(&warm), "query {query:?}");
    }
    // A real match is found.
    assert!(!warm.search("theme").unwrap().is_empty());
}
