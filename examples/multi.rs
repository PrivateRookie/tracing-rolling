use std::time::Duration;

use time::macros::offset;
use tokio::time::sleep;
use tracing::info;
use tracing_rolling::{Checker, Daily};
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[tokio::main]
async fn main() {
    let all = Daily::new("logs/all.log", "[year][month][day]", offset!(+8))
        .build()
        .unwrap()
        .with_filter(|e| !matches!(e.target(), "multi::a" | "multi::b"));
    let a = Daily::new("logs/a.log", "[year][month][day]", offset!(+8))
        .build()
        .unwrap()
        .with_filter(|e| e.target() == "multi::a");
    let b = Daily::new("logs/b.log", "[year][month][day]", offset!(+8))
        .build()
        .unwrap()
        .with_filter(|e| e.target() == "multi::b");

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_writer(all.and(a).and(b))
        .init();
    let mut count = 0;
    info!("start");
    loop {
        count += 1;
        sleep(Duration::from_millis(50)).await;
        info!("{count}");
        a::foo();
        b::bar();
    }
}

mod a {
    use tracing::info;

    pub fn foo() {
        info!("foo");
    }
}

mod b {
    use tracing::info;

    pub fn bar() {
        info!("bar");
    }
}
