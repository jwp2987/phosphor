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
  "git worktree add -b {{worktree_branch_name}} $HOME/.warp/worktrees/$(basename {{repo}})/{{worktree_branch_name}} {{branch}}",
  "cd $HOME/.warp/worktrees/$(basename {{repo}})/{{worktree_branch_name}}",
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
            "git worktree add -b {{worktree_branch_name}} $HOME/.warp/worktrees/$(basename {{repo}})/{{worktree_branch_name}} {{branch}}",
            "cd $HOME/.warp/worktrees/$(basename {{repo}})/{{worktree_branch_name}}"
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
        assert_eq!(
            commands[0].exec,
            "git worktree add -b my-feature $HOME/.warp/worktrees/$(basename /Users/me/repo)/my-feature main"
        );
        assert_eq!(
            commands[1].exec,
            "cd $HOME/.warp/worktrees/$(basename /Users/me/repo)/my-feature"
        );
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
  "git worktree add -b {{worktree_branch_name}} $HOME/.warp/worktrees/$(basename {{repo}})/{{worktree_branch_name}} {{branch}}",
  "cd $HOME/.warp/worktrees/$(basename {{repo}})/{{worktree_branch_name}}",
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
            assert_eq!(
                commands[0].exec,
                "git worktree add -b my-feature $HOME/.warp/worktrees/$(basename /Users/me/repo)/my-feature main"
            );
            assert_eq!(
                commands[1].exec,
                "cd $HOME/.warp/worktrees/$(basename /Users/me/repo)/my-feature"
            );
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
fn test_placeholder_inside_template_quotes_is_refused() {
    for command in [
        "cd \"/tmp/{{x}}\"",
        "cd '/tmp/{{x}}'",
        "echo \"a {{x}} b\"",
        "echo '{{x}} b'",
        // Quoting the shells read differently: `\'` is an escape in bash, not in PowerShell.
        "echo \\'{{x}}",
        "echo `\"{{x}}",
        "echo \u{2018}{{x}}\u{2019}",
        // Directly after an escape or `$` the word's opening quote changes meaning.
        "echo \\{{x}}",
        "echo ${{x}}",
        "echo `{{x}}",
    ] {
        let err = render_single_command(command, "$(touch pwned)")
            .expect_err(&format!("{command} must be refused"));
        assert!(err.contains("$(touch pwned)"), "{command}: {err}");
        // The infallible render falls back to an empty terminal.
        let config: TabConfig = toml::from_str(&build_test_tab_config_toml(
            "Quoting",
            vec![command.to_string()],
        ))
        .unwrap();
        let params = HashMap::from([("x".to_string(), "$(touch pwned)".to_string())]);
        assert!(
            render_commands(&config, &params, None).is_empty(),
            "{command}"
        );
    }
}

#[test]
fn test_template_quotes_away_from_placeholders_are_unaffected() {
    assert_eq!(
        render_single_command(
            "git checkout {{x}} && echo \"switched\" && grep 'a b' file",
            "$(touch pwned)"
        ),
        Ok(vec![
            "git checkout '$(touch pwned)' && echo \"switched\" && grep 'a b' file".to_string()
        ])
    );
    assert_eq!(
        render_single_command("echo \"done\" {{x}} 'x'", "main"),
        Ok(vec!["echo \"done\" main 'x'".to_string()])
    );
    // A placeholder with no value is left alone, wherever it is.
    assert_eq!(
        render_single_command("echo \"{{missing}}\" {{x}}", "main"),
        Ok(vec!["echo \"{{missing}}\" main".to_string()])
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
