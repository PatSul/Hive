use hive_remote::protocol::DaemonEvent;
use hive_remote::session::SessionJournal;
use tempfile::tempdir;

#[tokio::test]
async fn test_journal_append_and_replay() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();

    journal
        .append(&DaemonEvent::SendMessage {
            conversation_id: "c1".into(),
            content: "Hello".into(),
            model: "test".into(),
        })
        .unwrap();

    journal
        .append(&DaemonEvent::SwitchPanel {
            panel: "agents".into(),
        })
        .unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_journal_replay_empty_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty.jsonl");
    std::fs::write(&path, "").unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    assert_eq!(events.len(), 0);
}

#[tokio::test]
async fn test_journal_replay_corrupt_line_skipped() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("partial.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();
    journal.append(&DaemonEvent::Ping).unwrap();

    // Append a corrupt line directly to the file
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{{broken json").unwrap();

    journal.append(&DaemonEvent::Pong).unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    // Should recover 2 valid events, skip the corrupt one
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_journal_truncate() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();
    journal.append(&DaemonEvent::Ping).unwrap();
    journal.append(&DaemonEvent::Pong).unwrap();

    journal.truncate().unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    assert_eq!(events.len(), 0);
}

#[tokio::test]
async fn test_journal_nonexistent_path_creates_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sub/dir/journal.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();
    journal.append(&DaemonEvent::Ping).unwrap();

    assert!(path.exists());
}
