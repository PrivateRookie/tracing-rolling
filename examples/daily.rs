use std::time::Duration;

use time::UtcOffset;
use tokio::time::sleep;
use tracing::info;
use tracing_rolling::{Buffered, Daily, Rolling};

#[tokio::main]
async fn main() {
    let daily = Daily::new(
        "logs/testing.log",
        UtcOffset::current_local_offset().unwrap(),
    );
    let buffered = Buffered::new(daily, 1024);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_writer(Rolling::new(buffered).unwrap())
        .init();
    let mut count = 0;
    info!("start");
    loop {
        count += 1;
        sleep(Duration::from_millis(50)).await;
        info!("{count}");
    }
}
