//! The files and lines each scheduler reads: systemd units, a crontab line, a launchd
//! plist, plus the XML helpers Task Scheduler also uses. Pure text in and out, so it is
//! compiled and tested on every platform, while only the platform's own scheduler uses it.
// Each platform uses its own part of this module.
#![allow(dead_code)]

use jiff::civil::Time;

use super::{Installed, Job};

// ---- XML (Task Scheduler and launchd) ----

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The text of the first `<name>` element. Enough for the flat XML the schedulers write.
pub fn element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    Some(xml[start..end].trim())
}

// ---- systemd (Linux) ----

pub const SYSTEMD_SERVICE: &str = "databrus-oppdater.service";
pub const SYSTEMD_TIMER: &str = "databrus-oppdater.timer";

pub fn systemd_service(job: &Job) -> String {
    let command: Vec<String> = std::iter::once(job.program.to_string_lossy().into_owned())
        .chain(job.args.iter().cloned())
        .map(|arg| systemd_quote(&arg))
        .collect();
    format!(
        "[Unit]\n\
         Description=Henter energidrikkpriser for databrus\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={}\n",
        command.join(" ")
    )
}

/// `Persistent=true` runs a start that was missed while the machine was off.
pub fn systemd_timer(job: &Job) -> String {
    format!(
        "[Unit]\n\
         Description=Daglig henting for databrus\n\
         \n\
         [Timer]\n\
         OnCalendar=*-*-* {}\n\
         Persistent=true\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n",
        job.time.strftime("%H:%M:00")
    )
}

pub fn parse_systemd(service: &str, timer: &str) -> Installed {
    let value = |text: &str, key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key))
            .map(str::trim)
            .map(str::to_owned)
    };
    let time = value(timer, "OnCalendar=")
        .and_then(|v| v.split_whitespace().last().map(str::to_owned))
        .and_then(|t| t.parse::<Time>().ok());
    let command = value(service, "ExecStart=")
        .map(|c| c.replace("%%", "%").replace("$$", "$"))
        .unwrap_or_default();
    Installed { time, command }
}

/// Quotes an argument for `ExecStart=`: double quotes with C escapes where needed, and
/// `%` and `$` doubled so systemd does not expand them.
fn systemd_quote(arg: &str) -> String {
    let quoted =
        if arg.is_empty() || arg.contains(|c: char| c.is_whitespace() || "\"'\\".contains(c)) {
            format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
        } else {
            arg.to_owned()
        };
    quoted.replace('%', "%%").replace('$', "$$")
}

// ---- cron (Linux without systemd) ----

/// Marks our line in the crontab, so it can be found and replaced.
pub const CRON_MARKER: &str = "# databrus-oppdater";

/// `0 7 * * * /usr/bin/databrus oppdater --stille >> '/home/x/…/planlagt.log' 2>&1 # databrus-oppdater`
pub fn cron_line(job: &Job) -> String {
    let command: Vec<String> = std::iter::once(job.program.to_string_lossy().into_owned())
        .chain(job.args.iter().cloned())
        .map(|arg| shell_quote(&arg))
        .collect();
    let line = format!(
        "{} {} * * * {} >> {} 2>&1",
        job.time.minute(),
        job.time.hour(),
        command.join(" "),
        shell_quote(&job.log_file.to_string_lossy())
    );
    // cron turns an unescaped % into a newline.
    format!("{} {CRON_MARKER}", line.replace('%', "\\%"))
}

/// The crontab with our line replaced by `line` (or removed with `None`), and whether an
/// old line was there. Everything else is kept as it was.
pub fn crontab_with(existing: &str, line: Option<&str>) -> (String, bool) {
    let mut found = false;
    let mut lines: Vec<&str> = Vec::new();
    for l in existing.lines() {
        if l.trim_end().ends_with(CRON_MARKER) {
            found = true;
        } else {
            lines.push(l);
        }
    }
    lines.extend(line);
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    (text, found)
}

pub fn parse_cron(crontab: &str) -> Option<Installed> {
    let line = crontab
        .lines()
        .find(|l| l.trim_end().ends_with(CRON_MARKER))?;
    let line = line.trim_end().strip_suffix(CRON_MARKER)?.trim_end();
    let mut fields = line.splitn(6, ' ');
    let minute: i8 = fields.next()?.parse().ok()?;
    let hour: i8 = fields.next()?.parse().ok()?;
    let command = fields.nth(3).unwrap_or_default().replace("\\%", "%");
    Some(Installed {
        time: Time::new(hour, minute, 0, 0).ok(),
        command,
    })
}

/// POSIX shell quoting: plain words as they are, the rest in single quotes.
fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=@+-,".contains(c));
    if plain {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r#"'"'"'"#))
    }
}

// ---- launchd (macOS) ----

pub const LAUNCHD_LABEL: &str = "io.github.mathiashagen.databrus-oppdater";

/// A launch agent. launchd runs a start that was missed while the Mac slept as soon as it
/// wakes. stdout and stderr go to the log file, since an agent has no terminal.
pub fn launchd_plist(job: &Job) -> String {
    let arguments: String = std::iter::once(job.program.to_string_lossy().into_owned())
        .chain(job.args.iter().cloned())
        .map(|arg| format!("\n        <string>{}</string>", escape(&arg)))
        .collect();
    let log = escape(&job.log_file.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>{arguments}
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Hour</key>
        <integer>{hour}</integer>
        <key>Minute</key>
        <integer>{minute}</integer>
    </dict>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        hour = job.time.hour(),
        minute = job.time.minute(),
    )
}

pub fn parse_plist(plist: &str) -> Installed {
    let integer_after = |key: &str| {
        let rest = &plist[plist.find(&format!("<key>{key}</key>"))?..];
        element(rest, "integer")?.parse::<i8>().ok()
    };
    let time = match (integer_after("Hour"), integer_after("Minute")) {
        (Some(hour), Some(minute)) => Time::new(hour, minute, 0, 0).ok(),
        _ => None,
    };
    let command = plist
        .find("<key>ProgramArguments</key>")
        .and_then(|at| element(&plist[at..], "array"))
        .map(|array| {
            array
                .split("<string>")
                .skip(1)
                .filter_map(|s| s.split_once("</string>").map(|(arg, _)| unescape(arg)))
                .map(|arg| shell_quote(&arg))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    Installed { time, command }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jiff::civil::time;

    use super::*;

    fn job(program: &str, args: &[&str]) -> Job {
        Job {
            time: time(7, 5, 0, 0),
            program: PathBuf::from(program),
            args: args.iter().map(|a| a.to_string()).collect(),
            log_file: PathBuf::from("/home/kari/.local/share/databrus/planlagt.log"),
        }
    }

    #[test]
    fn systemd_units_round_trip() {
        let job = job(
            "/home/kari/.cargo/bin/databrus",
            &[
                "oppdater",
                "--stille",
                "--konfig",
                "/home/kari/Mine filer/100% konfig.toml",
            ],
        );
        let service = systemd_service(&job);
        let timer = systemd_timer(&job);
        assert!(service.contains(
            r#"ExecStart=/home/kari/.cargo/bin/databrus oppdater --stille --konfig "/home/kari/Mine filer/100%% konfig.toml""#
        ));
        assert!(service.contains("Type=oneshot"));
        assert!(timer.contains("OnCalendar=*-*-* 07:05:00"));
        assert!(timer.contains("Persistent=true"));
        assert!(timer.contains("WantedBy=timers.target"));

        let installed = parse_systemd(&service, &timer);
        assert_eq!(installed.time, Some(time(7, 5, 0, 0)));
        assert!(
            installed
                .command
                .ends_with(r#""/home/kari/Mine filer/100% konfig.toml""#)
        );
    }

    #[test]
    fn systemd_quoting() {
        assert_eq!(systemd_quote("oppdater"), "oppdater");
        assert_eq!(systemd_quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
        assert_eq!(systemd_quote("$HOME"), "$$HOME");
    }

    #[test]
    fn cron_line_quotes_and_escapes() {
        let job = job("/opt/data brus/databrus", &["oppdater", "--stille"]);
        assert_eq!(
            cron_line(&job),
            "5 7 * * * '/opt/data brus/databrus' oppdater --stille \
             >> /home/kari/.local/share/databrus/planlagt.log 2>&1 # databrus-oppdater"
        );
        let percent = job_with_log("/tmp/100%.log");
        assert!(cron_line(&percent).contains(r"'/tmp/100\%.log'"));
    }

    fn job_with_log(log: &str) -> Job {
        let mut j = job("/usr/bin/databrus", &["oppdater", "--stille"]);
        j.log_file = PathBuf::from(log);
        j
    }

    #[test]
    fn crontab_keeps_other_lines() {
        let existing = "MAILTO=kari\n30 6 * * 1 backup.sh\n0 7 * * * gammel # databrus-oppdater\n";
        let (text, found) = crontab_with(existing, Some("0 8 * * * ny # databrus-oppdater"));
        assert!(found);
        assert_eq!(
            text,
            "MAILTO=kari\n30 6 * * 1 backup.sh\n0 8 * * * ny # databrus-oppdater\n"
        );

        let (text, found) = crontab_with(&text, None);
        assert!(found);
        assert_eq!(text, "MAILTO=kari\n30 6 * * 1 backup.sh\n");

        assert_eq!(crontab_with("", None), (String::new(), false));
        assert_eq!(
            crontab_with("", Some("x # databrus-oppdater")),
            ("x # databrus-oppdater\n".to_owned(), false)
        );
    }

    #[test]
    fn cron_line_round_trips() {
        let job = job("/usr/bin/databrus", &["oppdater", "--stille"]);
        let installed = parse_cron(&format!("MAILTO=kari\n{}\n", cron_line(&job))).unwrap();
        assert_eq!(installed.time, Some(time(7, 5, 0, 0)));
        assert_eq!(
            installed.command,
            "/usr/bin/databrus oppdater --stille >> /home/kari/.local/share/databrus/planlagt.log 2>&1"
        );
        assert_eq!(parse_cron("30 6 * * 1 backup.sh\n"), None);
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(shell_quote("/usr/bin/databrus"), "/usr/bin/databrus");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("kari's"), r#"'kari'"'"'s'"#);
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn plist_round_trips() {
        let job = job(
            "/Users/kari/.cargo/bin/databrus",
            &[
                "oppdater",
                "--stille",
                "--konfig",
                "/Users/kari/R&D/konfig.toml",
            ],
        );
        let plist = launchd_plist(&job);
        assert!(plist.contains(&format!("<string>{LAUNCHD_LABEL}</string>")));
        assert!(plist.contains("<string>/Users/kari/R&amp;D/konfig.toml</string>"));
        assert!(plist.contains("<key>Hour</key>\n        <integer>7</integer>"));
        assert!(plist.contains("<key>StandardErrorPath</key>"));

        let installed = parse_plist(&plist);
        assert_eq!(installed.time, Some(time(7, 5, 0, 0)));
        assert_eq!(
            installed.command,
            "/Users/kari/.cargo/bin/databrus oppdater --stille --konfig '/Users/kari/R&D/konfig.toml'"
        );
    }
}
