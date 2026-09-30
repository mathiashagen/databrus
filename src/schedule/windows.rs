//! Windows Task Scheduler via `schtasks.exe`.
//!
//! The task is registered from an XML definition rather than with `/sc daily`, since only
//! the XML can say "run as soon as possible after a missed start" and "also run on
//! battery". It runs as the current user, only while they are logged on, so no password
//! is stored.

use std::process::{Command, Output};

use jiff::Zoned;
use jiff::civil::{DateTime, Time};

use super::{Installed, Job, Scheduler, TASK_NAME};
use crate::error::AppError;

pub struct TaskScheduler;

impl Scheduler for TaskScheduler {
    fn install(&self, job: &Job) -> Result<(), AppError> {
        let now = Zoned::now().datetime();
        let xml = task_xml(job, start_boundary(job.time, now));
        // schtasks reads the definition from a file, which must be UTF-16 to match the
        // XML declaration.
        let file = std::env::temp_dir().join(format!("{TASK_NAME}-{}.xml", std::process::id()));
        std::fs::write(&file, utf16_with_bom(&xml))?;
        let output = schtasks(&[
            "/create".as_ref(),
            "/tn".as_ref(),
            TASK_NAME.as_ref(),
            "/xml".as_ref(),
            file.as_os_str(),
            "/f".as_ref(),
        ]);
        let _ = std::fs::remove_file(&file);
        check(&output?, "kunne ikke installere oppgaven")
    }

    fn remove(&self) -> Result<bool, AppError> {
        if self.status()?.is_none() {
            return Ok(false);
        }
        let output = schtasks(&["/delete", "/tn", TASK_NAME, "/f"].map(AsRef::as_ref))?;
        check(&output, "kunne ikke fjerne oppgaven")?;
        Ok(true)
    }

    fn status(&self) -> Result<Option<Installed>, AppError> {
        let output = schtasks(&["/query", "/tn", TASK_NAME, "/xml"].map(AsRef::as_ref))?;
        // schtasks has no exit code of its own for "no such task", and its messages are
        // localized, so any failed query counts as "not installed".
        if !output.status.success() {
            return Ok(None);
        }
        Ok(Some(parse_installed(&decode(&output.stdout))))
    }
}

fn schtasks(args: &[&std::ffi::OsStr]) -> Result<Output, AppError> {
    Command::new("schtasks.exe")
        .args(args)
        .output()
        .map_err(|e| AppError::Schedule(format!("kunne ikke starte schtasks.exe: {e}")))
}

fn check(output: &Output, what: &str) -> Result<(), AppError> {
    if output.status.success() {
        return Ok(());
    }
    let message = decode(&output.stderr);
    let message = message.trim();
    Err(AppError::Schedule(if message.is_empty() {
        format!("{what} ({})", output.status)
    } else {
        format!("{what}: {message}")
    }))
}

/// The first start: today at `time` if that is still ahead, otherwise tomorrow. A start in
/// the past would count as missed and run the task right after installing.
fn start_boundary(time: Time, now: DateTime) -> DateTime {
    let today = now.date().to_datetime(time);
    if today > now {
        today
    } else {
        today
            .date()
            .tomorrow()
            .map_or(today, |d| d.to_datetime(time))
    }
}

fn task_xml(job: &Job, start: DateTime) -> String {
    let command = escape(&job.program.to_string_lossy());
    let arguments = escape(
        &job.args
            .iter()
            .map(|a| quote_arg(a))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let start = start.strftime("%Y-%m-%dT%H:%M:%S");
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Henter ferske energidrikkpriser for databrus én gang om dagen.</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>{start}</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <ExecutionTimeLimit>PT1H</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{arguments}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn parse_installed(xml: &str) -> Installed {
    let time = element(xml, "StartBoundary")
        .and_then(|s| s.split_once('T'))
        .and_then(|(_, t)| t.get(..8))
        .and_then(|t| t.parse::<Time>().ok());
    let program = element(xml, "Command").map(unescape).unwrap_or_default();
    let command = match element(xml, "Arguments").map(unescape) {
        Some(args) if !args.is_empty() => format!("{} {args}", quote_arg(&program)),
        _ => quote_arg(&program),
    };
    Installed { time, command }
}

/// The text of the first `<name>` element. Enough for the flat XML schtasks prints.
fn element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    Some(xml[start..end].trim())
}

/// Quotes an argument the way the MSVC runtime splits command lines.
fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn utf16_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

/// schtasks prints in the console code page even though its XML claims to be UTF-16, but
/// it can print real UTF-16 too. Non-ASCII text in the code page comes out lossy.
fn decode(bytes: &[u8]) -> String {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.get(1) == Some(&0);
    if utf16 {
        let body = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jiff::civil::{date, time};

    use super::*;

    fn job(program: &str, args: &[&str]) -> Job {
        Job {
            time: time(7, 0, 0, 0),
            program: PathBuf::from(program),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn starts_today_when_the_time_is_ahead() {
        let now = date(2026, 9, 30).at(6, 59, 0, 0);
        assert_eq!(
            start_boundary(time(7, 0, 0, 0), now),
            date(2026, 9, 30).at(7, 0, 0, 0)
        );
    }

    #[test]
    fn starts_tomorrow_when_the_time_has_passed() {
        let now = date(2026, 9, 30).at(7, 0, 0, 0);
        assert_eq!(
            start_boundary(time(7, 0, 0, 0), now),
            date(2026, 10, 1).at(7, 0, 0, 0)
        );
    }

    #[test]
    fn xml_round_trips_time_and_command() {
        let job = job(
            r"C:\Program Files\R&D\databrus.exe",
            &[
                "oppdater",
                "--stille",
                "--konfig",
                r"C:\Mine filer\konfig.toml",
            ],
        );
        let xml = task_xml(&job, date(2026, 10, 1).at(7, 30, 0, 0));
        assert!(xml.contains("<StartBoundary>2026-10-01T07:30:00</StartBoundary>"));
        assert!(xml.contains(r"<Command>C:\Program Files\R&amp;D\databrus.exe</Command>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));

        let installed = parse_installed(&xml);
        assert_eq!(installed.time, Some(time(7, 30, 0, 0)));
        assert_eq!(
            installed.command,
            r#""C:\Program Files\R&D\databrus.exe" oppdater --stille --konfig "C:\Mine filer\konfig.toml""#
        );
    }

    #[test]
    fn parses_what_schtasks_prints() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\r\n<Task version=\"1.2\">\r\r\n\
            <Triggers><CalendarTrigger><StartBoundary>2026-10-01T06:15:00+02:00</StartBoundary>\
            </CalendarTrigger></Triggers><Actions Context=\"Author\"><Exec>\
            <Command>C:\\databrus\\databrus.exe</Command></Exec></Actions></Task>";
        let installed = parse_installed(xml);
        assert_eq!(installed.time, Some(time(6, 15, 0, 0)));
        assert_eq!(installed.command, r"C:\databrus\databrus.exe");
    }

    #[test]
    fn quoting_follows_the_msvc_rules() {
        assert_eq!(quote_arg("oppdater"), "oppdater");
        assert_eq!(quote_arg(""), r#""""#);
        assert_eq!(quote_arg(r"C:\a b\"), r#""C:\a b\\""#);
        assert_eq!(quote_arg(r#"si "hei""#), r#""si \"hei\"""#);
    }

    #[test]
    fn decodes_both_code_page_and_utf16_output() {
        assert_eq!(decode(b"<Task>"), "<Task>");
        assert_eq!(decode(&utf16_with_bom("<Task>")), "<Task>");
        let without_bom: Vec<u8> = "ok".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&without_bom), "ok");
    }
}
