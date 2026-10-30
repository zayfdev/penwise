use std::path::PathBuf;

use super::*;

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("penwise-skill-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_skill_is_tight_and_names_what_triggers_it() {
    let lines = TEXT.lines().count();
    assert!(lines < 120, "{lines} lines");
    let mut parts = TEXT.splitn(3, "---");
    assert_eq!("", parts.next().unwrap());
    let front = parts.next().unwrap();
    assert!(front.contains("name: penwise"));
    let description = front
        .lines()
        .find(|line| line.starts_with("description:"))
        .unwrap();
    for trigger in ["design review", "check my marks", "Penwise"] {
        assert!(description.contains(trigger), "{trigger}");
    }
    for tool in ["`look`", "`show`", "`resolve`", "`notes`"] {
        assert!(TEXT.contains(tool), "{tool}");
    }
    assert!(TEXT.contains("round: n"), "the round number rule");
    assert!(TEXT.contains("Never edit code before the owner"));
}

#[test]
fn the_skill_lands_where_each_agent_reads_it_and_is_removed_with_its_folders() {
    let root = temp("paths");
    let claude = write(&root, Agent::Claude).unwrap();
    let omp = write(&root, Agent::Omp).unwrap();
    assert_eq!(root.join(".claude/skills/penwise/SKILL.md"), claude);
    assert_eq!(root.join(".omp/skills/penwise/SKILL.md"), omp);
    assert_eq!(TEXT, std::fs::read_to_string(&claude).unwrap());
    assert_eq!(Removed::Deleted, remove(&root, Agent::Claude).unwrap());
    assert!(!root.join(".claude").exists(), "empty folders go with it");
    assert_eq!(Removed::Absent, remove(&root, Agent::Claude).unwrap());
    assert_eq!(Removed::Deleted, remove(&root, Agent::Omp).unwrap());
    assert_eq!(0, std::fs::read_dir(&root).unwrap().count());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_edited_skill_and_its_neighbours_stay() {
    let root = temp("edited");
    let path = write(&root, Agent::Claude).unwrap();
    std::fs::write(root.join(".claude/settings.json"), "{}").unwrap();
    std::fs::write(&path, format!("{TEXT}\nmine\n")).unwrap();
    assert_eq!(Removed::Edited, remove(&root, Agent::Claude).unwrap());
    assert!(path.exists());
    std::fs::write(&path, TEXT).unwrap();
    assert_eq!(Removed::Deleted, remove(&root, Agent::Claude).unwrap());
    assert!(
        root.join(".claude/settings.json").exists(),
        "the folder stays: it holds other files"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
