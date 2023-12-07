use std::time::Duration;

use time::macros::offset;
use tokio::time::sleep;
use tracing::info;
use tracing_rolling::{Checker, Daily};

#[tokio::main]
async fn main() {
    // create a daily rolling file with custom date format
    // output file pattern is testing.20230323.log
    let (writer, token) = Daily::new("logs/testing.log", "[year][month][day]", offset!(+8))
        // Daily::new("logs/testing.log", None, offset!(+8))
        .buffered() // buffer file if needed
        .build()
        .unwrap();

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_writer(writer)
        .init();
    let mut count = 0;
    info!("start");
    while count < 100 {
        count += 1;
        sleep(Duration::from_millis(50)).await;
        info!("{count}");
    }
    drop(token);
}
