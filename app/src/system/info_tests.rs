use byte_unit::Byte;

use crate::{terminal::model::test_utils::TestBlockBuilder, test_util::mock_blockgrid};

use super::*;

#[test]
fn test_memory_usage_stats_construction() {
    let total_application_usage_bytes = 1024;
    let mut stats = MemoryUsageStats::new(Byte::from_u64(total_application_usage_bytes));

    let now = Local::now();

    let mut block_with_content = TestBlockBuilder::new().build();
    block_with_content.set_prompt_and_command_grid(mock_blockgrid("line1\nline2"));
    block_with_content.set_output_grid(mock_blockgrid("line3"));
    block_with_content.update_last_painted_at(now);

    let inactive_5m_block = TestBlockBuilder::new().build();
    inactive_5m_block.update_last_painted_at(now - chrono::Duration::minutes(10));

    let inactive_1h_block1 = TestBlockBuilder::new().build();
    inactive_1h_block1.update_last_painted_at(now - chrono::Duration::minutes(70));

    let inactive_1h_block2 = TestBlockBuilder::new().build();
    inactive_1h_block2.update_last_painted_at(now - chrono::Duration::minutes(70));

    let blocks = [
        block_with_content,
        inactive_5m_block,
        inactive_1h_block1,
        inactive_1h_block2,
        TestBlockBuilder::new().build(),
    ];

    stats.add_blocks(now, blocks.iter());

    assert_eq!(
        stats.total_application_usage_bytes,
        total_application_usage_bytes as usize
    );
    assert_eq!(stats.total_blocks, 5);
    assert_eq!(stats.total_lines, 3);

    assert_eq!(stats.active_block_stats.num_blocks, 1);
    assert_eq!(stats.active_block_stats.num_lines, 3);

    assert_eq!(stats.inactive_5m_stats.num_blocks, 1);
    assert_eq!(stats.inactive_5m_stats.num_lines, 0);

    assert_eq!(stats.inactive_1h_stats.num_blocks, 2);
    assert_eq!(stats.inactive_1h_stats.num_lines, 0);

    assert_eq!(stats.inactive_24h_stats.num_blocks, 1);
    assert_eq!(stats.inactive_24h_stats.num_lines, 0);
}

#[test]
fn all_processes_refresh_kind_does_not_sample_cpu_or_memory() {
    let all = SystemInfo::all_processes_refresh_kind();
    assert!(!all.cpu(), "full-table sweep must not sample CPU");
    assert!(!all.memory(), "full-table sweep must not sample memory");

    // The single-PID self-poll still samples both.
    let own = SystemInfo::refresh_kind();
    assert!(own.cpu());
    assert!(own.memory());
}

/// No consumer, no timer: without `heap_usage_tracking` the 5 s self-rescheduling refresh
/// must not be started (it would wake the process forever for no subscriber).
#[test]
fn refresh_timer_only_scheduled_when_a_consumer_exists() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(SystemInfo::new);
        app.read(|ctx| {
            assert_eq!(
                handle.as_ref(ctx).refresh_scheduled(),
                cfg!(feature = "heap_usage_tracking")
            );
        });
    });
}
