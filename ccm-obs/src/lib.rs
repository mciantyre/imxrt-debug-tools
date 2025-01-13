// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: Copyright 2024 Ian McIntyre

//! Query the CCM_OBS peripheral on your i.MX RT MCU.
//!
//! Use this library to implement your own debugging tool that
//! queries the CCM_OBS peripheral. If you're looking for the
//! command line tool, that's provided in a separate package.

use probe_rs::MemoryInterface;
use std::{collections::BTreeMap, sync::LazyLock, time::Duration};

/// A CCM_OBS root clock identifier.
///
/// These are named and exposed through [`RootClocks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootClock {
    select_index: u32,
    slice_number: u64,
}

impl RootClock {
    /// Define a new root clock.
    ///
    /// The implementation performs no runtime checks to make sure this
    /// is valid. Use this only if your root clock isn't available.
    pub const fn new(select_index: u32, slice_number: u64) -> Self {
        Self {
            select_index,
            slice_number,
        }
    }

    /// Returns the select index for this root clock.
    pub const fn select_index(&self) -> u32 {
        self.select_index
    }

    /// Returns the slice number for this root clock.
    pub const fn slice_number(&self) -> u64 {
        self.slice_number
    }
}

/// A collection of root clocks.
///
/// You can obtain these using [`Imxrt::root_clocks`]. Note that
/// an [`Imxrt`] MCU may not have implemented all root clocks. If
/// that's the case, either contribute to the library, or construct
/// the root clock yourself; see [`RootClock::new`].
///
///
/// Root clocks are identified by `SCREAMING_SNAKE_CASE`, just
/// as they're named in the reference manual.
pub type RootClocks = BTreeMap<String, RootClock>;

/// A valid root clock name.
///
/// If you obtain one of these, you can infallibly retrieve the root
/// clock using [`get`](Imxrt::get). You can obtain one of these using
/// the methods on [`Imxrt`].
#[derive(Debug, Clone, Copy)]
pub struct RootClockName<'a>(&'a String);

impl std::fmt::Display for RootClockName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An i.MX RT MCU implementation.
///
/// These are exposed as `static`s of the library.
/// Use these to search for root clocks and perform
/// clock observations.
pub struct Imxrt {
    /// Starting address of the CCM_OBS block.
    ///
    /// For the 1170, this is defined as a separate peripheral
    /// block. For the 1180, point this at the zeroth control
    /// register (there's only one slice, apparently?).
    ccm_obs: u64,
    root_clocks: RootClocks,
}

impl Imxrt {
    /// Returns the collection of all clock names.
    pub fn all_root_clock_names(&self) -> impl Iterator<Item = RootClockName<'_>> {
        self.root_clocks.keys().map(RootClockName)
    }

    /// Try to normalize the name, returning a key if known.
    ///
    /// This helps out users who don't want to use all uppercase names,
    /// or who might forget suffixes.
    pub fn lookup_root_clock(&self, name: &str) -> Option<RootClockName<'_>> {
        let name = name.to_uppercase();

        if let Some((key, _)) = self.root_clocks.get_key_value(&name) {
            return Some(RootClockName(key));
        }

        if !name.ends_with("_CLK_ROOT") {
            if let Some((key, _)) = self.root_clocks.get_key_value(&format!("{name}_CLK_ROOT")) {
                return Some(RootClockName(key));
            }
        }

        if !name.ends_with("_OUT") {
            if let Some((key, _)) = self.root_clocks.get_key_value(&format!("{name}_OUT")) {
                return Some(RootClockName(key));
            }
        }

        None
    }

    fn get(&self, name: RootClockName) -> &RootClock {
        self.root_clocks
            .get(name.0.as_str())
            .expect("All RootClockNames are valid")
    }

    /// Observe a root clock's frequencies.
    ///
    /// Manipulates the CCM_OBS registers to measure the
    /// given `root_clock` frequencies. It delays 100ms
    /// before it starts polling for the frequencies.
    ///
    /// Use [`get`](Self::get) to create the collection
    /// of root clocks.
    pub fn observe(
        &self,
        root_clocks: &[RootClockName],
        mem: &mut dyn MemoryInterface,
    ) -> Result<Vec<Frequencies>, Error> {
        self.observe_with_delay(root_clocks, mem, Duration::from_millis(100))
    }

    /// Observe a root clock's frequencies with configurable delay.
    ///
    /// This is the same as [`observe`](Self::observe), and it gives
    /// you the change to change the delay before sampling. The
    /// implementation enforces a minimum delay of 20ms.
    pub fn observe_with_delay(
        &self,
        root_clocks: &[RootClockName],
        mem: &mut dyn MemoryInterface,
        delay: Duration,
    ) -> Result<Vec<Frequencies>, Error> {
        let delay = delay.max(Duration::from_millis(20));
        root_clocks
            .iter()
            .map(|root_clock| {
                let root_clock = self.get(*root_clock);
                let slice = CcmObsSlice::for_root_clock(self.ccm_obs, root_clock);

                // Direct write to control register zeros all other bits.
                const OFF: u32 = 1 << 24;
                mem.write_word_32(slice.control(), OFF)
                    .map_err(context("turning off the slice"))?;

                // Include the RESET bit, maintain OFF bit.
                const RESET: u32 = 1 << 15;
                mem.write_word_32(slice.control_set(), RESET)
                    .map_err(context("resetting the slice"))?;

                /// The divider used before clock sampling.
                ///
                /// We need to divide clocks below 400MHz. This divider is large
                /// enough to work with a 3.2GHz clock, which isn't expected to
                /// exist on i.MX RT MCUs.
                const DIVIDER: u32 = 8;
                const fn divider_field() -> u32 {
                    (DIVIDER - 1) << 16
                }

                // Update the root select and the divider while keeping
                // RESET and OFF.
                mem.write_word_32(
                    slice.control(),
                    OFF | RESET | divider_field() | root_clock.select_index,
                )
                .map_err(context("setting the divider and root select"))?;

                // Clear the OFF and RESET to begin sampling.
                mem.write_word_32(slice.control_clr(), OFF | RESET)
                    .map_err(context("starting to sample"))?;

                // Force the probe to dispatch writes to the MCU.
                mem.flush()
                    .map_err(context("flushing commands to the MCU"))?;

                // Wait for completion.
                std::thread::sleep(delay);

                let mut freqs = [0u32; 3];
                mem.read_32(slice.frequency_current(), &mut freqs)
                    .map_err(context("sampling frequencies"))?;

                // We're done; turn off the slice.
                mem.write_word_32(slice.control(), OFF)
                    .map_err(context("turning off the slice"))?;

                mem.flush()
                    .map_err(context("flushing cleanup to the MCU"))?;

                Ok(Frequencies {
                    raw_current: freqs[0],
                    raw_min: freqs[1],
                    raw_max: freqs[2],
                    divider: DIVIDER,
                })
            })
            .collect()
    }
}

fn root_clock(name: &'static str, select_index: u32, slice_number: u64) -> (String, RootClock) {
    (name.into(), RootClock::new(select_index, slice_number))
}

/// Provides access to the CCM_OBS on 1170 MCUs.
///
/// See [`Imxrt`] for more information.
pub static IMXRT1170: LazyLock<Imxrt> = LazyLock::new(|| {
    let root_clocks = vec![
        root_clock("M7_CLK_ROOT", 128, 4),
        root_clock("M4_CLK_ROOT", 129, 0),
        root_clock("BUS_CLK_ROOT", 130, 2),
        root_clock("BUS_CLK_LPSR_CLK_ROOT", 131, 0),
        root_clock("M4_SYSTICK_CLK_ROOT", 135, 0),
        root_clock("M7_SYSTICK_CLK_ROOT", 136, 2),
        root_clock("ENET1_CLK_ROOT", 179, 2),
        root_clock("ENET2_CLK_ROOT", 180, 2),
        root_clock("ENET_QOS_CLK_ROOT", 181, 2),
        root_clock("ENET_25M_CLK_ROOT", 182, 2),
        root_clock("ENET_TIMER1_CLK_ROOT", 183, 2),
        root_clock("ENET_TIMER2_CLK_ROOT", 184, 2),
        root_clock("ENET_TIMER3_CLK_ROOT", 185, 2),
        root_clock("OSC_RC_400M", 227, 0),
        root_clock("OSC_24M_OUT", 229, 0),
    ]
    .into_iter()
    .collect();

    Imxrt {
        ccm_obs: 0x4015_0000,
        root_clocks,
    }
});

/// Provides access to the `CCM_OBS` on the 1180 MCUs.
///
/// See [`Imxrt`] for more information.
pub static IMXRT1180: LazyLock<Imxrt> = LazyLock::new(|| {
    let root_clocks = vec![
        root_clock("OSC_RC_24M", 2, 0),
        root_clock("OSC_RC_400M", 3, 0),
        root_clock("OSC_24M_OUT", 5, 0),
        root_clock("PLL_480_OUT", 15, 0),
        root_clock("PLL_480_DIV2", 16, 0),
        root_clock("PLL_480_PFD0", 17, 0),
        root_clock("PLL_480_PFD1", 18, 0),
        root_clock("PLL_480_PFD2", 19, 0),
        root_clock("PLL_480_PFD3", 20, 0),
        root_clock("M33_CLK_ROOT", 129, 0),
        root_clock("FLEXSPI1_CLK_ROOT", 149, 0),
    ]
    .into_iter()
    .collect();

    Imxrt {
        ccm_obs: 0x4445_0000 + 0x4400,
        root_clocks,
    }
});

/// Frequency measurements provided by the CCM_OBS peripheral block.
///
/// You may access the current, minimum, and maximum frequencies (Hz)
/// using [`current`](Self::current), [`min`](Self::min), and [`max`](Self::max)
/// respectively. These values are scaled by a divider, set as an implementation
/// detail. If the frequency overflows, the return is `None`.
///
/// The raw readings are exposed via the `raw_*` accessors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frequencies {
    raw_current: u32,
    raw_min: u32,
    raw_max: u32,
    divider: u32,
}

impl Frequencies {
    /// Return the current frequency measurement, in Hz.
    ///
    /// Returns `None` if multiplication with the divider
    /// occurred.
    pub const fn current(&self) -> Option<u32> {
        self.raw_current.checked_mul(self.divider)
    }
    /// Return the minimum frequency observed, in Hz.
    ///
    /// Returns `None` if multiplication with the divider
    /// occurred.
    pub const fn min(&self) -> Option<u32> {
        self.raw_min.checked_mul(self.divider)
    }
    /// Return the maximum frequency observed, in Hz.
    ///
    /// Returns `None` if multiplication with the divider
    /// occurred.
    pub const fn max(&self) -> Option<u32> {
        self.raw_max.checked_mul(self.divider)
    }

    /// Compute the difference in max and min.
    pub fn diff(&self) -> Option<u32> {
        let max = self.max()?;
        let min = self.min()?;
        Some(max.saturating_sub(min))
    }

    /// Return the raw measurement observed by the peripheral.
    pub const fn raw_current(&self) -> u32 {
        self.raw_current
    }
    /// Return the raw minimum observed by the peripheral.
    pub const fn raw_min(&self) -> u32 {
        self.raw_min
    }
    /// Return the raw maximum observed by the peripheral.
    pub const fn raw_max(&self) -> u32 {
        self.raw_max
    }
}

/// An error during observation.
pub type Error = Box<dyn std::error::Error>;

fn context<E: std::error::Error + 'static>(what: &'static str) -> impl FnOnce(E) -> Error {
    #[derive(Debug)]
    struct ErrorContext {
        what: &'static str,
        extra: Error,
    }

    impl std::fmt::Display for ErrorContext {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "{}", self.what)
        }
    }

    impl std::error::Error for ErrorContext {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&*self.extra)
        }
    }

    move |extra| {
        Box::new(ErrorContext {
            what,
            extra: extra.into(),
        })
    }
}

/// A slice of the CCM_OBS peripheral.
///
/// Used to access individual registers of the slice.
#[derive(Debug, Clone, Copy)]
struct CcmObsSlice(u64);

impl CcmObsSlice {
    fn for_root_clock(peripheral_address: u64, root_clock: &RootClock) -> Self {
        Self(peripheral_address + (root_clock.slice_number * 0x80))
    }
    fn control(self) -> u64 {
        self.0
    }
    fn control_set(self) -> u64 {
        self.0 + 0x4
    }
    fn control_clr(self) -> u64 {
        self.0 + 0x8
    }
    fn frequency_current(self) -> u64 {
        self.0 + 0x40
    }
}
