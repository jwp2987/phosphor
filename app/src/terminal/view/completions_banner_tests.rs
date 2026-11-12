//! #2179: the "completions are not working" banner and the ssh error block's button must not
//! advise the removed tmux phosphorization option, in any language.

const LOCALES: [(&str, &str); 3] = [
    ("en", include_str!("../../../i18n/en/warp.ftl")),
    ("ja", include_str!("../../../i18n/ja/warp.ftl")),
    ("zh-CN", include_str!("../../../i18n/zh-CN/warp.ftl")),
];

fn value<'a>(ftl: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key} =");
    ftl.lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
}

#[test]
fn the_completions_banner_does_not_mention_tmux() {
    for (locale, ftl) in LOCALES {
        for part in ["prefix", "middle", "suffix"] {
            let key = format!("terminal-banner-completions-not-working-{part}");
            let text = value(ftl, &key).unwrap_or_else(|| panic!("{locale}: {key} missing"));
            assert!(
                !text.to_lowercase().contains("tmux"),
                "{locale}: {key} still mentions tmux: {text}"
            );
            assert!(!text.trim().is_empty(), "{locale}: {key} is empty");
        }
    }
}

#[test]
fn the_ssh_error_block_button_does_not_mention_tmux() {
    for (locale, ftl) in LOCALES {
        let text = value(ftl, "terminal-warpify-without-tmux")
            .unwrap_or_else(|| panic!("{locale}: label missing"));
        assert!(
            !text.to_lowercase().contains("tmux"),
            "{locale}: the button label still mentions tmux: {text}"
        );
    }
}
