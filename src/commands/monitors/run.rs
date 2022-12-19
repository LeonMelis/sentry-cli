use std::process;
use std::time::Instant;

use anyhow::Result;
use clap::{Arg, ArgMatches, Command};
use console::style;
use uuid::Uuid;

use crate::api::{Api, CreateMonitorCheckIn, MonitorStatus, UpdateMonitorCheckIn};
use crate::utils::args::validate_uuid;
use crate::utils::system::{print_error, QuietExit};

pub fn make_command(command: Command) -> Command {
    command
        .about("Wraps a command")
        .arg(
            Arg::new("monitor")
                .help("The monitor ID")
                .required(true)
                .validator(validate_uuid),
        )
        .arg(
            Arg::new("allow_failure")
                .short('f')
                .long("allow-failure")
                .help("Run provided command even when Sentry reports an error."),
        )
        .arg(
            Arg::new("args")
                .value_name("ARGS")
                .required(true)
                .takes_value(true)
                .multiple_values(true)
                .last(true),
        )
}

pub fn execute(matches: &ArgMatches) -> Result<()> {
    let api = Api::current();

    let monitor = matches
        .value_of("monitor")
        .map(|x| x.parse::<Uuid>().unwrap())
        .unwrap();

    let allow_failure = matches.is_present("allow_failure");
    let args: Vec<_> = matches.values_of("args").unwrap().collect();

    let monitor_checkin = api.create_monitor_checkin(
        &monitor,
        &CreateMonitorCheckIn {
            status: MonitorStatus::InProgress,
        },
    );

    let started = Instant::now();
    let mut p = process::Command::new(args[0]);
    p.args(&args[1..]);
    let (success, code) = match p.status() {
        Ok(status) => (status.success(), status.code()),
        Err(err) => {
            eprintln!(
                "{} could not invoke program '{}': {}",
                style("error").red(),
                args[0],
                err
            );
            (false, None)
        }
    };

    match monitor_checkin {
        Ok(checkin) => {
            api.update_monitor_checkin(
                &monitor,
                &checkin.id,
                &UpdateMonitorCheckIn {
                    status: Some(if success {
                        MonitorStatus::Ok
                    } else {
                        MonitorStatus::Error
                    }),
                    duration: Some({
                        let elapsed = started.elapsed();
                        elapsed.as_secs() * 1000 + u64::from(elapsed.subsec_millis())
                    }),
                },
            )
            .ok();
        }
        Err(e) => {
            if allow_failure {
                print_error(&anyhow::Error::from(e));
            } else {
                return Err(e.into());
            }
        }
    }

    if !success {
        if let Some(code) = code {
            Err(QuietExit(code).into())
        } else {
            Err(QuietExit(1).into())
        }
    } else {
        Ok(())
    }
}
