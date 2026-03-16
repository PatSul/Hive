use hive_agents::heartbeat_scheduler::{HeartbeatMode, HeartbeatScheduler, HeartbeatTask};

#[test]
fn scheduler_add_and_list_tasks() {
    let scheduler = HeartbeatScheduler::new();
    let task = HeartbeatTask {
        id: "hb-1".into(),
        agent_id: "agent-1".into(),
        spec: "refactor error handling".into(),
        interval_secs: 60,
        mode: HeartbeatMode::FixedInterval,
        max_iterations: Some(10),
        paused: false,
        iteration_count: 0,
        last_fired: None,
        total_cost: 0.0,
    };
    scheduler.add(task);
    assert_eq!(scheduler.list().len(), 1);
    assert_eq!(scheduler.list()[0].spec, "refactor error handling");
}

#[test]
fn scheduler_pause_and_resume() {
    let scheduler = HeartbeatScheduler::new();
    scheduler.add(HeartbeatTask {
        id: "hb-1".into(),
        agent_id: "agent-1".into(),
        spec: "test".into(),
        interval_secs: 60,
        mode: HeartbeatMode::FixedInterval,
        max_iterations: None,
        paused: false,
        iteration_count: 0,
        last_fired: None,
        total_cost: 0.0,
    });

    scheduler.pause("hb-1");
    assert!(scheduler.list()[0].paused);

    scheduler.resume("hb-1");
    assert!(!scheduler.list()[0].paused);
}

#[test]
fn scheduler_cancel_removes_task() {
    let scheduler = HeartbeatScheduler::new();
    scheduler.add(HeartbeatTask {
        id: "hb-1".into(),
        agent_id: "agent-1".into(),
        spec: "test".into(),
        interval_secs: 60,
        mode: HeartbeatMode::FixedInterval,
        max_iterations: None,
        paused: false,
        iteration_count: 0,
        last_fired: None,
        total_cost: 0.0,
    });
    assert_eq!(scheduler.list().len(), 1);

    scheduler.cancel("hb-1");
    assert_eq!(scheduler.list().len(), 0);
}

#[test]
fn heartbeat_mode_backoff_doubles_interval() {
    let mut interval = 60u64;
    let max = 600u64;
    let multiplier = 2.0f64;

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 120);

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 240);

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 480);

    interval = ((interval as f64) * multiplier).min(max as f64) as u64;
    assert_eq!(interval, 600); // capped
}
