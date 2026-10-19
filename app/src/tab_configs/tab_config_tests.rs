use std::collections::HashMap;
use std::path::Path;

use crate::launch_configs::launch_config::{PaneTemplateType, SplitDirection};

use super::*;

const WORKTREE_TOML: &str = r#"
name = "New Worktree"
title = "{{worktree_branch_name}}"

[[panes]]
id = "main"
type = "terminal"
directory = "{{repo}}"
commands = [
  "git worktree add -b {{worktree_branch_name}} -- {{worktree_path}} {{branch}}",
  "cd {{worktree_path}}",
]

[params.repo]
type = "repo"
description = "Absolute path to repository"

[params.branch]
type = "branch"
description = "Base branch to branch from"

[params.worktree_branch_name]
type = "text"
description = "New worktree branch name"
default = "my-feature-branch"
"#;

/// The generated worktree path for `repo`/`branch`, as the one shell word commands receive.
fn expected_path_word(repo: &str, branch: &str) -> String {
    portable_shell_word(&generated_worktree_path_string(Path::new(repo), branch))
        .expect("test paths are portable")
        .into_owned()
}

fn build_test_tab_config_toml(name: &str, commands: Vec<String>) -> String {
    let config = TabConfig {
        name: name.to_string(),
        title: None,
        color: None,
        panes: vec![TabConfigPaneNode {
            id: "main".to_string(),
            pane_type: Some(TabConfigPaneType::Terminal),
            split: None,
            children: None,
            is_focused: None,
            directory: Some("/Users/me/repo".to_string()),
            commands: Some(commands),
            shell: None,
        }],
        params: HashMap::new(),
        source_path: None,
    };

    toml::to_string_pretty(&config).expect("Test config should serialize")
}

#[test]
fn test_generated_worktree_path_uses_repo_name_directory() {
    let repo_dir = generated_worktree_repo_dir(Path::new("/Users/me/backend"));

    assert_eq!(
        repo_dir,
        warp_core::paths::data_dir()
            .join("worktrees")
            .join("backend")
    );
    assert_eq!(
        generated_worktree_path(Path::new("/Users/me/backend"), "mesa-coyote"),
        repo_dir.join("mesa-coyote")
    );
}

#[test]
fn test_parse_worktree_toml() {
    let config: TabConfig = toml::from_str(WORKTREE_TOML).expect("Should parse worktree TOML");

    assert_eq!(config.name, "New Worktree");
    assert_eq!(config.title.as_deref(), Some("{{worktree_branch_name}}"));
    assert_eq!(config.panes.len(), 1);
    assert_eq!(config.panes[0].id, "main");
    assert_eq!(config.panes[0].directory.as_deref(), Some("{{repo}}"));
    assert_eq!(
        config.panes[0].commands.as_deref().unwrap(),
        &[
            "git worktree add -b {{worktree_branch_name}} -- {{worktree_path}} {{branch}}",
            "cd {{worktree_path}}"
        ]
    );
    assert_eq!(config.params.len(), 3);
    assert_eq!(config.params["repo"].param_type, TabConfigParamType::Repo);
    assert_eq!(
        config.params["branch"].param_type,
        TabConfigParamType::Branch
    );
    assert_eq!(
        config.params["worktree_branch_name"].param_type,
        TabConfigParamType::Text
    );
    assert!(config.params["repo"].default.is_none());
    assert!(config.params["branch"].default.is_none());
    assert_eq!(
        config.params["worktree_branch_name"].default.as_deref(),
        Some("my-feature-branch")
    );
}

#[test]
fn test_parse_minimal_toml() {
    let toml = r#"name = "Plain Tab""#;
    let config: TabConfig = toml::from_str(toml).expect("Should parse minimal TOML");

    assert_eq!(config.name, "Plain Tab");
    assert!(config.title.is_none());
    assert!(config.panes.is_empty());
    assert!(config.params.is_empty());
}

#[test]
fn test_parse_toml_with_on_close_fails() {
    let toml = r#"
name = "Legacy Tab"

[[panes]]
id = "main"
type = "terminal"

[on_close]
commands = ["echo cleanup"]
"#;

    let error = toml::from_str::<TabConfig>(toml).expect_err("on_close should be rejected");
    assert!(error.to_string().contains("unknown field `on_close`"));
}

#[test]
fn test_default_param_values() {
    let config: TabConfig = toml::from_str(WORKTREE_TOML).unwrap();
    let defaults = config.default_param_values();
    assert_eq!(defaults["branch"], "");
    assert_eq!(defaults["repo"], "");
    assert_eq!(defaults["worktree_branch_name"], "my-feature-branch");
}

#[test]
fn test_is_worktree() {
    let worktree_config: TabConfig = toml::from_str(WORKTREE_TOML).unwrap();
    assert!(worktree_config.is_worktree());

    let plain_config: TabConfig = toml::from_str(
        r#"
name = "Plain Tab"

[[panes]]
id = "main"
type = "terminal"
commands = ["pwd", "ls"]
"#,
    )
    .unwrap();
    assert!(!plain_config.is_worktree());
}

#[test]
fn test_render_tab_config_substitutes_values() {
    let config: TabConfig = toml::from_str(WORKTREE_TOML).unwrap();

    let mut params = HashMap::new();
    params.insert("repo".to_string(), "/Users/me/repo".to_string());
    params.insert("branch".to_string(), "main".to_string());
    params.insert("worktree_branch_name".to_string(), "my-feature".to_string());

    let (title, pane_template) = render_tab_config(&config, &params, None);

    assert_eq!(title.as_deref(), Some("my-feature"));

    if let crate::launch_configs::launch_config::PaneTemplateType::PaneTemplate {
        cwd,
        commands,
        ..
    } = pane_template
    {
        assert_eq!(cwd, std::path::PathBuf::from("/Users/me/repo"));
        // Commands should have shell-quoted values.
        let path_word = expected_path_word("/Users/me/repo", "my-feature");
        assert_eq!(
            commands[0].exec,
            format!("git worktree add -b my-feature -- {path_word} main")
        );
        assert_eq!(commands[1].exec, format!("cd {path_word}"));
    } else {
        panic!("Expected PaneTemplate variant");
    }
}

#[test]
fn test_render_tab_config_shell_quotes_commands_with_spaces() {
    let config: TabConfig = toml::from_str(WORKTREE_TOML).unwrap();

    let mut params = HashMap::new();
    params.insert("repo".to_string(), "/Users/me/my project".to_string());
    params.insert("branch".to_string(), "release-candidate".to_string());
    params.insert("worktree_branch_name".to_string(), "my feature".to_string());

    let (_, pane_template) = render_tab_config(&config, &params, None);

    if let crate::launch_configs::launch_config::PaneTemplateType::PaneTemplate {
        commands, ..
    } = pane_template
    {
        // Values with spaces should be quoted in commands.
        assert!(
            commands[0].exec.contains("'my feature'"),
            "Expected shell-quoted worktree branch name in command: {}",
            commands[0].exec
        );
    } else {
        panic!("Expected PaneTemplate variant");
    }
}

#[test]
fn test_render_tab_config_multi_pane() {
    let toml = r#"
name = "Split Tab"

[[panes]]
id = "root"
split = "horizontal"
children = ["left", "right"]

[[panes]]
id = "left"
type = "terminal"
directory = "{{repo}}"
commands = [
  "git worktree add -b {{worktree_branch_name}} -- {{worktree_path}} {{branch}}",
  "cd {{worktree_path}}",
]

[[panes]]
id = "right"
type = "terminal"
directory = "{{repo}}"
commands = ["nvim"]

[params.repo]
type = "repo"
description = "Repo path"

[params.branch]
type = "branch"
description = "Base branch to branch from"

[params.worktree_branch_name]
type = "text"
description = "New worktree branch name"
"#;

    let config: TabConfig = toml::from_str(toml).expect("Should parse multi-pane TOML");
    let mut params = HashMap::new();
    params.insert("repo".to_string(), "/Users/me/repo".to_string());
    params.insert("branch".to_string(), "main".to_string());
    params.insert("worktree_branch_name".to_string(), "my-feature".to_string());

    let (title, pane_template) = render_tab_config(&config, &params, None);
    assert!(title.is_none());

    if let PaneTemplateType::PaneBranchTemplate {
        split_direction,
        panes,
    } = pane_template
    {
        assert_eq!(split_direction, SplitDirection::Horizontal);
        assert_eq!(panes.len(), 2);

        // First pane should be focused and have two commands.
        if let PaneTemplateType::PaneTemplate {
            cwd,
            commands,
            is_focused,
            ..
        } = &panes[0]
        {
            assert_eq!(*cwd, std::path::PathBuf::from("/Users/me/repo"));
            assert_eq!(commands.len(), 2);
            let path_word = expected_path_word("/Users/me/repo", "my-feature");
            assert_eq!(
                commands[0].exec,
                format!("git worktree add -b my-feature -- {path_word} main")
            );
            assert_eq!(commands[1].exec, format!("cd {path_word}"));
            assert_eq!(*is_focused, Some(true));
        } else {
            panic!("Expected PaneTemplate for first child");
        }

        // Second pane should not be focused.
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = &panes[1] {
            assert_eq!(*is_focused, Some(false));
        } else {
            panic!("Expected PaneTemplate for second child");
        }
    } else {
        panic!("Expected PaneBranchTemplate");
    }
}

#[test]
fn test_render_tab_config_cwd_is_not_quoted() {
    let config: TabConfig = toml::from_str(WORKTREE_TOML).unwrap();

    let mut params = HashMap::new();
    params.insert("repo".to_string(), "/Users/me/my project".to_string());
    params.insert("branch".to_string(), "main".to_string());
    params.insert("worktree_branch_name".to_string(), "my-feature".to_string());

    let (_, pane_template) = render_tab_config(&config, &params, None);

    if let crate::launch_configs::launch_config::PaneTemplateType::PaneTemplate { cwd, .. } =
        pane_template
    {
        // cwd should be unquoted (raw path).
        assert_eq!(cwd, std::path::PathBuf::from("/Users/me/my project"));
    } else {
        panic!("Expected PaneTemplate variant");
    }
}

// ── Flat pane format tests ──────────────────────────────────────────

#[test]
fn test_flat_single_pane() {
    let toml_str = r#"
name = "Single"

[[panes]]
id = "main"
type = "terminal"
directory = "~/code/project"
commands = ["npm run dev"]
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse flat single pane");
    assert_eq!(config.panes.len(), 1);
    assert_eq!(config.panes[0].id, "main");

    let (_, template) = render_tab_config(&config, &HashMap::new(), None);
    if let PaneTemplateType::PaneTemplate {
        commands,
        is_focused,
        ..
    } = template
    {
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].exec, "npm run dev");
        // Single pane should be auto-focused.
        assert_eq!(is_focused, Some(true));
    } else {
        panic!("Expected PaneTemplate for single flat pane");
    }
}

#[test]
fn test_flat_two_pane_split() {
    let toml_str = r#"
name = "Split"

[[panes]]
id = "root"
split = "horizontal"
children = ["left", "right"]

[[panes]]
id = "left"
type = "terminal"
directory = "~/code/frontend"
commands = ["npm start"]

[[panes]]
id = "right"
type = "terminal"
directory = "~/code/backend"
commands = ["cargo run"]
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse flat split");
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);

    if let PaneTemplateType::PaneBranchTemplate {
        split_direction,
        panes,
    } = template
    {
        assert_eq!(split_direction, SplitDirection::Horizontal);
        assert_eq!(panes.len(), 2);

        // First leaf should be auto-focused.
        if let PaneTemplateType::PaneTemplate {
            commands,
            is_focused,
            ..
        } = &panes[0]
        {
            assert_eq!(commands[0].exec, "npm start");
            assert_eq!(*is_focused, Some(true));
        } else {
            panic!("Expected PaneTemplate for left child");
        }

        // Second leaf should not be focused.
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = &panes[1] {
            assert_eq!(*is_focused, Some(false));
        } else {
            panic!("Expected PaneTemplate for right child");
        }
    } else {
        panic!("Expected PaneBranchTemplate");
    }
}

#[test]
fn test_flat_2x2_grid() {
    let toml_str = r#"
name = "Grid"

[[panes]]
id = "root"
split = "horizontal"
children = ["left_col", "right_col"]

[[panes]]
id = "left_col"
split = "vertical"
children = ["tl", "bl"]

[[panes]]
id = "tl"
type = "terminal"
directory = "~/a"

[[panes]]
id = "bl"
type = "terminal"
directory = "~/b"

[[panes]]
id = "right_col"
split = "vertical"
children = ["tr", "br"]

[[panes]]
id = "tr"
type = "terminal"
directory = "~/c"

[[panes]]
id = "br"
type = "terminal"
directory = "~/d"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse flat 2x2");
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);

    // Root should be a horizontal split.
    let PaneTemplateType::PaneBranchTemplate {
        split_direction: root_dir,
        panes: root_children,
    } = template
    else {
        panic!("Expected root PaneBranchTemplate");
    };
    assert_eq!(root_dir, SplitDirection::Horizontal);
    assert_eq!(root_children.len(), 2);

    // Left column should be a vertical split with 2 children.
    let PaneTemplateType::PaneBranchTemplate {
        split_direction: left_dir,
        panes: left_children,
    } = &root_children[0]
    else {
        panic!("Expected left_col PaneBranchTemplate");
    };
    assert_eq!(*left_dir, SplitDirection::Vertical);
    assert_eq!(left_children.len(), 2);

    // Top-left should be focused (first leaf in tree, auto-focus).
    if let PaneTemplateType::PaneTemplate { is_focused, .. } = &left_children[0] {
        assert_eq!(*is_focused, Some(true));
    } else {
        panic!("Expected PaneTemplate for tl");
    }

    // Bottom-left should not be focused.
    if let PaneTemplateType::PaneTemplate { is_focused, .. } = &left_children[1] {
        assert_eq!(*is_focused, Some(false), "Expected bl to not be focused");
    } else {
        panic!("Expected PaneTemplate for bl");
    }

    // Right column should be a vertical split with 2 children.
    let PaneTemplateType::PaneBranchTemplate {
        panes: right_children,
        ..
    } = &root_children[1]
    else {
        panic!("Expected right_col PaneBranchTemplate");
    };
    for (label, pane) in [("tr", &right_children[0]), ("br", &right_children[1])] {
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = pane {
            assert_eq!(
                *is_focused,
                Some(false),
                "Expected {label} to not be focused"
            );
        } else {
            panic!("Expected PaneTemplate for {label}");
        }
    }
}

#[test]
fn test_flat_explicit_focus() {
    let toml_str = r#"
name = "Focus Test"

[[panes]]
id = "root"
split = "horizontal"
children = ["left", "right"]

[[panes]]
id = "left"
type = "terminal"
directory = "~/a"

[[panes]]
id = "right"
type = "terminal"
directory = "~/b"
is_focused = true
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse");
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);

    if let PaneTemplateType::PaneBranchTemplate { panes, .. } = template {
        // Left should NOT be focused (explicit focus is on right).
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = &panes[0] {
            assert_eq!(*is_focused, Some(false));
        }
        // Right should be focused.
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = &panes[1] {
            assert_eq!(*is_focused, Some(true));
        }
    } else {
        panic!("Expected PaneBranchTemplate");
    }
}

#[test]
fn test_flat_auto_focus_first_leaf() {
    let toml_str = r#"
name = "Auto Focus"

[[panes]]
id = "root"
split = "horizontal"
children = ["left", "right"]

[[panes]]
id = "left"
type = "terminal"
directory = "~/a"

[[panes]]
id = "right"
type = "terminal"
directory = "~/b"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse");
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);

    if let PaneTemplateType::PaneBranchTemplate { panes, .. } = template {
        // First leaf (left) should be auto-focused.
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = &panes[0] {
            assert_eq!(*is_focused, Some(true));
        }
        // Second leaf should not be focused.
        if let PaneTemplateType::PaneTemplate { is_focused, .. } = &panes[1] {
            assert_eq!(*is_focused, Some(false));
        }
    } else {
        panic!("Expected PaneBranchTemplate");
    }
}

#[test]
fn test_flat_color_deserialized() {
    let toml_str = r#"
name = "Colored Tab"
color = "blue"

[[panes]]
id = "main"
type = "terminal"
directory = "~/code"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse with color");
    assert_eq!(
        config.color,
        Some(crate::themes::theme::AnsiColorIdentifier::Blue)
    );
}

#[test]
fn test_flat_missing_child_ref_falls_back() {
    let toml_str = r#"
name = "Bad Ref"

[[panes]]
id = "root"
split = "horizontal"
children = ["left", "nonexistent"]

[[panes]]
id = "left"
type = "terminal"
directory = "~/a"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse");
    // render_tab_config should fall back to a single empty terminal.
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);
    if let PaneTemplateType::PaneTemplate {
        commands,
        is_focused,
        ..
    } = template
    {
        assert!(commands.is_empty());
        assert_eq!(is_focused, Some(true));
    } else {
        panic!("Expected fallback PaneTemplate on error");
    }
}

#[test]
fn test_flat_duplicate_ids_falls_back() {
    let toml_str = r#"
name = "Dup IDs"

[[panes]]
id = "main"
type = "terminal"
directory = "~/a"

[[panes]]
id = "main"
type = "terminal"
directory = "~/b"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse");
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);
    // Should fall back because of duplicate IDs.
    if let PaneTemplateType::PaneTemplate {
        commands,
        is_focused,
        ..
    } = template
    {
        assert!(commands.is_empty());
        assert_eq!(is_focused, Some(true));
    } else {
        panic!("Expected fallback PaneTemplate on duplicate ID error");
    }
}

#[test]
fn test_flat_with_params() {
    let toml_str = r#"
name = "Param Test"
title = "{{branch}}"

[[panes]]
id = "main"
type = "terminal"
directory = "{{repo}}"
commands = ["git checkout {{branch}}"]

[params.repo]
type = "repo"
description = "Repo path"

[params.branch]
type = "branch"
description = "Branch"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse with params");
    let mut params = HashMap::new();
    params.insert("repo".to_string(), "/Users/me/code".to_string());
    params.insert("branch".to_string(), "main".to_string());

    let (title, template) = render_tab_config(&config, &params, None);
    assert_eq!(title.as_deref(), Some("main"));

    if let PaneTemplateType::PaneTemplate { cwd, commands, .. } = template {
        assert_eq!(cwd, std::path::PathBuf::from("/Users/me/code"));
        assert_eq!(commands[0].exec, "git checkout main");
    } else {
        panic!("Expected PaneTemplate");
    }
}

#[test]
fn test_worktree_template_substitution() {
    let worktree_path = generated_worktree_path_string(
        Path::new("/Users/me/repo"),
        "{{autogenerated_branch_name}}",
    );
    let toml_str = build_test_tab_config_toml(
        "Template Worktree",
        vec![
            format!("git worktree add -b {{{{autogenerated_branch_name}}}} {worktree_path} main"),
            format!("cd {worktree_path}"),
        ],
    );
    let config: TabConfig = toml::from_str(&toml_str).expect("Should parse");
    let (_, template) = render_tab_config(&config, &HashMap::new(), Some("mesa-coyote"));

    if let PaneTemplateType::PaneTemplate { commands, .. } = template {
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].exec,
            format!(
                "git worktree add -b mesa-coyote {} main",
                generated_worktree_path_string(Path::new("/Users/me/repo"), "mesa-coyote")
            )
        );
        assert_eq!(
            commands[1].exec,
            format!(
                "cd {}",
                generated_worktree_path_string(Path::new("/Users/me/repo"), "mesa-coyote")
            )
        );
    } else {
        panic!("Expected PaneTemplate");
    }
}

#[test]
fn test_worktree_custom_commands_with_template() {
    let worktree_path = generated_worktree_path_string(
        Path::new("/Users/me/repo"),
        "{{autogenerated_branch_name}}",
    );
    let toml_str = build_test_tab_config_toml(
        "Custom Worktree",
        vec![
            format!("git worktree add -b {{{{autogenerated_branch_name}}}} {worktree_path} main"),
            format!("cd {worktree_path}"),
            "gt branch create".to_string(),
            "npm install".to_string(),
        ],
    );
    let config: TabConfig = toml::from_str(&toml_str).expect("Should parse");
    let (_, template) = render_tab_config(&config, &HashMap::new(), Some("mesa-coyote"));

    if let PaneTemplateType::PaneTemplate { commands, .. } = template {
        assert_eq!(commands.len(), 4);
        assert_eq!(
            commands[0].exec,
            format!(
                "git worktree add -b mesa-coyote {} main",
                generated_worktree_path_string(Path::new("/Users/me/repo"), "mesa-coyote")
            )
        );
        assert_eq!(
            commands[1].exec,
            format!(
                "cd {}",
                generated_worktree_path_string(Path::new("/Users/me/repo"), "mesa-coyote")
            )
        );
        assert_eq!(commands[2].exec, "gt branch create");
        assert_eq!(commands[3].exec, "npm install");
    } else {
        panic!("Expected PaneTemplate");
    }
}

#[test]
fn test_build_worktree_toml_autogenerate_round_trips() {
    let toml_str =
        build_worktree_config_toml("Worktree: my-project", "/Users/me/repo", "main", None)
            .expect("plain names are accepted");
    let config: TabConfig = toml::from_str(&toml_str).expect("Generated TOML should parse");

    assert_eq!(config.name, "Worktree: my-project");
    assert!(config.title.is_none());
    assert!(config.params.is_empty());
    assert_eq!(config.panes.len(), 1);

    let pane = &config.panes[0];
    assert!(config.uses_autogenerated_branch_name());
    assert!(pane.commands.is_some());
    assert_eq!(pane.directory.as_deref(), Some("/Users/me/repo"));

    // Verify rendering produces the correct commands.
    let (_, template) = render_tab_config(&config, &HashMap::new(), Some("obsidian-hawk"));
    if let PaneTemplateType::PaneTemplate { commands, .. } = template {
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].exec,
            format!(
                "git worktree add -b obsidian-hawk -- {} main",
                expected_path_word("/Users/me/repo", "obsidian-hawk")
            )
        );
        assert_eq!(
            commands[1].exec,
            format!(
                "cd {}",
                expected_path_word("/Users/me/repo", "obsidian-hawk")
            )
        );
    } else {
        panic!("Expected PaneTemplate");
    }
}

#[test]
fn test_build_worktree_toml_manual_round_trips() {
    let toml_str = build_worktree_config_toml(
        "Worktree: my-project",
        "/Users/me/repo",
        "main",
        Some("my-feature"),
    )
    .expect("plain names are accepted");
    let config: TabConfig = toml::from_str(&toml_str).expect("Generated TOML should parse");

    assert_eq!(config.name, "Worktree: my-project");
    assert_eq!(config.title.as_deref(), Some("{{worktree_branch_name}}"));
    assert!(config.params.contains_key("worktree_branch_name"));
    assert_eq!(
        config.params["worktree_branch_name"].param_type,
        TabConfigParamType::Text
    );
    assert_eq!(config.panes.len(), 1);

    let pane = &config.panes[0];
    assert!(!config.uses_autogenerated_branch_name());
    assert!(pane.commands.is_some());

    // Verify rendering substitutes the branch name into commands and title.
    let mut params = HashMap::new();
    params.insert("worktree_branch_name".to_string(), "my-feature".to_string());
    let (title, template) = render_tab_config(&config, &params, None);
    assert_eq!(title.as_deref(), Some("my-feature"));

    if let PaneTemplateType::PaneTemplate { commands, .. } = template {
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].exec,
            format!(
                "git worktree add -b my-feature -- {} main",
                expected_path_word("/Users/me/repo", "my-feature")
            )
        );
        assert_eq!(
            commands[1].exec,
            format!("cd {}", expected_path_word("/Users/me/repo", "my-feature"))
        );
    } else {
        panic!("Expected PaneTemplate");
    }
}

#[test]
fn test_flat_split_with_fewer_than_two_children_falls_back() {
    let toml_str = r#"
name = "Bad Split"

[[panes]]
id = "root"
split = "horizontal"
children = ["only"]

[[panes]]
id = "only"
type = "terminal"
directory = "~/a"
"#;
    let config: TabConfig = toml::from_str(toml_str).expect("Should parse");
    let (_, template) = render_tab_config(&config, &HashMap::new(), None);
    // Should fall back because split needs >= 2 children.
    if let PaneTemplateType::PaneTemplate { is_focused, .. } = template {
        assert_eq!(is_focused, Some(true));
    } else {
        panic!("Expected fallback PaneTemplate");
    }
}

// ── #878: values interpolated into worktree commands ───────────────

/// Values that reach tab config commands, with the word [`portable_shell_word`] writes for
/// each (`None`: no spelling every shell reads alike, so the command must not run).
///
/// Every `Some` spelling was checked in real shells (bash 5.3, zsh 5.9, fish 4.2):
/// `printf '%s' WORD` reproduced the value byte for byte and nothing was executed.
const PORTABLE_WORDS: &[(&str, Option<&str>)] = &[
    ("main", Some("main")),
    ("feat/x-1.2", Some("feat/x-1.2")),
    ("/home/me/repo", Some("/home/me/repo")),
    ("_tmp:1", Some("_tmp:1")),
    ("", Some("''")),
    ("my feature", Some("'my feature'")),
    // A leading `-` would be a PowerShell parameter name, a leading digit a number literal.
    ("-x", Some("'-x'")),
    ("1kb", Some("'1kb'")),
    ("$(touch pwned)", Some("'$(touch pwned)'")),
    ("`touch pwned`", Some("'`touch pwned`'")),
    ("a;touch pwned", Some("'a;touch pwned'")),
    ("$(id)", Some("'$(id)'")),
    ("a\"b", Some("'a\"b'")),
    ("a!b", Some("'a!b'")),
    ("{{worktree_path}}", Some("'{{worktree_path}}'")),
    ("C:\\Users\\me\\repo", Some("'C:\\Users\\me\\repo'")),
    // A `'` (or a typographic one, which PowerShell also closes on) needs double quotes...
    ("it's", Some("\"it's\"")),
    ("it\u{2019}s", Some("\"it\u{2019}s\"")),
    // ...which cannot hold `$`, `` ` ``, `"`, `\` or `!` portably.
    ("it's $(id)", None),
    ("a'`id`", None),
    ("a'\"b", None),
    ("a'!b", None),
    // fish reads `\\` and `\'` as escapes even inside single quotes.
    ("a\\\\b", None),
    ("trail\\", None),
    // Control characters would reach the PTY raw (#867).
    ("a\rexit", None),
    ("a\nb", None),
    ("a\u{1b}[2J", None),
    ("a\u{7f}", None),
    ("a\u{2028}b", None),
];

#[test]
fn test_portable_shell_word_spellings() {
    for &(value, expected) in PORTABLE_WORDS {
        assert_eq!(
            portable_shell_word(value).as_deref(),
            expected,
            "spelling of {value:?}"
        );
    }
}

/// Repository directory names that were shell syntax inside the old `cd "<path>"`.
const HOSTILE_REPO_NAMES: &[&str] = &[
    "$(touch pwned)",
    "`touch pwned`",
    "a\"; touch pwned; echo \"",
    "it's",
    "a;touch pwned",
];

fn render_commands(
    config: &TabConfig,
    params: &HashMap<String, String>,
    autogenerated: Option<&str>,
) -> Vec<String> {
    match render_tab_config(config, params, autogenerated).1 {
        PaneTemplateType::PaneTemplate { commands, .. } => {
            commands.into_iter().map(|command| command.exec).collect()
        }
        PaneTemplateType::PaneBranchTemplate { .. } => panic!("Expected PaneTemplate"),
    }
}

#[test]
fn test_hostile_repo_name_is_quoted_as_part_of_one_path_word() {
    for name in HOSTILE_REPO_NAMES {
        let repo = format!("/Users/me/{name}");
        for manual in [None, Some("my-feature")] {
            let toml_str = build_worktree_config_toml("Worktree", &repo, "main", manual)
                .expect("a hostile repo name is quoted, not refused");
            assert!(
                !toml_str.contains(&format!("{name}/")),
                "the repo name must not be baked into a command: {toml_str}"
            );
            let config: TabConfig = toml::from_str(&toml_str).expect("Generated TOML parses");
            let (params, autogenerated, branch) = match manual {
                Some(branch) => (
                    HashMap::from([("worktree_branch_name".to_string(), branch.to_string())]),
                    None,
                    branch,
                ),
                None => (HashMap::new(), Some("mesa-coyote"), "mesa-coyote"),
            };
            let path_word = expected_path_word(&repo, branch);
            assert!(
                path_word.starts_with('\'') || path_word.starts_with('"'),
                "{name:?} must be quoted: {path_word}"
            );
            assert_eq!(
                render_commands(&config, &params, autogenerated),
                [
                    format!("git worktree add -b {branch} -- {path_word} main"),
                    format!("cd {path_word}"),
                ],
                "repo {name:?}"
            );
        }
    }
}

#[test]
fn test_session_config_hostile_repo_name_is_quoted() {
    use super::super::session_config::{SessionType, build_tab_config};
    for name in HOSTILE_REPO_NAMES {
        let repo = format!("/home/user/{name}");
        let config = build_tab_config(&SessionType::Terminal, Path::new(&repo), true, true);
        let path_word = expected_path_word(&repo, "mesa-coyote");
        assert_eq!(
            render_commands(&config, &HashMap::new(), Some("mesa-coyote")),
            [
                format!("git worktree add -b mesa-coyote -- {path_word}"),
                format!("cd {path_word}"),
            ],
            "repo {name:?}"
        );
    }
}

#[test]
fn test_hostile_branch_names_are_single_words() {
    // Base branches: git refnames may contain all of these except the control character.
    for (base, word) in [
        ("a;touch pwned", "'a;touch pwned'"),
        ("$(id)", "'$(id)'"),
        ("`id`", "'`id`'"),
        // After `--`, a leading `-` is a positional argument, not a git option.
        ("-x", "'-x'"),
    ] {
        let toml_str = build_worktree_config_toml("Worktree", "/Users/me/repo", base, None)
            .expect("quotable base branches are accepted");
        let config: TabConfig = toml::from_str(&toml_str).expect("Generated TOML parses");
        assert_eq!(
            render_commands(&config, &HashMap::new(), Some("mesa-coyote"))[0],
            format!(
                "git worktree add -b mesa-coyote -- {} {word}",
                expected_path_word("/Users/me/repo", "mesa-coyote")
            ),
            "base {base:?}"
        );
    }

    // A manually named branch is quoted both as `-b`'s value and inside the path word.
    let toml_str =
        build_worktree_config_toml("Worktree", "/Users/me/repo", "main", Some("x")).unwrap();
    let config: TabConfig = toml::from_str(&toml_str).unwrap();
    for (branch, word) in [("$(id)", "'$(id)'"), ("a;touch pwned", "'a;touch pwned'")] {
        let params = HashMap::from([("worktree_branch_name".to_string(), branch.to_string())]);
        let path_word = expected_path_word("/Users/me/repo", branch);
        assert!(path_word.starts_with('\''), "{path_word}");
        assert_eq!(
            render_commands(&config, &params, None),
            [
                format!("git worktree add -b {word} -- {path_word} main"),
                format!("cd {path_word}"),
            ],
            "branch {branch:?}"
        );
    }
}

#[test]
fn test_unquotable_base_branch_is_refused() {
    for base in [
        "it's $(id)",
        "a'`id`",
        "{{worktree_branch_name}}",
        "a}}b",
        "a\rb",
    ] {
        assert!(
            build_worktree_config_toml("Worktree", "/Users/me/repo", base, None).is_err(),
            "base {base:?} must be refused"
        );
    }
    // No base branch: git uses HEAD, and no empty word is passed.
    let toml_str = build_worktree_config_toml("Worktree", "/Users/me/repo", "", None).unwrap();
    let config: TabConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(
        render_commands(&config, &HashMap::new(), Some("mesa-coyote"))[0],
        format!(
            "git worktree add -b mesa-coyote -- {}",
            expected_path_word("/Users/me/repo", "mesa-coyote")
        )
    );
}

#[test]
fn test_unquotable_value_runs_no_commands() {
    // A control character in a repo name or param has no portable spelling: the render falls
    // back to an empty terminal instead of typing anything.
    let repo = "/Users/me/a\rtouch pwned";
    let toml_str = build_worktree_config_toml("Worktree", repo, "main", None).unwrap();
    let config: TabConfig = toml::from_str(&toml_str).unwrap();
    assert!(render_commands(&config, &HashMap::new(), Some("mesa-coyote")).is_empty());

    let config: TabConfig = toml::from_str(WORKTREE_TOML).unwrap();
    let params = HashMap::from([
        ("repo".to_string(), "/Users/me/repo".to_string()),
        ("branch".to_string(), "main".to_string()),
        (
            "worktree_branch_name".to_string(),
            "x'$(touch pwned)".to_string(),
        ),
    ]);
    assert!(render_commands(&config, &params, None).is_empty());
}

#[test]
fn test_legacy_generated_worktree_path_is_requoted() {
    // Configs saved before #878 baked `"<worktrees dir>/<repo>/{{branch}}"` into commands.
    for name in HOSTILE_REPO_NAMES {
        let repo = format!("/Users/me/{name}");
        for branch_var in ["autogenerated_branch_name", "worktree_branch_name"] {
            let legacy_path =
                generated_worktree_path_string(Path::new(&repo), &format!("{{{{{branch_var}}}}}"));
            let mut config: TabConfig = toml::from_str(&build_test_tab_config_toml(
                "Legacy",
                vec![
                    format!("git worktree add -b {{{{{branch_var}}}}} \"{legacy_path}\" main"),
                    format!("cd \"{legacy_path}\""),
                ],
            ))
            .unwrap();
            config.panes[0].directory = Some(repo.clone());
            let (params, autogenerated) = if branch_var == "worktree_branch_name" {
                (
                    HashMap::from([(branch_var.to_string(), "my-feature".to_string())]),
                    None,
                )
            } else {
                (HashMap::new(), Some("my-feature"))
            };
            let path_word = expected_path_word(&repo, "my-feature");
            assert_eq!(
                render_commands(&config, &params, autogenerated),
                [
                    format!("git worktree add -b my-feature {path_word} main"),
                    format!("cd {path_word}"),
                ],
                "repo {name:?}, {branch_var}"
            );
        }
    }
}

#[test]
fn test_worktree_path_param_overrides_builtin() {
    let mut config: TabConfig = toml::from_str(&build_test_tab_config_toml(
        "Custom",
        vec!["cd {{worktree_path}}".to_string()],
    ))
    .unwrap();
    config.panes[0].directory = Some("/Users/me/repo".to_string());
    let params = HashMap::from([("worktree_path".to_string(), "/tmp/my wt".to_string())]);
    assert_eq!(
        render_commands(&config, &params, Some("mesa-coyote")),
        ["cd '/tmp/my wt'"]
    );
}

fn render_single_command(command: &str, value: &str) -> Result<Vec<String>, String> {
    let config: TabConfig = toml::from_str(&build_test_tab_config_toml(
        "Quoting",
        vec![command.to_string()],
    ))
    .unwrap();
    let params = HashMap::from([("x".to_string(), value.to_string())]);
    try_render_tab_config(&config, &params, None).map(|(_, template)| match template {
        PaneTemplateType::PaneTemplate { commands, .. } => {
            commands.into_iter().map(|command| command.exec).collect()
        }
        PaneTemplateType::PaneBranchTemplate { .. } => panic!("Expected PaneTemplate"),
    })
}

#[test]
fn test_placeholder_wrapped_in_template_quotes_is_unwrapped() {
    // `cd "{{x}}"` used to become `cd "'$(touch pwned)'"`, which runs the substitution.
    for value in ["$(touch pwned)", "`touch pwned`", "a\"b", "it's"] {
        let word = portable_shell_word(value).unwrap().into_owned();
        for command in ["cd \"{{x}}\"", "cd '{{x}}'", "cd {{x}}"] {
            assert_eq!(
                render_single_command(command, value),
                Ok(vec![format!("cd {word}")]),
                "{command} with {value:?}"
            );
        }
        assert_eq!(
            render_single_command("cd \"{{x}}\" && echo \"{{x}}\" '{{x}}'", value),
            Ok(vec![format!("cd {word} && echo {word} {word}")]),
            "{value:?}"
        );
    }
}

#[test]
fn test_placeholder_near_shell_syntax_is_refused() {
    for command in [
        // Inside, or next to, the template's own quotes.
        "cd \"/tmp/{{x}}\"",
        "cd '/tmp/{{x}}'",
        "echo \"a {{x}} b\"",
        "echo '{{x}} b'",
        "echo \"'{{x}}'\"",
        "echo \u{2018}{{x}}\u{2019}",
        // Shapes that defeated quote tracking (#878 review): nested quotes in `$(...)`, a
        // default in `${...}`, an apostrophe in a comment, a heredoc body, arithmetic.
        "cd \"$(git -C \"{{x}}\" rev-parse --show-toplevel)\"",
        "echo \"${y:-\"{{x}}\"}\"",
        "# don't\necho '{{x}}'",
        "cat <<EOF\n{{x}}\nEOF",
        "echo $(( {{x}} ))",
        "echo $(basename {{x}})",
        "echo `basename {{x}}`",
        "echo {{x}} # note",
        // Escapes, and `$` directly before the word (`$'...'` is ANSI-C quoting in bash).
        "echo \\'{{x}}",
        "echo \\{{x}}",
        "echo ${{x}}",
        "echo $'{{x}}'",
    ] {
        let err = render_single_command(command, "$(touch pwned)")
            .expect_err(&format!("{command:?} must be refused"));
        assert!(err.contains("$(touch pwned)"), "{command:?}: {err}");
        // The infallible render falls back to an empty terminal.
        let config: TabConfig = toml::from_str(&build_test_tab_config_toml(
            "Quoting",
            vec![command.to_string()],
        ))
        .unwrap();
        let params = HashMap::from([("x".to_string(), "$(touch pwned)".to_string())]);
        assert!(
            render_commands(&config, &params, None).is_empty(),
            "{command:?}"
        );
    }
}

#[test]
fn test_plain_commands_with_placeholders_render() {
    assert_eq!(
        render_single_command(
            "git checkout {{x}} && npm install && ls -la {{x}}/src > out.txt",
            "$(touch pwned)"
        ),
        Ok(vec![
            "git checkout '$(touch pwned)' && npm install && ls -la '$(touch pwned)'/src > out.txt"
                .to_string()
        ])
    );
    // `$NAME` without a brace or paren is allowed, and `#` inside a word is not a comment.
    assert_eq!(
        render_single_command("cd $HOME/src/{{x}} && echo issue#5", "main"),
        Ok(vec!["cd $HOME/src/main && echo issue#5".to_string()])
    );
    // Commands without a substituted placeholder are not restricted.
    assert_eq!(
        render_single_command("echo \"$(date)\" 'x' # {{missing}}", "main"),
        Ok(vec!["echo \"$(date)\" 'x' # {{missing}}".to_string()])
    );
    // A balanced `{{{x}}}` is handlebars' escape: left literal, nothing substituted.
    assert_eq!(
        render_single_command("echo {{{x}}}", "$(touch pwned)"),
        Ok(vec!["echo {{{x}}}".to_string()])
    );
}

#[test]
fn test_unsafe_render_error_names_the_value() {
    let err = render_single_command("cd {{x}}", "a\rexit").unwrap_err();
    assert!(err.contains("`x`") && err.contains("a\\rexit"), "{err}");
}

#[test]
fn test_legacy_worktree_config_with_unquoted_hostile_base_is_refused() {
    let repo = "/Users/me/repo";
    let legacy_path =
        generated_worktree_path_string(Path::new(repo), "{{autogenerated_branch_name}}");
    for (base, refused) in [("main", false), ("a;>pwned", true), ("$(id)", true)] {
        let config: TabConfig = toml::from_str(&build_test_tab_config_toml(
            "Legacy",
            vec![format!(
                "git worktree add -b {{{{autogenerated_branch_name}}}} \"{legacy_path}\" {base}"
            )],
        ))
        .unwrap();
        let result = try_render_tab_config(&config, &HashMap::new(), Some("mesa-coyote"));
        assert_eq!(result.is_err(), refused, "base {base:?}: {result:?}");
    }
}

#[test]
fn test_placeholder_in_arithmetic_or_subscript_is_refused() {
    // A branch named `feature/a$(touch pwned)` is a valid refname; inside `$[ ]` or a
    // subscript the quoted word is evaluated as arithmetic, which expands `$(...)`.
    for command in [
        "echo $[ {{x}} ]",
        "echo $[{{x}}]",
        "echo ${arr[{{x}}]}",
        "echo arr[{{x}}]",
        "ls *(e:{{x}}:)",
    ] {
        let err = render_single_command(command, "feature/a$(touch pwned)")
            .expect_err(&format!("{command:?} must be refused"));
        assert!(
            err.contains("feature/a$(touch pwned)"),
            "{command:?}: {err}"
        );
    }
}

#[test]
fn test_placeholder_passed_to_reparsing_command_is_refused() {
    for command in [
        "ssh host cat {{x}}",
        "eval echo {{x}}",
        "bash -c {{x}}",
        "sh -lc {{x}}",
        "/usr/bin/zsh -c {{x}}",
        "fish -c {{x}}",
        "watch ls {{x}}",
        "tmux new-session ls {{x}}",
        "screen -dm ls {{x}}",
        "docker exec box ls {{x}}",
        "podman exec box ls {{x}}",
        "kubectl exec pod -- ls {{x}}",
        "su root -c {{x}}",
        "sudo sh -c {{x}}",
        "sudo -u me bash -c {{x}}",
        "FOO=1 env ssh host {{x}}",
        "echo {{x}} | xargs rm",
        "find . -name {{x}} -exec rm {} +",
        "script -c {{x}} out.log",
        "trap {{x}} EXIT",
        "iex {{x}}",
        "Invoke-Expression {{x}}",
        "git -c alias.x=!sh {{x}}",
        "cd /tmp && ssh host {{x}}",
    ] {
        let err = render_single_command(command, "main")
            .expect_err(&format!("{command:?} must be refused"));
        assert!(
            err.contains("as code") || err.contains("evaluate code") || err.contains("runs a shell"),
            "{command:?}: {err}"
        );
    }
    // Ordinary commands, including ones that merely mention those words, still render.
    for command in [
        "git checkout {{x}}",
        "npm run dev -- {{x}}",
        "sudo apt install {{x}}",
        "find . -name {{x}}",
        "git -c core.pager=less log {{x}}",
        "echo ssh {{x}}",
        "w {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_ok(),
            "{command:?} should render"
        );
    }
}

#[test]
fn test_closed_literal_quotes_away_from_placeholders_render() {
    // Generated commands bake the base branch as a quoted literal next to placeholders; those
    // literals, and the same forms written by hand, must not trip the whitelist.
    for (command, expected) in [
        (
            "git worktree add -b {{x}} -- /wt 'a;touch pwned'",
            "git worktree add -b main -- /wt 'a;touch pwned'",
        ),
        (
            "git worktree add -b {{x}} -- /wt '$(id)'",
            "git worktree add -b main -- /wt '$(id)'",
        ),
        (
            "git worktree add -b {{x}} -- /wt \"it's\"",
            "git worktree add -b main -- /wt \"it's\"",
        ),
        (
            "git commit -m \"wip\" && git checkout {{x}}",
            "git commit -m \"wip\" && git checkout main",
        ),
        ("echo 'a;ssh;b' {{x}}", "echo 'a;ssh;b' main"),
        ("echo 'a # b' {{x}}", "echo 'a # b' main"),
    ] {
        assert_eq!(
            render_single_command(command, "main"),
            Ok(vec![expected.to_string()]),
            "{command:?}"
        );
    }

    // Quoted strings the shells read differently, or that hold a placeholder, are refused.
    for command in [
        "echo \"a$b\" {{x}}",
        "echo \"a!b\" {{x}}",
        "echo 'a\\b' {{x}}",
        "echo 'unclosed {{x}}",
        "echo 'a' \"{{x}} b\"",
        "echo 'a\nb' {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_err(),
            "{command:?} must be refused"
        );
    }
}

#[test]
fn test_quoted_command_names_are_seen_unquoted() {
    // Quoting a command name or flag does not change what runs, so the re-parsing check must
    // see through it (review of d0f3ea65e: all of these executed the value).
    for command in [
        "'ssh' host {{x}}",
        "\"ssh\" host {{x}}",
        "s'sh' host {{x}}",
        "'eval' {{x}}",
        "e''val {{x}}",
        "'bash' -c {{x}}",
        "bash '-c' {{x}}",
        "git -c 'alias.p=!sh -c x' p {{x}}",
    ] {
        let err = render_single_command(command, "$(touch pwned)")
            .expect_err(&format!("{command:?} must be refused"));
        assert!(err.contains("$(touch pwned)"), "{command:?}: {err}");
    }
    // A quoted literal containing separators is still one harmless word.
    assert_eq!(
        render_single_command("git worktree add -b {{x}} -- /wt 'a;ssh;b'", "main"),
        Ok(vec![
            "git worktree add -b main -- /wt 'a;ssh;b'".to_string()
        ])
    );
}

#[test]
fn test_placeholder_as_arithmetic_variable_or_command_input_is_refused() {
    for command in [
        // A variable as the command name.
        "$cmd {{x}}",
        "FOO=1 $cmd {{x}}",
        // PowerShell and cmd re-parsing.
        "powershell -Command {{x}}",
        "pwsh -c {{x}}",
        "pwsh -EncodedCommand {{x}}",
        "cmd /c {{x}}",
        "cmd.exe /C {{x}}",
        "Invoke-Command {{x}}",
        "Start-Process {{x}}",
        "git --% {{x}}",
        // Arithmetic operands and variable names.
        "let n={{x}}",
        "declare -i n={{x}}",
        "typeset -i n={{x}}",
        "local n={{x}}",
        "[[ {{x}} -eq 1 ]]",
        "[ -d {{x}} ]",
        "test {{x}} -eq 1",
        "test -v {{x}}",
        "read {{x}}",
        "printf -v {{x}} x",
    ] {
        assert!(
            render_single_command(command, "main").is_err(),
            "{command:?} must be refused"
        );
    }
    for command in [
        "printf %s {{x}}",
        "ls [abc]* {{x}}",
        "test -d /tmp && cd {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_ok(),
            "{command:?} should render"
        );
    }
}

/// Command-word bypasses found in review of 423dc85a7 (50 executions in real bash/zsh/fish
/// before the structural command-word check). Every one must be refused.
#[test]
fn test_command_word_bypasses_are_refused() {
    for (command, value) in [
        ("'ssh' host {{x}}", "touch pwned"),
        ("\"ssh\" host {{x}}", "touch pwned"),
        ("s'sh' host {{x}}", "touch pwned"),
        ("'eval' {{x}}", "touch pwned"),
        ("e''val {{x}}", "touch pwned"),
        ("'bash' -c {{x}}", "touch pwned"),
        ("bash '-c' {{x}}", "touch pwned"),
        ("sudo 'bash' -c {{x}}", "touch pwned"),
        ("find . -maxdepth 0 '-exec' sh -c {{x}} ';'", "touch pwned"),
        ("git -c 'alias.p=!sh -c \"$1\" -' p {{x}}", "touch pwned"),
        ("eval 'true' {{x}}", "touch pwned"),
        ("'ss'h host {{x}}", "touch pwned"),
        ("ss''h host {{x}}", "touch pwned"),
        ("\"s\"sh host {{x}}", "touch pwned"),
        ("e\"va\"l {{x}}", "touch pwned"),
        ("\\ssh host {{x}}", "touch pwned"),
        ("${c} {{x}}", "touch pwned"),
        ("$c {{x}}", "touch pwned"),
        ("echo $((1)) {{x}}", "touch pwned"),
        ("{s,}sh {{x}}", "touch pwned"),
        ("{eval,} {{x}}", "touch pwned"),
        ("{ssh,host} {{x}}", "touch pwned"),
        ("/bin/ba?h -c {{x}}", "touch pwned"),
        ("/bin/ba[s]h -c {{x}}", "touch pwned"),
        ("/bin/bas*h -c {{x}}", "touch pwned"),
        ("command eval {{x}}", "touch pwned"),
        ("exec ssh host {{x}}", "touch pwned"),
        ("builtin eval {{x}}", "touch pwned"),
        ("nohup sh -c {{x}}", "touch pwned"),
        ("time eval {{x}}", "touch pwned"),
        ("env -i ssh host {{x}}", "touch pwned"),
        ("noglob eval {{x}}", "touch pwned"),
        ("nocorrect eval {{x}}", "touch pwned"),
        ("- eval {{x}}", "touch pwned"),
        ("if eval {{x}}; then :; fi", "touch pwned"),
        ("! eval {{x}}", "touch pwned"),
        ("{ eval {{x}}; }", "touch pwned"),
        ("true; and eval {{x}}", "touch pwned"),
        ("not eval {{x}}", "touch pwned"),
        ("</dev/null eval {{x}}", "touch pwned"),
        (">/dev/null eval {{x}}", "touch pwned"),
        ("c=eval; command $c {{x}}", "touch pwned"),
        ("c=eval; builtin $c {{x}}", "touch pwned"),
        ("echo {{x}} | sh", "touch pwned"),
        ("printf %s {{x}} | bash", "touch pwned"),
        ("sh <(echo {{x}})", "touch pwned"),
        ("echo {{x}} > /dev/null | . /dev/stdin", "touch pwned"),
        ("flock /tmp/x.lock -c {{x}}", "touch pwned"),
        ("env -S {{x}}", "touch pwned"),
        ("timeout 5 bash -c {{x}}", "touch pwned"),
        ("dash -c {{x}}", "touch pwned"),
        ("busybox sh -c {{x}}", "touch pwned"),
        ("busybox ash -c {{x}}", "touch pwned"),
        ("setsid -w eval {{x}}", "touch pwned"),
        ("unset {{x}}", "a[$(touch pwned)]"),
        ("unset -v {{x}}", "a[$(touch pwned)]"),
        ("getopts a {{x}}", "a[$(touch pwned)]"),
        ("mapfile -t {{x}} </dev/null", "a[$(touch pwned)]"),
        ("integer n={{x}}", "a[$(touch pwned)]"),
        ("float n={{x}}", "a[$(touch pwned)]"),
        ("repeat {{x}} true", "a[$(touch pwned)]"),
        ("export n={{x}}; echo $((n))", "a[$(touch pwned)]"),
        ("shift {{x}}", "a[$(touch pwned)]"),
        ("printf %d {{x}}", "a[$(touch pwned)]"),
        ("test {{x}} -nt x", "a[$(touch pwned)]"),
        ("readonly n={{x}}", "a[$(touch pwned)]"),
        ("export -i n={{x}}", "a[$(touch pwned)]"),
        ("vared {{x}}", "a[$(touch pwned)]"),
        ("print -v {{x}} x", "a[$(touch pwned)]"),
        ("wait -p {{x}}", "a[$(touch pwned)]"),
        ("ulimit -t {{x}}", "a[$(touch pwned)]"),
        ("kill -l {{x}}", "a[$(touch pwned)]"),
        ("set -A {{x}} x", "a[$(touch pwned)]"),
    ] {
        assert!(
            render_single_command(command, value).is_err(),
            "{command:?} with {value:?} must be refused"
        );
    }
    // Shells are refused however they are invoked, not only with `-c`.
    for command in [
        "bash script.sh {{x}}",
        "echo {{x}} | sh",
        "powershell {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_err(),
            "{command:?}"
        );
    }
}

#[test]
fn test_ordinary_commands_with_placeholders_still_render() {
    for command in [
        "git worktree add -b {{x}} -- {{x}} 'origin/main'",
        "cd {{x}}",
        "npm run dev -- {{x}}",
        "cargo build -p {{x}}",
        "make {{x}}",
        "code {{x}}",
        "ls -la {{x}}",
        "test -d {{x}} && cd {{x}}",
        "sudo apt install {{x}}",
        "cd {{x}} && claude",
        "echo {{x}} >&2",
        "npm test 2>&1 {{x}}",
        "FOO=1 npm start {{x}}",
    ] {
        assert!(
            render_single_command(command, "feature/x").is_ok(),
            "{command:?} should render"
        );
    }
}

#[test]
fn test_round_six_command_word_bypasses_are_refused() {
    for command in [
        // A: shell-running precommand options in clusters, long forms and with a placeholder
        // attached.
        "env -S{{x}}",
        "env -iS {{x}}",
        "env --split-string={{x}}",
        "env --sp {{x}}",
        "sudo -Es {{x}}",
        "sudo --sh {{x}}",
        "sudo -i {{x}}",
        "flock /tmp/l -xc {{x}}",
        "flock /tmp/l -c{{x}}",
        "flock /tmp/l --comm {{x}}",
        // B: `script` with `c` in any short-option cluster, or --command.
        "script -qc {{x}} /dev/null",
        "script -q -c{{x}} /dev/null",
        "script -q --command {{x}} /dev/null",
        // C: `time` is a precommand with options.
        "time -p sh -c {{x}}",
        "time -p eval {{x}}",
        // D: `{`/`}` start a new segment.
        "function f { eval {{x}}; }; f",
        "if true { eval {{x}} }",
        "{ true } always { eval {{x}} }",
        "coproc N { eval {{x}}; }",
        // Versioned interpreter names and more evaluating builtins.
        "python3.14 -c {{x}}",
        "perl5.40.1 -e {{x}}",
        "node-18 {{x}}",
        "rbash -c {{x}}",
        "emulate sh -c {{x}}",
        "sched +0 {{x}}",
        "fc -e {{x}} -1",
        "alias q={{x}}",
        "bind -x {{x}}",
        "complete -C {{x}} foo",
    ] {
        assert!(
            render_single_command(command, "touch pwned").is_err(),
            "{command:?} must be refused"
        );
    }
}

#[test]
fn test_round_six_over_refusals_render() {
    for (command, expected) in [
        ("[ -d /tmp ] && cd {{x}}", "[ -d /tmp ] && cd main"),
        ("[[ -d /tmp ]] && cd {{x}}", "[[ -d /tmp ]] && cd main"),
        (
            "timeout -s KILL 300 cargo test {{x}}",
            "timeout -s KILL 300 cargo test main",
        ),
        ("nice -n 5 make {{x}}", "nice -n 5 make main"),
        ("sudo -u me ls {{x}}", "sudo -u me ls main"),
        (
            "sudo apt install python {{x}}",
            "sudo apt install python main",
        ),
        ("env FOO=1 npm start {{x}}", "env FOO=1 npm start main"),
        ("time make {{x}}", "time make main"),
        ("exec -a name ls {{x}}", "exec -a name ls main"),
        ("flock /tmp/l make {{x}}", "flock /tmp/l make main"),
        ("ls2 {{x}}", "ls2 main"),
    ] {
        assert_eq!(
            render_single_command(command, "main"),
            Ok(vec![expected.to_string()]),
            "{command:?}"
        );
    }
    // The word after a precommand's options is still checked, and `[`'s arithmetic arm works.
    for command in [
        "timeout -s KILL 300 sh -c {{x}}",
        "sudo -u me bash -c {{x}}",
        "nice -n 5 eval {{x}}",
        "[ 1 -eq 1 ] && cd {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_err(),
            "{command:?}"
        );
    }
}

/// #884: wrappers that run another command, and values a shell evaluates later.
#[test]
fn test_unmodelled_wrappers_and_later_evaluated_values_are_refused() {
    for command in [
        "strace -o /dev/null sh -c {{x}}",
        "dbus-run-session -- sh -c {{x}}",
        "/proc/self/exe -c {{x}}",
        "/proc/1234/exe -c {{x}}",
        "sshpass -p pw ssh host {{x}}",
        "xvfb-run sh -c {{x}}",
        "unshare -r sh -c {{x}}",
        "chroot /srv sh -c {{x}}",
        "systemd-run --user sh -c {{x}}",
        "docker run img sh -c {{x}}",
        "nix-shell --run {{x}}",
        "wsl bash -c {{x}}",
        "export PS1={{x}}",
        "export PROMPT_COMMAND={{x}}",
        "GIT_SSH_COMMAND={{x}} git ls-remote origin",
        "env GIT_SSH_COMMAND={{x}} git fetch",
        "export EDITOR={{x}}",
        "PAGER={{x}} git log",
        "BASH_ENV={{x}} true",
        "set -gx PS1 {{x}}",
        "command {{x}}",
        "env {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_err(),
            "{command:?} must be refused"
        );
    }
}

/// The same variables with literal values, and unrelated names, still render.
#[test]
fn test_evaluated_variable_check_does_not_over_refuse() {
    for (command, expected) in [
        ("EDITOR=vim git commit {{x}}", "EDITOR=vim git commit main"),
        ("MYPAGER={{x}} true", "MYPAGER=main true"),
        ("docker build -t {{x}} .", "docker build -t main ."),
    ] {
        assert_eq!(
            render_single_command(command, "main"),
            Ok(vec![expected.to_string()]),
            "{command:?}"
        );
    }
}

/// #884: `docker run -it {{image}}` is the dev-container use case and renders; a placeholder in
/// an option or after the image (the container's command) is refused.
#[test]
fn test_container_run_allows_only_an_image_placeholder() {
    for (command, expected) in [
        ("docker run -it {{image}}", "docker run -it main"),
        ("podman run --rm -it {{image}}", "podman run --rm -it main"),
        (
            "docker run -v /src:/src -e A=b {{image}}",
            "docker run -v /src:/src -e A=b main",
        ),
        (
            "docker run alpine && echo {{image}}",
            "docker run alpine && echo main",
        ),
        ("echo PS1 {{x}}", "echo PS1 main"),
        ("set -gx FOO {{x}}", "set -gx FOO main"),
    ] {
        assert_eq!(
            render_single_command(command, "main"),
            Ok(vec![expected.to_string()]),
            "{command:?}"
        );
    }
    for command in [
        "docker run {{x}} img",
        "docker run img {{x}}",
        "docker run -v {{x}}:/mnt img",
        "docker run --privileged{{x}} img",
        "docker run --mystery {{x}} img",
        "docker run -it img sh {{x}}",
        "nerdctl run --entrypoint {{x}} img",
        "kubectl run pod --image=img -- sh {{x}}",
        "set -gx PS1 {{x}}",
    ] {
        assert!(
            render_single_command(command, "main").is_err(),
            "{command:?} must be refused"
        );
    }
}
