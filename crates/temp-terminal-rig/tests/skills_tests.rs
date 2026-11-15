use std::fs;
use temp_terminal_rig::{InstalledSkillResult, RigRole, SkillsDeployer};
use tempfile::tempdir;

fn is_emoji_char(c: char) -> bool {
    matches!(
        c as u32,
        0x1F600..=0x1F64F
            | 0x1F300..=0x1F5FF
            | 0x1F680..=0x1F6FF
            | 0x1F700..=0x1F77F
            | 0x1F780..=0x1F7FF
            | 0x1F800..=0x1F8FF
            | 0x1F900..=0x1F9FF
            | 0x1FA00..=0x1FA6F
            | 0x1FA70..=0x1FAFF
            | 0x2600..=0x26FF
            | 0x2700..=0x27BF
            | 0xFE00..=0xFE0F
            | 0x1F1E6..=0x1F1FF
    )
}

#[test]
fn test_rig_role_from_str_and_as_str() {
    assert_eq!(RigRole::from_str("manager"), Some(RigRole::Manager));
    assert_eq!(RigRole::from_str("  MANAGER \n"), Some(RigRole::Manager));
    assert_eq!(RigRole::from_str("worker"), Some(RigRole::Worker));
    assert_eq!(RigRole::from_str("Worker"), Some(RigRole::Worker));
    assert_eq!(RigRole::from_str("reviewer"), Some(RigRole::Reviewer));
    assert_eq!(RigRole::from_str("Reviewer"), Some(RigRole::Reviewer));
    assert_eq!(
        RigRole::from_str("orchestrator"),
        Some(RigRole::Orchestrator)
    );
    assert_eq!(
        RigRole::from_str("ORCHESTRATOR"),
        Some(RigRole::Orchestrator)
    );

    assert_eq!(RigRole::from_str("unknown"), None);
    assert_eq!(RigRole::from_str(""), None);

    assert_eq!(RigRole::Manager.as_str(), "manager");
    assert_eq!(RigRole::Worker.as_str(), "worker");
    assert_eq!(RigRole::Reviewer.as_str(), "reviewer");
    assert_eq!(RigRole::Orchestrator.as_str(), "orchestrator");
}

#[test]
fn test_rig_role_serde_roundtrip() {
    let roles = [
        RigRole::Manager,
        RigRole::Worker,
        RigRole::Reviewer,
        RigRole::Orchestrator,
    ];

    for role in roles {
        let json = serde_json::to_string(&role).expect("serialize role");
        let deserialized: RigRole = serde_json::from_str(&json).expect("deserialize role");
        assert_eq!(role, deserialized);
    }

    let parsed: RigRole = serde_json::from_str("\"worker\"").expect("deserialize from string");
    assert_eq!(parsed, RigRole::Worker);
}

#[test]
fn test_rig_role_prompt_templates_anti_slop() {
    let roles = [
        RigRole::Manager,
        RigRole::Worker,
        RigRole::Reviewer,
        RigRole::Orchestrator,
    ];

    for role in roles {
        let template = role.prompt_template();
        assert!(!template.trim().is_empty(), "Prompt template for {:?} should not be empty", role);
        assert!(
            template.to_lowercase().contains(role.as_str()),
            "Prompt template for {:?} must contain its role name",
            role
        );

        assert!(
            !template.contains('\u{2014}'),
            "Prompt template for {:?} contains em dash character",
            role
        );

        for (idx, ch) in template.chars().enumerate() {
            assert!(
                !is_emoji_char(ch),
                "Prompt template for {:?} contains emoji {:?} at index {}",
                role,
                ch,
                idx
            );
        }
    }
}

#[test]
fn test_skills_deployer_get_skill_dirs() {
    let temp = tempdir().expect("create temp dir");
    let home = temp.path();

    let dirs = SkillsDeployer::get_skill_dirs(home);
    let harness_names: Vec<&str> = dirs.iter().map(|(h, _)| *h).collect();

    assert_eq!(
        harness_names,
        vec!["claude", "codex", "opencode", "pi", "omp", "commandcode"]
    );

    let map: std::collections::HashMap<&str, std::path::PathBuf> = dirs.into_iter().collect();
    assert_eq!(map.get("claude").unwrap(), &home.join(".claude").join("skills"));
    assert_eq!(map.get("codex").unwrap(), &home.join(".codex").join("skills"));
    assert_eq!(
        map.get("opencode").unwrap(),
        &home.join(".config").join("opencode").join("skills")
    );
    assert_eq!(
        map.get("pi").unwrap(),
        &home.join(".pi").join("agent").join("skills")
    );
    assert_eq!(
        map.get("omp").unwrap(),
        &home.join(".omp").join("agent").join("skills")
    );
    assert_eq!(
        map.get("commandcode").unwrap(),
        &home.join(".commandcode").join("skills")
    );
}

#[test]
fn test_skills_deployer_install_all_discovered_harnesses() {
    let temp = tempdir().expect("create temp dir");
    let home = temp.path();

    // Create mock parent directories for claude and omp
    fs::create_dir_all(home.join(".claude")).expect("create .claude");
    fs::create_dir_all(home.join(".omp")).expect("create .omp");

    let results: Vec<InstalledSkillResult> =
        SkillsDeployer::install_skills(home, None).expect("install skills");

    // 2 discovered harnesses * 4 roles = 8 installed skills
    assert_eq!(results.len(), 8);

    for res in &results {
        assert!(res.skill_dir.exists(), "Skill dir should exist: {:?}", res.skill_dir);
        let skill_file = res.skill_dir.join("SKILL.md");
        assert!(skill_file.exists(), "SKILL.md should exist: {:?}", skill_file);

        let content = fs::read_to_string(&skill_file).expect("read SKILL.md");
        assert!(content.contains(&format!("name: rig-{}", res.role.as_str())));
        assert!(content.contains(&format!("description: Run this session as the rig {}.", res.role.as_str())));
        assert!(content.contains(&format!("# rig {}", res.role.as_str())));
        assert!(content.contains(&format!("Run `rig skill {}` now and follow what it prints.", res.role.as_str())));

        assert!(!content.contains('\u{2014}'), "SKILL.md contains em dash");
        for ch in content.chars() {
            assert!(!is_emoji_char(ch), "SKILL.md contains emoji {:?}", ch);
        }
    }
}

#[test]
fn test_skills_deployer_target_harnesses_filtering() {
    let temp = tempdir().expect("create temp dir");
    let home = temp.path();

    // Create .claude and .codex
    fs::create_dir_all(home.join(".claude")).expect("create .claude");
    fs::create_dir_all(home.join(".codex")).expect("create .codex");

    // Filter only codex
    let results = SkillsDeployer::install_skills(home, Some(&["codex"])).expect("install codex only");
    assert_eq!(results.len(), 4);
    for res in &results {
        assert_eq!(res.harness, "codex");
        assert!(res.skill_dir.exists());
    }

    // claude should not have skills created
    assert!(!home.join(".claude").join("skills").exists());

    // Explicitly target a harness whose parent does not exist yet (commandcode)
    let explicit_results = SkillsDeployer::install_skills(home, Some(&["commandcode"]))
        .expect("install explicit harness");
    assert_eq!(explicit_results.len(), 4);
    for res in &explicit_results {
        assert_eq!(res.harness, "commandcode");
        assert!(res.skill_dir.exists());
    }
}
