// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: Copyright 2024 Ian McIntyre

use std::time::Duration;

use ccm_obs::Imxrt;
use clap::{error::ErrorKind, CommandFactory, Parser, ValueEnum};
use probe_rs::Permissions;

/// Query the CCM_OBS peripheral for root clock frequencies
#[derive(Parser)]
#[command(version, about, long_about)]
struct Cli {
    /// The i.MXRT MCU variant
    #[arg(ignore_case = true)]
    mcu: Mcu,

    /// The root clocks to observe
    ///
    /// If empty, observe all known root clocks. This is case insensitive.
    root_clocks: Vec<String>,

    /// List all available root clocks, then exit.
    #[arg(short, long)]
    list: bool,

    /// Delay (ms) before sampling frequencies
    #[arg(long, default_value = "100")]
    delay_ms: Option<u64>,

    /// Reset and halt the MCU before taking measurements.
    ///
    /// Useful for measuring the effects of the boot ROM based
    /// on its execution path.
    #[arg(long)]
    after_halted_reset: bool,
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "UPPER")]
enum Mcu {
    Imxrt1170,
    Imxrt1180,
}

impl Mcu {
    fn selection(&self) -> &'static Imxrt {
        match self {
            Self::Imxrt1170 => &ccm_obs::IMXRT1170,
            Self::Imxrt1180 => &ccm_obs::IMXRT1180,
        }
    }
    fn probe_rs_name(&self) -> &'static str {
        match self {
            Self::Imxrt1170 => "MIMXRT1170",
            Self::Imxrt1180 => "MIMXRT1189",
        }
    }
}

fn freq_to_str(freq: Option<u32>) -> String {
    freq.map(|f| f.to_string())
        .unwrap_or_else(|| String::from("???"))
}

fn main() {
    let cli = Cli::parse();
    let mcu = cli.mcu.selection();

    if cli.list {
        for name in mcu.all_root_clock_names() {
            println!("{name}");
        }
        return;
    }

    let names: Vec<_> = if cli.root_clocks.is_empty() {
        mcu.all_root_clock_names().collect()
    } else {
        let mut names = Vec::new();
        for user_selection in &cli.root_clocks {
            if let Some(name) = mcu.lookup_root_clock(user_selection) {
                names.push(name);
            } else {
                Cli::command()
                    .error(
                        ErrorKind::InvalidValue,
                        format!("Root clock named '{user_selection}' isn't known to this MCU."),
                    )
                    .exit();
            }
        }
        names
    };

    let mut session =
        match probe_rs::Session::auto_attach(cli.mcu.probe_rs_name(), Permissions::default()) {
            Ok(session) => session,
            Err(err) => Cli::command()
                .error(
                    ErrorKind::Io,
                    format!("{err} {err:?}\nIs your MCU connected to your debugger?"),
                )
                .exit(),
        };

    let mut core = session.core(0).unwrap();

    if cli.after_halted_reset {
        core.reset_and_halt(Duration::from_millis(200)).unwrap();
    }

    let delay = cli.delay_ms.map(Duration::from_millis).unwrap();

    let frequencies = mcu.observe_with_delay(&names, &mut core, delay).unwrap();

    println!(
        "{:>30} | {:>12} | {:>12} | {:>12} | {:>12}",
        "Name", "Current (Hz)", "Min (Hz)", "Max (Hz)", "Max-Min (Hz)"
    );
    let line: String = "-".repeat(30 + (4 * 3) + (12 * 4));
    println!("{line}");

    for (name, frequencies) in names.iter().zip(frequencies) {
        println!(
            "{:>30} | {:>12} | {:>12} | {:>12} | {:>12}",
            name,
            freq_to_str(frequencies.current()),
            freq_to_str(frequencies.min()),
            freq_to_str(frequencies.max()),
            freq_to_str(frequencies.diff())
        );
    }
}
