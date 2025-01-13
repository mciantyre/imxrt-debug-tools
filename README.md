A collection of tools to support i.MXRT MCU debugging.

# Getting started

You'll need a recent Rust installation to compile these tools. The latest stable
toolchain should be perfect.

Most of these packages build upon [probe-rs]. Therefore, using these tools
requires that you

1. [install] all of the *dependencies* required by probe-rs.
2. perform all [probe setup] for your host system.

[probe-rs]: https://probe.rs
[install]: https://probe.rs/docs/getting-started/installation/
[probe setup]: https://probe.rs/docs/getting-started/probe-setup/

These tools are not distributed on crates.io. Nevertheless, you can install them
using `cargo install`. All command line tools are implemented in the `tools`
package.

```
cargo install tools --git https://github.com/mciantyre/imxrt-debug-tools
```

The rest of this document summarizes the tools.

# `imxrt-ccm-obs`

The `imxrt-ccm-obs` command line tool uses your MCU's `CCM_OBS` peripheral block
to measure root clock frequencies. The tool works for the following MCUs:

- iMXRT1170
- iMXRT1180

To query all root clocks:

```
imxrt-ccm-obs imxrt1170
```

Once the MCU makes all observations, the tool renders the frequencies in a
table.

```
                          Name | Current (Hz) |     Min (Hz) |     Max (Hz) | Max-Min (Hz)
------------------------------------------------------------------------------------------
         BUS_CLK_LPSR_CLK_ROOT |     99692032 |     99660800 |     99713536 |        52736
                  BUS_CLK_ROOT |    199294976 |    199254016 |    199352320 |        98304
                ENET1_CLK_ROOT |     50003968 |     50003456 |     50003968 |          512
                ENET2_CLK_ROOT |     24001536 |     24001536 |     24002048 |          512
             ENET_25M_CLK_ROOT |     24002048 |     24001536 |     24002048 |          512
             ENET_QOS_CLK_ROOT |     24001536 |     24001536 |     24002048 |          512
          ENET_TIMER1_CLK_ROOT |     24002048 |     24001536 |     24002048 |          512
          ENET_TIMER2_CLK_ROOT |     24001536 |     24001536 |     24002048 |          512
          ENET_TIMER3_CLK_ROOT |     24001536 |     24001536 |     24002048 |          512
                   M4_CLK_ROOT |    199403520 |    199233536 |    199405056 |       171520
           M4_SYSTICK_CLK_ROOT |     23974912 |     23973376 |     23974912 |         1536
                   M7_CLK_ROOT |    398840320 |    398806528 |    398958592 |       152064
           M7_SYSTICK_CLK_ROOT |     23973888 |     23973376 |     23974400 |         1024
                   OSC_24M_OUT |     24001536 |     24001536 |     24002048 |          512
                   OSC_RC_400M |    398791680 |    398612992 |    398877184 |       264192
```

If you only want a subset of root clocks, you can name them:

```
imxrt-ccm-obs imxrt1170 bus_clk_root enet1_clk_root
```

For more information,

```
imxrt-ccm-obs --help
```

## Limitations

Consult your MCUs reference manual to understand the limitations of the
`CCM_OBS` peripheral block.

Check the project's issue tracker for other missing features and bugs.

# License

All packages are licensed MPL-2.0. See [LICENSE](./LICENSE) for more
information.
