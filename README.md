# tracing-rolling
helper crate to customize rolling log file with tracing crate

```rust
use std::time::Duration;

use time::macros::offset;
use tokio::time::sleep;
use tracing::info;
use tracing_rolling::{Checker, Daily};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_writer(
            // create a daily rolling file with custom date format
            // output file will be testing.20230323.log
            Daily::new("logs/testing.log", "[year][month][day]", offset!(+8))
                // Daily::new("logs/testing.log", None, offset!(+8))
                .buffered() // buffer file if needed
                .build()
                .unwrap(),
        )
        .init();
    let mut count = 0;
    info!("start");
    loop {
        count += 1;
        sleep(Duration::from_millis(50)).await;
        info!("{count}");
    }
}
```