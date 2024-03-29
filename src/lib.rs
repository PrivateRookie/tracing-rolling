use std::{
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::{Mutex, MutexGuard};
use time::{
    format_description::{parse_owned, Component, OwnedFormatItem},
    Date, Duration, OffsetDateTime, Time, UtcOffset,
};
use tracing_subscriber::fmt::MakeWriter;

pub trait Checker: Sized {
    type W: Write;
    fn should_update(&self) -> bool;
    fn new_writer(&self) -> io::Result<Self::W>;
    /// create a buffered writer with default size: 4K
    fn buffered(self) -> Buffered<Self, Self::W> {
        Buffered {
            checker: self,
            size: 4096,
        }
    }
    /// create a buffered writer with specified buffer size
    fn buffer_with(self, size: usize) -> Buffered<Self, Self::W> {
        Buffered {
            checker: self,
            size,
        }
    }

    fn build(self) -> io::Result<(Rolling<Self, Self::W>, Token<Self::W>)> {
        let fd = Arc::new(Mutex::new(self.new_writer()?));
        let t = Token(fd.clone());
        let r = Rolling::new(self, fd);
        Ok((r, t))
    }
}

#[must_use = "must manual drop to ensure flush file when process exits"]
pub struct Token<W: Write>(Arc<Mutex<W>>);

impl<W: Write> Drop for Token<W> {
    fn drop(&mut self) {
        if let Err(e) = self.0.lock().flush() {
            eprintln!("drop writer {e}");
        }
    }
}

pub struct Rolling<C: Checker<W = W>, W: Write> {
    writer: Arc<Mutex<W>>,
    checker: C,
}

impl<C: Checker<W = W>, W: Write> Rolling<C, W> {
    pub fn new(checker: C, writer: Arc<Mutex<W>>) -> Self {
        Self { writer, checker }
    }

    fn update_writer(&self) -> io::Result<()> {
        {
            let mut writer = self.writer.lock();
            writer.flush()?;
        }
        let writer = self.checker.new_writer()?;
        *self.writer.lock() = writer;
        Ok(())
    }
}

impl<'a, C: Checker<W = W>, W: Write + 'a> MakeWriter<'a> for Rolling<C, W> {
    type Writer = RollingWriter<'a, W>;

    fn make_writer(&'a self) -> Self::Writer {
        if self.checker.should_update() {
            if let Err(e) = self.update_writer() {
                eprintln!("can not update log file {e}")
            }
        }
        RollingWriter(self.writer.lock())
    }
}

pub struct RollingWriter<'a, W: Write>(MutexGuard<'a, W>);

impl<'a, W: Write> Write for RollingWriter<'a, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

pub trait Period {
    fn previous_dt(&self) -> Result<OffsetDateTime, String>;
    fn now(&self) -> OffsetDateTime;
    fn new_path(&self) -> String;
    fn duration(&self) -> &Duration;
}

impl<P: Period> Checker for P {
    type W = File;
    fn should_update(&self) -> bool {
        let file_dt = match self.previous_dt() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("parse previous file failed: {e}");
                return false;
            }
        };
        self.now() - file_dt >= *self.duration()
    }

    fn new_writer(&self) -> io::Result<File> {
        let path = self.new_path();
        let file = File::options().append(true).create(true).open(path)?;
        Ok(file)
    }
}

pub struct Minute {
    offset: UtcOffset,
    fmt: OwnedFormatItem,
    active: Mutex<String>,
}

impl Minute {
    pub const DURATION: Duration = Duration::MINUTE;

    pub fn new(path: impl AsRef<Path>, offset: impl Into<Option<UtcOffset>>) -> Self {
        let ext = path
            .as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let fmt = path
            .as_ref()
            .with_extension(format!("[year]-[month]-[day]-[hour]-[minute].{ext}"));
        let fmt = parse_owned::<1>(&format!("{}", fmt.display())).unwrap();
        Self {
            offset: offset.into().unwrap_or(UtcOffset::UTC),
            fmt,
            active: Default::default(),
        }
    }
}

impl Period for Minute {
    fn previous_dt(&self) -> Result<OffsetDateTime, String> {
        let file = self.active.lock();
        let date = Date::parse(&file, &self.fmt).map_err(|e| e.to_string())?;
        let time = Time::parse(&file, &self.fmt).map_err(|e| e.to_string())?;
        Ok(date.with_time(time).assume_offset(self.offset))
    }

    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc().to_offset(self.offset)
    }

    fn new_path(&self) -> String {
        let now = self.now();
        let file = now.format(&self.fmt).unwrap();
        *self.active.lock() = file.clone();
        file
    }

    fn duration(&self) -> &Duration {
        &Self::DURATION
    }
}

pub struct Hourly {
    offset: UtcOffset,
    fmt: OwnedFormatItem,
    hour_regex: regex::Regex,
    active: Mutex<String>,
}

impl Hourly {
    pub const DURATION: Duration = Duration::HOUR;

    pub fn new(path: impl AsRef<Path>, offset: impl Into<Option<UtcOffset>>) -> Self {
        let ext = path
            .as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let fmt = path
            .as_ref()
            .with_extension(format!("[year]-[month]-[day]-[hour].{ext}"));
        let hour_regex =
            regex::Regex::new(&format!(r".*\d{{4}}-\d{{2}}-\d{{2}}-(\d{{2}})\.{ext}")).unwrap();
        let fmt = parse_owned::<1>(&format!("{}", fmt.display())).unwrap();
        Self {
            offset: offset.into().unwrap_or(UtcOffset::UTC),
            fmt,
            active: Default::default(),
            hour_regex,
        }
    }
}

impl Period for Hourly {
    fn previous_dt(&self) -> Result<OffsetDateTime, String> {
        let file = self.active.lock();
        let date = Date::parse(&file, &self.fmt).map_err(|e| e.to_string())?;
        let hour = self
            .hour_regex
            .captures(&file)
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse::<u8>().ok())
            .ok_or_else(|| format!("invalid hour component of {file}"))?;
        let time = Time::from_hms(hour, 0, 0).unwrap();
        Ok(date.with_time(time).assume_offset(self.offset))
    }

    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc().to_offset(self.offset)
    }

    fn new_path(&self) -> String {
        let now = self.now();
        let file = now.format(&self.fmt).unwrap();
        *self.active.lock() = file.clone();
        file
    }

    fn duration(&self) -> &Duration {
        &Self::DURATION
    }
}

pub struct Daily {
    offset: UtcOffset,
    fmt: OwnedFormatItem,
    active: Mutex<String>,
}

impl Daily {
    pub const DURATION: Duration = Duration::DAY;

    fn ensure_year_month_day(fmt: &OwnedFormatItem) {
        match fmt {
            OwnedFormatItem::Compound(items) => {
                let mut year = false;
                let mut month = false;
                let mut day = false;
                for item in &items[..] {
                    match item {
                        OwnedFormatItem::Component(Component::Year(_)) => {
                            year = !year;
                        }
                        OwnedFormatItem::Component(Component::Month(_)) => {
                            month = !month;
                        }
                        OwnedFormatItem::Component(Component::Day(_)) => {
                            day = !day;
                        }
                        _ => {}
                    }
                }
                if !(year && month && day) {
                    panic!("invalid daily format");
                }
            }
            _ => panic!("expect compound format"),
        }
    }

    /// **NOTE: if fmt is specified, it should be valid time format_description and contain
    /// year, month, day**
    ///
    /// default fmt is `[year]-[month]-[day]`
    pub fn new<S>(
        path: impl AsRef<Path>,
        fmt: impl Into<Option<S>>,
        offset: impl Into<Option<UtcOffset>>,
    ) -> Self
    where
        S: std::fmt::Display,
    {
        let ext = path
            .as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();

        let file_stem = path
            .as_ref()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();

        let file_name = fmt
            .into()
            .map(|f| format!("{file_stem}-{f}.{ext}"))
            .unwrap_or_else(|| format!("{file_stem}-[year]-[month]-[day].{ext}"));
        let fmt = parse_owned::<1>(&format!(
            "{}",
            path.as_ref().with_file_name(file_name).display()
        ))
        .unwrap();
        Self::ensure_year_month_day(&fmt);
        Self {
            offset: offset.into().unwrap_or(UtcOffset::UTC),
            fmt,
            active: Default::default(),
        }
    }
}

impl Period for Daily {
    fn previous_dt(&self) -> Result<OffsetDateTime, String> {
        let file = self.active.lock();
        let date = Date::parse(&file, &self.fmt).map_err(|e| e.to_string())?;
        Ok(date
            .with_time(time::macros::time!(0:0:0))
            .assume_offset(self.offset))
    }

    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc().to_offset(self.offset)
    }

    fn new_path(&self) -> String {
        let now = self.now();
        let file = now.format(&self.fmt).unwrap();
        *self.active.lock() = file.clone();
        file
    }

    fn duration(&self) -> &Duration {
        &Self::DURATION
    }
}

pub struct Buffered<C: Checker<W = W>, W: Write> {
    checker: C,
    size: usize,
}

impl<C: Checker<W = W>, W: Write> Buffered<C, W> {
    pub fn new(checker: C, size: usize) -> Self {
        Self { checker, size }
    }
}

impl<C: Checker<W = W>, W: Write> Checker for Buffered<C, W> {
    type W = BufWriter<W>;
    fn should_update(&self) -> bool {
        self.checker.should_update()
    }
    fn new_writer(&self) -> io::Result<BufWriter<W>> {
        Ok(BufWriter::with_capacity(
            self.size,
            self.checker.new_writer()?,
        ))
    }
}

/// construct a non rolling file
pub struct ConstFile(PathBuf);

impl Checker for ConstFile {
    type W = File;

    fn should_update(&self) -> bool {
        false
    }

    fn new_writer(&self) -> io::Result<Self::W> {
        File::options().append(true).create(true).open(&self.0)
    }
}

impl ConstFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_path_buf())
    }
}
